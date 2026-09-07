use crate::orchestrator::state::TaskRecord;
use anyhow::Result;
use clap::Args;
use std::env;
use std::path::{Path, PathBuf};

/// Emit a consolidated compliance bundle for one task as JSON.
///
/// Collects the task record, safety proof, test execution, trace spans, and
/// cost totals from `.niki/tasks/<id>/` into a single machine-readable
/// document. Missing pieces are `null` (with `complete: false`) rather than
/// an error — a partial trail must be inspectable, not hidden.
#[derive(Args)]
pub struct AuditArgs {
    /// Task ID — full UUID or unique short prefix (default: most recent task)
    pub task_id: Option<String>,

    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
}

fn read_json(dir: &Path, name: &str) -> Option<serde_json::Value> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
}

fn read_lines(dir: &Path, name: &str) -> Option<Vec<serde_json::Value>> {
    std::fs::read_to_string(dir.join(name)).ok().map(|c| {
        c.lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    })
}

pub fn handle(args: &AuditArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    let config = crate::config::NikiConfig::load(&project_dir).unwrap_or_default();
    let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");

    let task_id = match &args.task_id {
        Some(id) => crate::cli::report::resolve_task_id(&tasks_dir, id)?,
        None => crate::cli::report::latest_task_id(&tasks_dir)
            .ok_or_else(|| anyhow::anyhow!("No tasks found in {}", tasks_dir.display()))?,
    };
    let dir = tasks_dir.join(&task_id);

    let record: Option<TaskRecord> =
        read_json(&dir, "task.json").and_then(|v| serde_json::from_value(v).ok());
    let (status, branch, verdict, cost_usd, input_tokens, output_tokens) = match &record {
        Some(r) => (
            r.status.to_string(),
            r.branch.clone(),
            r.verdict.clone(),
            r.total_cost_usd,
            r.total_input_tokens,
            r.total_output_tokens,
        ),
        None => ("unknown".to_string(), None, None, 0.0, 0, 0),
    };
    let safety_proof = read_json(&dir, "safety_proof.json");
    let test_execution = read_json(&dir, "artifacts/test_execution.json")
        .or_else(|| read_json(&dir, "test_execution.json"));
    let trace = read_lines(&dir, "trace.jsonl");
    let has_report = dir.join("report.md").is_file();
    let has_patch = dir.join("changes.patch").is_file();

    let bundle = serde_json::json!({
        "task_id": task_id,
        "status": status,
        "branch": branch,
        "verdict": verdict,
        "cost_usd": cost_usd,
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "has_report": has_report,
        "has_patch": has_patch,
        "safety_proof": safety_proof,
        "test_execution": test_execution,
        "trace_spans": trace,
        "complete": record.is_some() && has_report && safety_proof.is_some(),
    });
    println!("{}", serde_json::to_string_pretty(&bundle)?);
    Ok(())
}
