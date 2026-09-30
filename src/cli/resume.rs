//! `niki resume <session-id>` — resume an interrupted agent session from a checkpoint.

use anyhow::Result;
use clap::Args;
use std::env;
use std::path::PathBuf;

use crate::config::NikiConfig;
use crate::runtime::AgentRuntime;

#[derive(Args, Debug)]
pub struct ResumeArgs {
    /// Session ID or Task ID (full UUID or short prefix)
    pub session_id: String,

    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
}

pub async fn handle(args: &ResumeArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.canonicalize()?,
        None => env::current_dir()?,
    };

    let config = NikiConfig::load(&project_dir)?;
    let output_dir = config.general.output_dir.clone();
    let runtime = AgentRuntime::new(config);

    println!(
        "Locating checkpoint for session/task '{}'...",
        args.session_id
    );
    let (session, checkpoint) = runtime.resume(&project_dir, &args.session_id).await?;

    println!("============================================================");
    println!("Resumed session:    {}", session.session_id);
    println!("Task ID:            {}", session.task_id);
    println!("Task description:   {}", session.task_description);
    println!("Last active role:   {:?}", checkpoint.current_role);
    println!("Current turn:       {}", checkpoint.current_turn);
    println!("Current step:       {}", checkpoint.current_step);
    println!("Checkpoint time:    {}", checkpoint.timestamp);
    println!(
        "Artifacts recorded: {}",
        checkpoint.produced_artifacts.len()
    );
    for (role, _) in &checkpoint.produced_artifacts {
        println!("  - Artifact from:  {:?}", role);
    }
    println!("Context fragments:  {}", checkpoint.fragments.len());
    println!(
        "Total tokens:       {}",
        session.context_store.read().await.total_estimated_tokens()
    );
    if let Some(ref branch) = checkpoint.active_branch {
        println!("Active branch:      {}", branch);
    }
    println!("============================================================");

    // What actually happened, and what to do about it.
    //
    // This used to print "Session state restored successfully. Ready for
    // continuation." and exit 0. Nothing was restored into anything: the
    // `runtime` holding the session is dropped on the next line, and no code
    // path anywhere re-enters the pipeline from a checkpoint. The message told
    // a user with an interrupted run that they could carry on, and then
    // nothing happened.
    //
    // Re-entering the pipeline from a checkpoint is a feature, not a repair —
    // it is the whole of the next slice's decision, and pretending otherwise
    // here would be the exact dishonesty this branch has been removing
    // elsewhere. So this says what is true and names the commands that do
    // something.
    let task_dir = project_dir
        .join(&output_dir)
        .join("tasks")
        .join(checkpoint.task_id.to_string());
    println!("Nothing was re-run. This command located and described the checkpoint;");
    println!("it did not restart the pipeline, and NIKI cannot yet continue one mid-flight.");
    println!();
    println!(
        "  What survived: {} artifact(s) from the checkpoint above.",
        checkpoint.produced_artifacts.len()
    );
    if task_dir.is_dir() {
        println!("  On disk:      {}", task_dir.display());
        println!(
            "                report.md, changes.patch and artifacts/ are there if the run got that far."
        );
    }
    println!(
        "  To continue:  re-run the task — `niki run \"{}\"`",
        session.task_description
    );
    println!(
        "                or, from the TUI, `niki chat` then `/run {}`.",
        session.task_description
    );
    println!("  To inspect:   `niki report {}`", checkpoint.task_id);

    Ok(())
}
