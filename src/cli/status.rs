use anyhow::Result;
use std::env;
use std::path::PathBuf;
use crate::config::NikiConfig;
use crate::orchestrator::state::TaskRecord;

pub async fn handle() -> Result<()> {
    let project_dir = env::current_dir()?;
    let config = NikiConfig::load(&project_dir).unwrap_or_default();
    let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");

    let mut latest: Option<(PathBuf, TaskRecord)> = None;
    if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
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

    match latest {
        Some((dir, record)) => {
            println!("Task:       {}", record.task_id);
            println!("Description: {}", record.description);
            println!("Status:     {:?}", record.status);
            if let Some(branch) = &record.branch {
                println!("Branch:     {}", branch);
            }
            if let Some(verdict) = &record.verdict {
                println!("Verdict:    {}", verdict);
            }
            println!("Revisions:  {}", record.revision_rounds);
            println!("Artifacts:  {}", dir.join("artifacts").display());
            println!("Report:     {}", dir.join("report.md").display());
        }
        None => {
            println!("No tasks found in {}", tasks_dir.display());
        }
    }

    Ok(())
}
