use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::memory::{load_memory, query_memory_by_tag, get_all_tags, render_memory_for_prompt};
use crate::artifacts::types::AgentRole;

#[derive(Args)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: MemoryCommands,

    /// Project directory (default: current directory).
    #[arg(short, long, global = true)]
    pub project: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Show memory for a specific role (or all roles).
    Show {
        /// Agent role to show memory for (planner, coder, tester, reviewer, red, security_auditor, synthesizer).
        #[arg(short, long)]
        role: Option<String>,

        /// Maximum number of entries to show per role.
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },

    /// List all unique tags across all roles.
    Tags,

    /// Query memory entries by tag.
    Query {
        /// Tag to search for.
        tag: String,

        /// Maximum number of results.
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// Show the memory that would be injected into prompts for a role.
    Preview {
        /// Agent role to preview memory for.
        #[arg(short, long, default_value = "coder")]
        role: String,

        /// Maximum number of entries to include.
        #[arg(short = 'n', long, default_value = "5")]
        limit: usize,
    },

    /// Clear all memory for a role (or all roles).
    Clear {
        /// Agent role to clear memory for. If omitted, clears all roles.
        #[arg(short, long)]
        role: Option<String>,

        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
    },
}

pub fn handle(args: &MemoryArgs) -> Result<()> {
    let project = args
        .project
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    match &args.command {
        MemoryCommands::Show { role, limit } => {
            let roles = parse_roles(role);
            for r in roles {
                let memory = load_memory(&project, r);
                println!("\n=== {:?} ({}) ===", r, memory.entries.len());
                for entry in memory.entries.iter().rev().take(*limit) {
                    println!("  [{}] {}", &entry.timestamp[..10], entry.task);
                    if !entry.tags.is_empty() {
                        println!("    Tags: {}", entry.tags.join(", "));
                    }
                    let preview: String = entry.content.chars().take(200).collect();
                    println!("    {}", preview);
                    println!();
                }
            }
        }

        MemoryCommands::Tags => {
            let tags = get_all_tags(&project);
            if tags.is_empty() {
                println!("No memory tags found. Memory is populated after running `niki run`.");
            } else {
                println!("Memory tags:");
                for tag in &tags {
                    println!("  - {}", tag);
                }
            }
        }

        MemoryCommands::Query { tag, limit } => {
            let results = query_memory_by_tag(&project, tag);
            if results.is_empty() {
                println!("No entries found with tag '{}'.", tag);
            } else {
                println!("Entries with tag '{}':", tag);
                for (role, entry) in results.iter().take(*limit) {
                    println!("  [{:?}] [{}] {}", role, &entry.timestamp[..10], entry.task);
                    let preview: String = entry.content.chars().take(200).collect();
                    println!("    {}", preview);
                    println!();
                }
            }
        }

        MemoryCommands::Preview { role, limit } => {
            let r = parse_role(role)?;
            let rendered = render_memory_for_prompt(&project, r, *limit);
            if rendered.is_empty() {
                println!("No memory entries for {:?}. Memory is populated after running `niki run`.", r);
            } else {
                println!("{}", rendered);
            }
        }

        MemoryCommands::Clear { role, force } => {
            let roles = parse_roles(role);
            if !force {
                let role_names: Vec<String> = roles.iter().map(|r| format!("{:?}", r)).collect();
                println!("This will clear memory for: {}", role_names.join(", "));
                print!("Are you sure? [y/N] ");
                use std::io::{self, Write};
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            for r in roles {
                let path = memory_path(&project, r);
                if path.exists() {
                    std::fs::remove_file(&path)?;
                    println!("Cleared {:?} memory.", r);
                } else {
                    println!("No memory file for {:?}.", r);
                }
            }
        }
    }

    Ok(())
}

fn parse_roles(role: &Option<String>) -> Vec<AgentRole> {
    match role.as_deref() {
        Some("planner") => vec![AgentRole::Planner],
        Some("coder") => vec![AgentRole::Coder],
        Some("tester") => vec![AgentRole::Tester],
        Some("reviewer") => vec![AgentRole::Reviewer],
        Some("red") => vec![AgentRole::Red],
        Some("security_auditor") => vec![AgentRole::SecurityAuditor],
        Some("synthesizer") => vec![AgentRole::Synthesizer],
        Some(other) => {
            eprintln!("Unknown role: {}. Valid: planner, coder, tester, reviewer, red, security_auditor, synthesizer", other);
            std::process::exit(1);
        }
        None => vec![
            AgentRole::Planner,
            AgentRole::Coder,
            AgentRole::Tester,
            AgentRole::Reviewer,
            AgentRole::Red,
            AgentRole::SecurityAuditor,
            AgentRole::Synthesizer,
        ],
    }
}

fn parse_role(s: &str) -> Result<AgentRole> {
    match s {
        "planner" => Ok(AgentRole::Planner),
        "coder" => Ok(AgentRole::Coder),
        "tester" => Ok(AgentRole::Tester),
        "reviewer" => Ok(AgentRole::Reviewer),
        "red" => Ok(AgentRole::Red),
        "security_auditor" => Ok(AgentRole::SecurityAuditor),
        "synthesizer" => Ok(AgentRole::Synthesizer),
        _ => Err(anyhow::anyhow!(
            "Unknown role: {}. Valid: planner, coder, tester, reviewer, red, security_auditor, synthesizer",
            s
        )),
    }
}

fn memory_path(project_dir: &PathBuf, role: AgentRole) -> std::path::PathBuf {
    let role_name = match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
    };
    project_dir.join(".niki").join("memory").join(format!("{}.json", role_name))
}
