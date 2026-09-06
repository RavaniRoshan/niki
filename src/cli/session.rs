use crate::session::{RewindMode, SessionManager};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use std::env;
use std::path::{Path, PathBuf};

/// Inspect and rewind chat/pipeline sessions.
///
/// Sessions and their checkpoints live under `.niki/sessions/`. The pipeline
/// records an `after_planner` checkpoint on every run, so `rewind` can restore
/// the conversation, the code, or both to that point.
#[derive(Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCommands,

    /// Path to the project (default: current directory)
    #[arg(short, long, global = true)]
    pub project: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List sessions, most recent first
    List,
    /// Show a session's detail (default: current session)
    Show {
        /// Session ID (default: current)
        id: Option<String>,
    },
    /// List checkpoints of the current session
    Checkpoints,
    /// Step back one checkpoint (conversation only)
    Undo,
    /// Rewind one checkpoint, optionally restoring code too
    Rewind {
        /// What to restore: both (default), code, or conversation
        #[arg(long, default_value = "both")]
        mode: String,
        /// Allow code restore with a dirty working tree (default: refuse)
        #[arg(long)]
        force: bool,
    },
}

fn manager(project: &Option<PathBuf>) -> Result<(PathBuf, SessionManager)> {
    let project_dir = match project {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    Ok((project_dir.clone(), SessionManager::new(&project_dir)))
}

fn parse_mode(s: &str) -> Result<RewindMode> {
    match s.to_lowercase().as_str() {
        "both" => Ok(RewindMode::Both),
        "code" => Ok(RewindMode::CodeOnly),
        "conversation" => Ok(RewindMode::ConversationOnly),
        other => bail!("unknown mode '{other}': expected both | code | conversation"),
    }
}

/// True when the tracked working tree is clean. Untracked files are ignored —
/// scratch state like `.niki/` itself must not block a rewind. A code rewind
/// would discard uncommitted *tracked* changes, so those refuse without `--force`.
fn working_tree_clean(project_dir: &Path) -> bool {
    std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(project_dir)
        .output()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(true)
}

fn checkout_commit(project_dir: &Path, commit: &str) -> Result<()> {
    let out = std::process::Command::new("git")
        .args(["checkout", commit])
        .current_dir(project_dir)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "git checkout {} failed: {}",
            commit,
            String::from_utf8_lossy(&out.stderr).trim()
        )
    }
}

pub fn handle(args: &SessionArgs) -> Result<()> {
    let (project_dir, mgr) = manager(&args.project)?;
    match &args.command {
        SessionCommands::List => {
            let sessions = mgr.list()?;
            if sessions.is_empty() {
                println!("No sessions in {}", project_dir.display());
                return Ok(());
            }
            println!(
                "  ID                                    UPDATED               MSGS  COST      TITLE"
            );
            for s in sessions {
                println!(
                    "  {}  {}  {:>4}  ${:<8.4}  {}",
                    s.id,
                    s.updated_at.format("%Y-%m-%d %H:%M"),
                    s.messages.len(),
                    s.total_cost_usd,
                    s.title,
                );
            }
            Ok(())
        }
        SessionCommands::Show { id } => {
            let session = match id {
                Some(sid) => Some(mgr.load(sid)?),
                None => mgr.load_current()?,
            };
            match session {
                Some(s) => {
                    println!("Session {}", s.id);
                    println!("  Title: {}", s.title);
                    println!("  Model: {} ({})", s.model, s.provider);
                    println!("  Messages: {}", s.messages.len());
                    println!(
                        "  Tokens: {} in / {} out · cost ${:.4}",
                        s.total_input_tokens, s.total_output_tokens, s.total_cost_usd
                    );
                    println!("  Checkpoints: {}", s.checkpoints.len());
                    for (i, c) in s.checkpoints.iter().enumerate() {
                        let cur = if Some(i) == s.current_checkpoint {
                            "  <-- current"
                        } else {
                            ""
                        };
                        println!("    [{}] {}{}", i, c.label, cur);
                    }
                }
                None => println!("No current session in {}", project_dir.display()),
            }
            Ok(())
        }
        SessionCommands::Checkpoints => {
            for label in mgr.checkpoint_labels()? {
                println!("- {}", label);
            }
            Ok(())
        }
        SessionCommands::Undo => match mgr.undo()? {
            true => {
                println!("Undone one checkpoint.");
                Ok(())
            }
            false => {
                println!("Nothing to undo.");
                Ok(())
            }
        },
        SessionCommands::Rewind { mode, force } => {
            let mode = parse_mode(mode)?;
            let needs_code = matches!(mode, RewindMode::Both | RewindMode::CodeOnly);
            if needs_code && !working_tree_clean(&project_dir) && !force {
                bail!(
                    "working tree is dirty — a code rewind would discard uncommitted work. \
                     Commit/stash first, or re-run with `--force`."
                );
            }
            match mgr.rewind_mode(mode)? {
                Some((label, commit)) => {
                    println!("Rewound to checkpoint '{label}'.");
                    if needs_code {
                        match commit {
                            Some(c) => {
                                checkout_commit(&project_dir, &c)?;
                                println!(
                                    "Code restored to {c} (mode: {}).",
                                    if mode == RewindMode::Both {
                                        "conversation + code"
                                    } else {
                                        "code only"
                                    }
                                );
                            }
                            None => println!(
                                "Checkpoint has no git commit recorded — conversation restored, code untouched."
                            ),
                        }
                    }
                    Ok(())
                }
                None => {
                    println!("Nothing to rewind to.");
                    Ok(())
                }
            }
        }
    }
}
