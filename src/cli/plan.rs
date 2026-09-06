use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

/// Research and propose a change without making it (plan mode).
///
/// Runs the Planner only, writes a reviewable `plan.md` plus the
/// machine-readable `artifacts/planner.json` into `.niki/tasks/<id>/`, and
/// prints the approval command. Nothing is executed, no branch is created.
/// Approve with `niki run --plan <id>`.
#[derive(Args)]
pub struct PlanArgs {
    /// Natural language description of the task to plan
    pub description: String,

    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
}

pub async fn handle(args: &PlanArgs) -> Result<()> {
    // Plan mode is a dry run with a review surface: delegate to the run
    // pipeline with execution disabled, then point at the approval command.
    // (The `plan.md` + hint are emitted by the dry-run path in `run`.)
    crate::cli::run::handle(&crate::cli::run::RunArgs {
        description: args.description.clone(),
        project: args.project.clone(),
        branch: None,
        max_rounds: None,
        planner_model: None,
        coder_model: None,
        tester_model: None,
        reviewer_model: None,
        backend: None,
        dry_run: true,
        quiet: false,
        verbose: false,
        tui: false,
        force: false,
        plan: None,
    })
    .await
}
