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

    // A status command that prints a failed run and exits 0 is a command a
    // script cannot use: `niki status && deploy` deploys after a failure.
    // This is the same defect `niki report` had, in the sibling command.
    let now = chrono::Utc::now();
    let failed_reason = latest.as_ref().and_then(|(_, r)| {
        if r.is_stale_running(now) {
            return Some(format!(
                "the run stopped without recording an outcome (last progress: {}). \
                 It was killed rather than finishing — nothing was committed.",
                r.last_update
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ));
        }
        match &r.status {
            crate::orchestrator::state::TaskStatus::Failed { error } => Some(error.clone()),
            crate::orchestrator::state::TaskStatus::Cancelled => {
                Some("the run was cancelled".to_string())
            }
            _ => None,
        }
    });

    match latest {
        Some((dir, record)) => {
            println!("Task:       {}", record.task_id);
            println!("Description: {}", record.description);
            // A run whose process stopped writing is not running, however the
            // record's own status field still reads.
            let status_line = if record.is_stale_running(now) {
                "interrupted — the process ended without recording an outcome".to_string()
            } else {
                record.status.to_string()
            };
            println!("Status:     {status_line}");
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

    if let Some(reason) = failed_reason {
        anyhow::bail!("the most recent task did not succeed: {reason}");
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
