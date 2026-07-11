use anyhow::{anyhow, Result};
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use std::env;
use tokio::sync::Mutex;
use tokio::signal;
use bollard::Docker;
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::orchestrator::pipeline::{execute_pipeline, Task};
use crate::orchestrator::state::{TaskRecord, TaskStatus};
use crate::sandbox::docker::ActiveContainers;
use crate::artifacts::types::AgentRole;
use crate::NikiError;

#[derive(Args)]
pub struct RunArgs {
    /// Natural language description of the task
    pub description: String,

    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,

    /// Name for the output branch (default: niki/{task_id_short})
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Override max revision rounds (default: from config)
    #[arg(long)]
    pub max_rounds: Option<u32>,

    /// Override planner model
    #[arg(long)]
    pub planner_model: Option<String>,

    /// Override coder model
    #[arg(long)]
    pub coder_model: Option<String>,

    /// Override tester model
    #[arg(long)]
    pub tester_model: Option<String>,

    /// Override reviewer model
    #[arg(long)]
    pub reviewer_model: Option<String>,

    /// Run the Planner only and show the spec without executing
    #[arg(long)]
    pub dry_run: bool,

    /// Minimal output — no streaming, just final report
    #[arg(long)]
    pub quiet: bool,

    /// Show full agent reasoning (not just summaries)
    #[arg(long)]
    pub verbose: bool,
}

fn role_filename(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
    }
}

pub async fn handle(args: &RunArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.canonicalize()?,
        None => env::current_dir()?,
    };

    let mut config = NikiConfig::load(&project_dir)?;

    if let Some(r) = args.max_rounds {
        config.general.max_revision_rounds = r;
    }
    if let Some(ref m) = args.planner_model {
        config.agents.planner.model = m.clone();
    }
    if let Some(ref m) = args.coder_model {
        config.agents.coder.model = m.clone();
    }
    if let Some(ref m) = args.tester_model {
        config.agents.tester.model = m.clone();
    }
    if let Some(ref m) = args.reviewer_model {
        config.agents.reviewer.model = m.clone();
    }

    let task = Task {
        id: Uuid::new_v4(),
        description: args.description.clone(),
        project_path: project_dir.clone(),
    };

    let mut display = AgenticDisplay::new();

    if !args.quiet {
        display.show_banner(&task, &config);
    }

    // Resolve output locations up front so the Ctrl+C handler can persist state.
    let task_dir = project_dir
        .join(&config.general.output_dir)
        .join("tasks")
        .join(task.id.to_string());

    // Track containers so a Ctrl+C handler can tear them down cleanly.
    let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));

    {
        let containers = containers.clone();
        let task_dir = task_dir.clone();
        let task_id_str = task.id.to_string();
        let output_dir = config.general.output_dir.clone();
        tokio::spawn(async move {
            if signal::ctrl_c().await.is_ok() {
                eprintln!("\n Shutting down — cleaning up Docker containers...");

                let ids = containers.lock().await.clone();
                match Docker::connect_with_local_defaults() {
                    Ok(docker) => {
                        for id in ids {
                            // force:true stops the container if still running, then removes it.
                            let _ = docker
                                .remove_container(
                                    &id,
                                    Some(bollard::container::RemoveContainerOptions {
                                        force: true,
                                        ..Default::default()
                                    }),
                                )
                                .await;
                        }
                    }
                    Err(_) => {}
                }

                // Persist a cancelled task record so status commands reflect reality.
                let mut rec = TaskRecord::new(
                    uuid::Uuid::parse_str(&task_id_str).unwrap_or_default(),
                    "",
                );
                rec.status = TaskStatus::Cancelled;
                let _ = rec.save_to_disk(&task_dir);

                eprintln!(" Containers cleaned up. Partial results saved under ./{}/tasks/", output_dir);
                std::process::exit(1);
            }
        });
    }

    // Fail fast with a friendly message if Docker isn't reachable. The dry-run path
    // never touches the sandbox, so it skips the daemon ping and works without Docker.
    let docker = Docker::connect_with_local_defaults()
        .map_err(|e| anyhow!("Docker error: {}", e))?;
    if !args.dry_run {
        docker.ping().await.map_err(|_| NikiError::DockerNotRunning)?;
    }

    // Persist an initial "running" record.
    let mut record = TaskRecord::new(task.id, &task.description);
    if let Err(e) = record.save_to_disk(&task_dir) {
        eprintln!("Warning: could not save task state: {}", e);
    }

    let result = match execute_pipeline(
        &task,
        &config,
        &docker,
        &mut display,
        containers.clone(),
        args.dry_run,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let mut rec = TaskRecord::new(task.id, &task.description);
            rec.status = TaskStatus::Failed {
                error: e.to_string(),
            };
            let _ = rec.save_to_disk(&task_dir);
            return Err(e);
        }
    };

    let branch_name = args
        .branch
        .clone()
        .unwrap_or_else(|| format!("niki/{}", &task.id.to_string()[..8]));

    // Save raw agent artifacts.
    let artifacts_dir = task_dir.join("artifacts");
    if let Err(e) = std::fs::create_dir_all(&artifacts_dir) {
        eprintln!("Warning: could not create artifacts dir: {}", e);
    } else {
        for (role, json) in &result.artifacts {
            let path = artifacts_dir.join(format!("{}.json", role_filename(*role)));
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("Warning: could not save artifact {:?}: {}", role, e);
            }
        }
    }

    // Generate the markdown report.
    if let Err(e) = crate::output::report::generate_report(&task, &config, &result) {
        eprintln!("Warning: could not generate report: {}", e);
    }

    // Generate the patch file.
    if let Err(e) =
        crate::output::patch::generate_patch(&result.final_diff, &task_dir.join("changes.patch"))
    {
        eprintln!("Failed to generate patch: {}", e);
    }

    // Create the git branch + commit (no-op when there is no diff).
    if let Err(e) = crate::output::git::create_branch_and_commit(
        &project_dir,
        &branch_name,
        &result.final_diff,
        &task.id.to_string(),
    ) {
        eprintln!("Warning: git branch/commit failed: {}", e);
    }

    // Persist final task record.
    record.status = TaskStatus::Completed;
    record.branch = Some(branch_name.clone());
    record.verdict = Some(format!("{:?}", result.verdict));
    record.revision_rounds = result.revision_rounds;
    if let Err(e) = record.save_to_disk(&task_dir) {
        eprintln!("Warning: could not save final task state: {}", e);
    }

    if !args.quiet {
        display.show_completion(&result, &branch_name, &task_dir);
    }

    Ok(())
}
