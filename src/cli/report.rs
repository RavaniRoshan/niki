use anyhow::Result;
use clap::Args;
use std::env;
use std::path::{Path, PathBuf};
use crate::config::NikiConfig;
use crate::orchestrator::state::TaskRecord;

#[derive(Args)]
pub struct ReportArgs {
    /// Task ID to view the report for — full UUID or a unique short prefix
    /// (e.g. `09c71e4b`). Defaults to the most recent task.
    pub task_id: Option<String>,

    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
}

pub async fn handle(args: &ReportArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");

    let task_id = match &args.task_id {
        Some(id) => match resolve_task_id(&tasks_dir, id) {
            Ok(resolved) => resolved,
            Err(e) => {
                eprintln!("{e}");
                return Ok(());
            }
        },
        None => match latest_task_id(&tasks_dir) {
            Some(id) => id,
            None => {
                eprintln!("No tasks found in {}", tasks_dir.display());
                return Ok(());
            }
        },
    };

    let report_path = tasks_dir.join(&task_id).join("report.md");
    match std::fs::read_to_string(&report_path) {
        Ok(content) => print!("{}", content),
        Err(_) => eprintln!("Report not found: {}", report_path.display()),
    }

    Ok(())
}

/// Resolve a user-supplied task id (full UUID or a short prefix) to a concrete
/// task directory name. Errors if nothing matches or the prefix is ambiguous.
fn resolve_task_id(tasks_dir: &Path, input: &str) -> Result<String> {
    // Exact directory match (full UUID) wins immediately.
    if tasks_dir.join(input).join("task.json").is_file() {
        return Ok(input.to_string());
    }

    // Otherwise treat the input as a prefix over task directory names.
    let mut matches: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(tasks_dir) {
        for entry in entries.flatten() {
            if !entry.path().join("task.json").is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(input) {
                    matches.push(name.to_string());
                }
            }
        }
    }

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(anyhow::anyhow!(
            "No task matching '{}' found in {}",
            input,
            tasks_dir.display()
        )),
        _ => {
            matches.sort();
            Err(anyhow::anyhow!(
                "Task id '{}' is ambiguous — matches: {}",
                input,
                matches.join(", ")
            ))
        }
    }
}

/// Find the most recently created task's directory name.
fn latest_task_id(tasks_dir: &Path) -> Option<String> {
    let mut latest: Option<(String, chrono::DateTime<chrono::Utc>)> = None;
    if let Ok(entries) = std::fs::read_dir(tasks_dir) {
        for entry in entries.flatten() {
            let path = entry.path().join("task.json");
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(record) = serde_json::from_str::<TaskRecord>(&content) {
                    let newer = match &latest {
                        Some((_, t)) => record.created_at > *t,
                        None => true,
                    };
                    if newer {
                        latest = Some((record.task_id.to_string(), record.created_at));
                    }
                }
            }
        }
    }
    latest.map(|(id, _)| id)
}
