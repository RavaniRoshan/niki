use anyhow::Result;
use clap::Args;
use std::env;
use uuid::Uuid;
use crate::config::NikiConfig;
use crate::orchestrator::state::TaskRecord;

#[derive(Args)]
pub struct ReportArgs {
    /// Task ID to view report for (optional, defaults to most recent)
    pub task_id: Option<Uuid>,
}

pub async fn handle(args: &ReportArgs) -> Result<()> {
    let project_dir = env::current_dir()?;
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");

    let task_id = match args.task_id {
        Some(id) => id,
        None => {
            let mut latest: Option<(Uuid, chrono::DateTime<chrono::Utc>)> = None;
            if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
                for entry in entries.flatten() {
                    let path = entry.path().join("task.json");
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(record) = serde_json::from_str::<TaskRecord>(&content) {
                            let newer = match &latest {
                                Some((_, t)) => record.created_at > *t,
                                None => true,
                            };
                            if newer {
                                latest = Some((record.task_id, record.created_at));
                            }
                        }
                    }
                }
            }
            match latest {
                Some((id, _)) => id,
                None => {
                    eprintln!("No tasks found in {}", tasks_dir.display());
                    return Ok(());
                }
            }
        }
    };

    let report_path = tasks_dir.join(task_id.to_string()).join("report.md");
    match std::fs::read_to_string(&report_path) {
        Ok(content) => print!("{}", content),
        Err(_) => eprintln!("Report not found: {}", report_path.display()),
    }

    Ok(())
}
