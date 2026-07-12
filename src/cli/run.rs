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
use crate::sandbox::SandboxBackend;
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

    /// Sandbox backend: docker (container), worktree (git worktree + local process,
    /// no Docker), or cloud (NIKI infra, beta). Overrides [docker] backend in config.
    #[arg(long, value_enum)]
    pub backend: Option<BackendArg>,

    /// Shortcut for `--backend cloud` — run the pipeline on NIKI's cloud infra (beta).
    #[arg(long)]
    pub cloud: bool,

    /// Run the Planner only and show the spec without executing
    #[arg(long)]
    pub dry_run: bool,

    /// Minimal output — no streaming, just final report
    #[arg(long)]
    pub quiet: bool,

    /// Show full agent reasoning (not just summaries)
    #[arg(long)]
    pub verbose: bool,

    /// Render a rich terminal TUI (panels per agent stage) instead of the
    /// inline streaming view. Requires a TTY; ignored when piped.
    #[arg(long)]
    pub tui: bool,
}

/// CLI spelling of the sandbox backend; maps onto [`crate::sandbox::SandboxBackend`].
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum BackendArg {
    Docker,
    Worktree,
    Cloud,
}

impl From<BackendArg> for crate::sandbox::SandboxBackend {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Docker => crate::sandbox::SandboxBackend::Docker,
            BackendArg::Worktree => crate::sandbox::SandboxBackend::Worktree,
            BackendArg::Cloud => crate::sandbox::SandboxBackend::Cloud,
        }
    }
}

fn role_filename(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
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

    // Resolve the sandbox backend: explicit --backend / --cloud wins, otherwise
    // fall back to [docker] backend in config (default: docker).
    let backend = if args.cloud {
        SandboxBackend::Cloud
    } else if let Some(b) = args.backend {
        b.into()
    } else {
        config.docker.backend
    };
    config.docker.backend = backend;

    // Cloud execution is a beta scaffold: the trait seam exists so the
    // orchestrator can target NIKI infra unchanged, but the real remote executor
    // needs infra + credentials that aren't part of a local build. Fail fast here
    // with a clear message rather than burning Planner tokens and erroring
    // mid-pipeline. The `NIKI_CLOUD_ENDPOINT` env var is the seam: when a future
    // build wires up a real endpoint it can bypass this guard.
    if matches!(backend, SandboxBackend::Cloud) && env::var("NIKI_CLOUD_ENDPOINT").is_err() {
        eprintln!(
            "Cloud execution (beta) is not available in this build.\n\
             The architecture supports it — the `cloud` sandbox backend implements the same\n\
             trait as Docker/worktree — but running agents on NIKI infrastructure requires\n\
             infra + credentials that ship separately.\n\n\
             To run locally without Docker, use:  niki run \"<task>\" --backend worktree"
        );
        std::process::exit(2);
    }

    let uses_docker = matches!(backend, SandboxBackend::Docker);

    let task = Task {
        id: Uuid::new_v4(),
        description: args.description.clone(),
        project_path: project_dir.clone(),
    };

    let mut display = AgenticDisplay::new();

    // Opt-in rich TUI. Must be enabled before any display call so the banner
    // and subsequent events are routed to the render thread.
    if args.tui {
        display.enable_tui(task.description.clone());
    }

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
                eprintln!("\n Shutting down — cleaning up...");

                let ids = containers.lock().await.clone();
                if !ids.is_empty() {
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
                }

                // Persist a cancelled task record so status commands reflect reality.
                let mut rec = TaskRecord::new(
                    uuid::Uuid::parse_str(&task_id_str).unwrap_or_default(),
                    "",
                );
                rec.status = TaskStatus::Cancelled;
                let _ = rec.save_to_disk(&task_dir);

                eprintln!(" Partial results saved under ./{}/tasks/", output_dir);
                // 130 = 128 + SIGINT(2), the conventional exit code for Ctrl+C.
                // Lets CI/scripts distinguish an interrupt from a generic failure.
                std::process::exit(130);
            }
        });
    }

    // Only connect to the Docker daemon when the chosen backend needs it. The
    // worktree and cloud backends never touch Docker, so they run without a daemon.
    // The dry-run path also skips the daemon ping (it never creates a sandbox).
    let docker = if uses_docker && !args.dry_run {
        let d = Docker::connect_with_local_defaults()
            .map_err(|e| anyhow!("Docker error: {}", e))?;
        d.ping().await.map_err(|_| NikiError::DockerNotRunning)?;
        Some(d)
    } else {
        None
    };

    // Borrow the connection for the pipeline; None for non-Docker backends.
    let docker_ref = docker.as_ref();

    // Persist an initial "running" record.
    let mut record = TaskRecord::new(task.id, &task.description);
    if let Err(e) = record.save_to_disk(&task_dir) {
        eprintln!("Warning: could not save task state: {}", e);
    }

    let result = match execute_pipeline(
        &task,
        &config,
        docker_ref,
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
            display.finish_tui();
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

    // Generate the static HTML dashboard (diff viewer + annotations).
    {
        let find_artifact = |role: AgentRole| -> Option<String> {
            result
                .artifacts
                .iter()
                .find(|(r, _)| *r == role)
                .map(|(_, j)| j.clone())
        };
        let review_json = find_artifact(AgentRole::Reviewer);
        let security_json = find_artifact(AgentRole::SecurityAuditor);

        let total_in: u32 = result.metrics.iter().map(|m| m.input_tokens).sum();
        let total_out: u32 = result.metrics.iter().map(|m| m.output_tokens).sum();
        let total_cost: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
        let total_ms: u64 = result.metrics.iter().map(|m| m.latency_ms).sum();
        let metrics_rows = vec![
            ("Agents run".to_string(), result.metrics.len().to_string()),
            ("Input tokens".to_string(), total_in.to_string()),
            ("Output tokens".to_string(), total_out.to_string()),
            ("Latency".to_string(), format!("{:.1}s", total_ms as f64 / 1000.0)),
            (
                "Est. cost".to_string(),
                if total_cost > 0.0 {
                    format!("${:.4}", total_cost)
                } else {
                    "n/a".to_string()
                },
            ),
        ];

        let input = crate::output::dashboard::DashboardInput {
            task_id: &task.id.to_string(),
            description: &task.description,
            verdict: &format!("{:?}", result.verdict),
            revision_rounds: result.revision_rounds,
            final_diff: &result.final_diff,
            review_json: review_json.as_deref(),
            security_json: security_json.as_deref(),
            metrics_rows,
        };
        if let Err(e) = crate::output::dashboard::write_dashboard(&task_dir, &input) {
            eprintln!("Warning: could not generate dashboard: {}", e);
        }
    }

    // Generate the patch file.
    if let Err(e) =
        crate::output::patch::generate_patch(&result.final_diff, &task_dir.join("changes.patch"))
    {
        eprintln!("Failed to generate patch: {}", e);
    }

    // For non-Docker backends the change still lives inside the sandbox copy (a
    // separate git worktree or a cloud VM), so `working_tree_diff` on the host
    // would be empty. Apply the sandbox's diff to the host working tree first; the
    // Docker backend already wrote through the bind mount and skips this step.
    if !uses_docker && !result.final_diff.trim().is_empty() {
        if let Err(e) =
            crate::output::git::apply_diff_to_working_tree(&project_dir, &result.final_diff)
        {
            eprintln!("Warning: could not apply sandbox diff to host: {}", e);
        }
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
    record.add_metrics(&result.metrics);
    if let Err(e) = record.save_to_disk(&task_dir) {
        eprintln!("Warning: could not save final task state: {}", e);
    }

    if !args.quiet {
        display.show_completion(&result, &branch_name, &task_dir);
    }

    // Tear down the TUI (if active): this joins the render thread, which
    // restores the terminal before any further output.
    display.finish_tui();

    Ok(())
}
