use anyhow::{anyhow, Result};
use clap::Args;
use std::env;
use std::path::PathBuf;

use crate::config::NikiConfig;
use crate::orchestrator::state::TaskRecord;
use crate::output::dashboard::{write_dashboard, DashboardInput};

#[derive(Args)]
pub struct DashboardArgs {
    /// Task ID to build the dashboard for (default: most recent task).
    #[arg(long)]
    pub task: Option<String>,

    /// Path to the project (default: current directory).
    #[arg(short, long)]
    pub project: Option<PathBuf>,

    /// Print the dashboard path only (do not regenerate).
    #[arg(long)]
    pub path_only: bool,
}

/// Locate the task directory: an explicit `--task` id, or the most recent task
/// under `.niki/tasks/`.
fn find_task_dir(tasks_dir: &std::path::Path, task: &Option<String>) -> Result<(PathBuf, TaskRecord)> {
    if let Some(id) = task {
        let dir = tasks_dir.join(id);
        let record_path = dir.join("task.json");
        let content = std::fs::read_to_string(&record_path)
            .map_err(|_| anyhow!("No task '{}' under {}", id, tasks_dir.display()))?;
        let record: TaskRecord = serde_json::from_str(&content)?;
        return Ok((dir, record));
    }

    let mut latest: Option<(PathBuf, TaskRecord)> = None;
    if let Ok(entries) = std::fs::read_dir(tasks_dir) {
        for entry in entries.flatten() {
            let path = entry.path().join("task.json");
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(record) = serde_json::from_str::<TaskRecord>(&content) {
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
    }
    latest.ok_or_else(|| anyhow!("No tasks found in {}", tasks_dir.display()))
}

pub fn handle(args: &DashboardArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");

    let (dir, record) = find_task_dir(&tasks_dir, &args.task)?;
    let dashboard_path = dir.join("dashboard.html");

    if args.path_only {
        println!("{}", dashboard_path.display());
        return Ok(());
    }

    // Rebuild from persisted artifacts so the dashboard reflects the latest
    // on-disk state even if it was never generated (older runs).
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    let final_diff = read("changes.patch").unwrap_or_default();
    let review_json = std::fs::read_to_string(dir.join("artifacts").join("reviewer.json")).ok();
    let security_json =
        std::fs::read_to_string(dir.join("artifacts").join("security_auditor.json")).ok();

    let m = &record;
    let metrics_rows = vec![
        ("Agents run".to_string(), m.agent_metrics.len().to_string()),
        ("Input tokens".to_string(), m.total_input_tokens.to_string()),
        ("Output tokens".to_string(), m.total_output_tokens.to_string()),
        (
            "Latency".to_string(),
            format!("{:.1}s", m.total_latency_ms as f64 / 1000.0),
        ),
        (
            "Est. cost".to_string(),
            if m.total_cost_usd > 0.0 {
                format!("${:.4}", m.total_cost_usd)
            } else {
                "n/a".to_string()
            },
        ),
    ];

    let input = DashboardInput {
        task_id: &record.task_id.to_string(),
        description: &record.description,
        verdict: record.verdict.as_deref().unwrap_or("—"),
        revision_rounds: record.revision_rounds,
        final_diff: &final_diff,
        review_json: review_json.as_deref(),
        security_json: security_json.as_deref(),
        metrics_rows,
    };

    let path = write_dashboard(&dir, &input)?;
    println!("Dashboard written to {}", path.display());
    println!("Open it in a browser: file://{}", path.display());
    Ok(())
}
