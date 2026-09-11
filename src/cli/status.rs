use crate::config::NikiConfig;
use crate::orchestrator::state::TaskRecord;
use anyhow::Result;
use clap::Args;
use std::env;
use std::path::PathBuf;

#[derive(Args)]
pub struct StatusArgs {
    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
    /// Show the run provenance manifest (repo snapshot, config hash, costs)
    /// for the task: the latest, or TASK_ID when given.
    #[arg(long)]
    pub with_provenance: bool,
    /// Task id (full UUID or unique short prefix). Defaults to the latest task.
    pub task_id: Option<String>,
}

pub async fn handle(args: &StatusArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");

    let mut latest: Option<(PathBuf, TaskRecord)> = None;
    // A task id (full UUID or unique short prefix) selects one task dir;
    // otherwise the latest task wins.
    let only_dir: Option<PathBuf> = match &args.task_id {
        Some(id) => Some(tasks_dir.join(crate::cli::report::resolve_task_id(&tasks_dir, id)?)),
        None => None,
    };
    if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            if let Some(only) = &only_dir
                && entry.path() != *only
            {
                continue;
            }
            let path = entry.path().join("task.json");
            if let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(record) = serde_json::from_str::<TaskRecord>(&content)
            {
                let newer = match &latest {
                    Some((_, l)) => record.created_at > l.created_at,
                    None => true,
                };
                if newer {
                    latest = Some((entry.path(), record));
                }
            }
        }
    }

    match latest {
        Some((dir, record)) => {
            println!("Task:       {}", record.task_id);
            println!("Description: {}", record.description);
            println!("Status:     {}", record.status);
            if let Some(branch) = &record.branch {
                println!("Branch:     {}", branch);
            }
            if let Some(verdict) = &record.verdict {
                println!("Verdict:    {}", verdict);
            }
            println!("Revisions:  {}", record.revision_rounds);
            println!("Artifacts:  {}", dir.join("artifacts").display());
            println!("Report:     {}", dir.join("report.md").display());
            if args.with_provenance {
                print_provenance(&dir);
            }
        }
        None => {
            println!(
                "No tasks found in {}. Run `niki run \"<task>\"` to create one (or `niki plan \"<task>\"` to review a plan first).",
                tasks_dir.display()
            );
        }
    }

    Ok(())
}

/// Render the run provenance manifest (`manifest.json`) for a task dir.
/// Missing/unreadable manifests print a one-line note, never an error.
fn print_provenance(task_dir: &std::path::Path) {
    let manifest = match crate::orchestrator::provenance::read_manifest(task_dir) {
        Ok(m) => m,
        Err(_) => {
            println!(
                "Provenance: (no manifest.json — run predates provenance or snapshots are disabled)"
            );
            return;
        }
    };
    println!("Provenance:");
    println!("  Snapshot:   {}", manifest.active_snapshot.snapshot_id);
    println!(
        "  Repo HEAD:  {}",
        manifest
            .active_snapshot
            .commit_sha
            .as_deref()
            .unwrap_or("(not a git repo)")
    );
    println!("  Tree:       {}", manifest.active_snapshot.kind);
    if let Some(branch) = &manifest.repo_identity.branch {
        println!("  On branch:  {}", branch);
    }
    if let Some(url) = &manifest.repo_identity.remote_url {
        println!("  Remote:     {}", url);
    }
    if let Some(fp) = &manifest.repo_identity.workdir_fingerprint {
        println!("  Workdir fp: {}", fp);
    }
    match &manifest.config_fingerprint.content_hash {
        Some(hash) => println!(
            "  Config:     {} (#{})",
            manifest
                .config_fingerprint
                .path
                .as_deref()
                .unwrap_or("niki.toml"),
            hash
        ),
        None => println!("  Config:     (no local niki.toml)"),
    }
    println!(
        "  Toolchain:  niki {} / {}",
        manifest.toolchain.niki,
        manifest
            .toolchain
            .rustc
            .as_deref()
            .unwrap_or("(rustc unknown)")
    );
    if !manifest.agent_roles.is_empty() {
        println!(
            "  Stages:     {}",
            manifest
                .agent_roles
                .iter()
                .map(|r| format!("{r:?}"))
                .collect::<Vec<_>>()
                .join(" → ")
        );
    }
    if let Some(branch) = &manifest.branch {
        println!("  Result:     branch {branch}");
    }
    if manifest.total_cost_usd > 0.0 {
        println!("  Cost:       ${:.4}", manifest.total_cost_usd);
    }
    if manifest.dry_run {
        println!("  (dry run — no branch by design)");
    }
}
