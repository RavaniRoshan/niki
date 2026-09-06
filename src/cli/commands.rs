use crate::commands::CommandRegistry;
use anyhow::Result;
use clap::{Args, Subcommand};
use std::env;
use std::path::PathBuf;

/// Inspect custom slash commands.
///
/// User commands live in `<project>/.niki/commands/*.md` (filename → `/name`,
/// personal tier — `.niki/` is git-ignored). They expand `$ARGUMENTS` and
/// support `description:` / `aliases:` frontmatter.
#[derive(Args)]
pub struct CommandsArgs {
    #[command(subcommand)]
    pub command: CommandsCommands,

    /// Path to the project (default: current directory)
    #[arg(short, long, global = true)]
    pub project: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum CommandsCommands {
    /// List built-in and custom commands
    List,
    /// Show one command's template
    Show {
        /// Command name (or alias), without leading `/`
        name: String,
    },
    /// Expand a command with arguments (prints the resulting prompt)
    Expand {
        /// Command name (or alias), without leading `/`
        name: String,
        /// Arguments replacing `$ARGUMENTS` (default: empty)
        #[arg(last = true)]
        args: Vec<String>,
    },
}

pub fn handle(args: &CommandsArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.clone(),
        None => env::current_dir()?,
    };
    let registry = CommandRegistry::with_project(&project_dir);
    match &args.command {
        CommandsCommands::List => {
            println!("  COMMAND              GROUP     DESCRIPTION");
            for cmd in registry.list() {
                println!(
                    "  /{:<20} {:<9} {}",
                    cmd.name,
                    cmd.group.as_deref().unwrap_or("-"),
                    cmd.description
                );
            }
            Ok(())
        }
        CommandsCommands::Show { name } => {
            let key = name.trim_start_matches('/');
            match registry.get(key) {
                Some(cmd) => {
                    println!("/{} — {}", cmd.name, cmd.description);
                    if !cmd.aliases.is_empty() {
                        println!("aliases: {}", cmd.aliases.join(", "));
                    }
                    println!();
                    println!("{}", cmd.template);
                    Ok(())
                }
                None => {
                    anyhow::bail!(
                        "unknown command '/{key}'. Add `<project>/.niki/commands/{key}.md` to create it."
                    )
                }
            }
        }
        CommandsCommands::Expand { name, args } => {
            let key = name.trim_start_matches('/');
            match registry.expand(key, &args.join(" ")) {
                Some(text) => {
                    print!("{}", text);
                    Ok(())
                }
                None => {
                    anyhow::bail!(
                        "unknown command '/{key}'. Add `<project>/.niki/commands/{key}.md` to create it."
                    )
                }
            }
        }
    }
}
