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
    println!("Session state restored successfully. Ready for continuation.");

    Ok(())
}
