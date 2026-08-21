//! `niki acp` - run NIKI as an Agent Client Protocol (ACP) server.
//!
//! Reads newline-delimited JSON-RPC 2.0 requests from stdin, dispatches them
//! to the pipeline, and writes responses + progress notifications to stdout.
//! Drives an IDE (Zed, Claude Code) over stdio.

use clap::Args;
use std::path::PathBuf;

/// Run the ACP server over stdio.
#[derive(Args, Clone, Default)]
pub struct AcpArgs {
    /// Path to the project directory
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,
}

/// Entry point for `niki acp`.
pub async fn handle(args: &AcpArgs) -> anyhow::Result<()> {
    let project_dir = args
        .project
        .canonicalize()
        .unwrap_or_else(|_| args.project.clone());
    crate::acp::server::run(project_dir).await?;
    Ok(())
}
