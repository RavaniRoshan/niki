use crate::session::{RewindMode, SessionManager};
use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use std::env;
use std::path::{Path, PathBuf};

/// What `niki session export` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    /// A readable transcript.
    Markdown,
    /// The Agent Trajectory Interchange Format.
    Atif,
}

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
    /// Write a session out in a shareable form
    Export {
        /// Session ID (default: current)
        id: Option<String>,
        /// `markdown` (default) reads as a transcript; `atif` is the Agent Trajectory
        /// Interchange Format an external harness can validate.
        #[arg(long, value_enum, default_value_t = ExportFormat::Markdown)]
        format: ExportFormat,
        /// Write here instead of stdout.
        #[arg(long, short, value_name = "PATH")]
        out: Option<PathBuf>,
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

/// Current branch name, if HEAD is attached. Used to warn before a code
/// rewind detaches HEAD (Phase 5.6).
fn current_branch(project_dir: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(project_dir)
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !name.is_empty() {
        Some(name)
    } else {
        None
    }
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
        SessionCommands::Export { id, format, out } => {
            let session = match id {
                Some(sid) => Some(mgr.load(sid)?),
                None => mgr.load_current()?,
            };
            let Some(s) = session else {
                // Not an error and not an empty file: there is nothing to export, and writing
                // an empty document would be indistinguishable from a session with no messages.
                bail!(
                    "No session to export in {}.\nNothing was written.",
                    project_dir.display()
                );
            };
            let rendered = match format {
                ExportFormat::Markdown => render_markdown(&s),
                ExportFormat::Atif => render_atif(&s)?,
            };
            match out {
                Some(path) => {
                    if let Some(parent) = path.parent()
                        && !parent.as_os_str().is_empty()
                    {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, rendered)
                        .map_err(|e| anyhow::anyhow!("could not write {}: {e}", path.display()))?;
                    eprintln!("Wrote {} ({format:?}).", path.display());
                }
                None => print!("{rendered}"),
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
                                // Phase 5.6: checking out a commit hash
                                // detaches HEAD — warn first with the way
                                // back, instead of silently stranding the
                                // user off-branch.
                                let before = current_branch(&project_dir);
                                eprintln!(
                                    "Warning: code rewind checks out {c} and detaches HEAD{}. Restore with: git switch {}",
                                    before
                                        .as_deref()
                                        .map(|b| format!(" (was on '{b}')"))
                                        .unwrap_or_default(),
                                    before.as_deref().unwrap_or("-"),
                                );
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

/// A readable transcript.
///
/// The model, provider and cost are in the header because a transcript without them reads as a
/// conversation that happened to nobody in particular. Usage is printed from what the session
/// actually recorded; a session that never recorded usage says so rather than showing zeros,
/// which would be a claim nobody made.
fn render_markdown(s: &crate::session::Session) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "# {}",
        if s.title.is_empty() { &s.id } else { &s.title }
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "- Session: `{}`", s.id);
    let _ = writeln!(out, "- Model: {} ({})", s.model, s.provider);
    let _ = writeln!(out, "- Project: {}", s.project_path.display());
    if s.total_input_tokens > 0 || s.total_output_tokens > 0 {
        let _ = writeln!(
            out,
            "- Usage: {} in / {} out",
            s.total_input_tokens, s.total_output_tokens
        );
    }
    if s.total_cost_usd > 0.0 {
        let _ = writeln!(out, "- Cost: ${:.4}", s.total_cost_usd);
    }
    let _ = writeln!(out, "- Started: {}", s.created_at.to_rfc3339());
    let _ = writeln!(out);
    let _ = writeln!(out, "---");
    let _ = writeln!(out);

    if s.messages.is_empty() {
        let _ = writeln!(out, "_This session recorded no messages._");
        return out;
    }
    for m in &s.messages {
        // Fenced, because message content routinely contains its own backticks and code fences,
        // and a transcript that mangles them is not a transcript.
        let fence = if m.content.contains("```") {
            "````"
        } else {
            "```"
        };
        let _ = writeln!(
            out,
            "## {}",
            if m.role.is_empty() {
                "unknown"
            } else {
                &m.role
            }
        );
        let _ = writeln!(out, "_{}_", m.timestamp.to_rfc3339());
        let _ = writeln!(out);
        let _ = writeln!(out, "{fence}");
        let _ = writeln!(out, "{}", m.content);
        let _ = writeln!(out, "{fence}");
        let _ = writeln!(out);
    }
    out
}

/// The same session as an ATIF trajectory, so an external harness can read a conversation
/// rather than only a pipeline run.
fn render_atif(s: &crate::session::Session) -> Result<String> {
    use crate::artifacts::atif::{AtifMetrics, AtifTrajectory};
    use std::fmt::Write as _;

    let mut t = AtifTrajectory::new(
        "niki",
        env!("CARGO_PKG_VERSION"),
        (!s.model.is_empty()).then(|| s.model.clone()),
    );
    for m in &s.messages {
        // The source is the declared set, not the stored string: a role like "assistant" maps to
        // `agent`, and anything unrecognised is an `agent` step rather than an invented source
        // the ATIF schema does not allow.
        let source = match m.role.as_str() {
            "system" => "system",
            "user" => "user",
            _ => "agent",
        };
        t.push(source, m.content.clone(), Some(m.timestamp.to_rfc3339()));
    }

    let last = t.last_step_mut();
    if s.total_input_tokens > 0 || s.total_output_tokens > 0 || s.total_cost_usd > 0.0 {
        if let Some(step) = last {
            step.metrics = Some(AtifMetrics {
                prompt_tokens: Some(s.total_input_tokens as u32),
                completion_tokens: Some(s.total_output_tokens as u32),
                cached_tokens: None,
                cost_usd: (s.total_cost_usd > 0.0).then_some(s.total_cost_usd),
            });
        }
    }
    t.finalize(s.total_cost_usd > 0.0);
    let mut json = t.to_json()?;
    // The writer emits no trailing newline; a file that ends mid-line is a nuisance to diff.
    let _ = writeln!(json);
    Ok(json)
}
