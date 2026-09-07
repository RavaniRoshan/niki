use anyhow::Result;
use clap::Subcommand;
use std::fs;

use crate::cli::auth::{PROVIDERS, resolve_api_key};

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Initialize a new niki.toml configuration file
    Init {
        /// Run interactively (prompt for settings, check env vars)
        #[arg(short, long)]
        interactive: bool,
        /// Scan the project and draft AGENTS.md from detected languages,
        /// dependencies, and test commands
        #[arg(long)]
        scan: bool,
    },
    /// Export the JSON schema for niki.toml (for editor autocomplete)
    Schema,
}

pub async fn handle(command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Init { interactive, scan } => cmd_init(*interactive, *scan).await,
        ConfigCommands::Schema => cmd_schema(),
    }
}

async fn cmd_init(interactive: bool, scan: bool) -> Result<()> {
    let target_path = std::env::current_dir()?.join("niki.toml");
    let project_dir = std::env::current_dir()?;

    if target_path.exists() {
        println!("niki.toml already exists in the current directory.");
        println!("Run `niki auth login` to configure credentials via keyring.");
    } else if interactive {
        cmd_init_interactive(&target_path).await?;
    } else {
        let example_content = include_str!("../../niki.example.toml");
        fs::write(&target_path, example_content)?;
        println!("Created niki.toml from template.");
    }

    if scan {
        cmd_scan(&project_dir).await?;
    }

    println!(
        "Edit it to add your API keys, or run `niki auth login` to store them securely in your OS keyring."
    );
    Ok(())
}

/// Scan the project layout and draft `AGENTS.md` (the instruction file NIKI's
/// agents actually read) from detected languages, dependencies, and the test
/// command. Never overwrites an existing AGENTS.md — prints a diff-style
/// suggestion instead, so human-curated instructions are never clobbered.
async fn cmd_scan(project_dir: &std::path::Path) -> Result<()> {
    let config = crate::config::NikiConfig::load(project_dir).unwrap_or_default();
    let knowledge = crate::knowledge::index_project(project_dir, &config).await?;
    let test_command = crate::agents::tester::autodetect_test_command(project_dir);

    let mut draft = String::from(
        "# AGENTS.md (drafted by `niki init --scan` — edit freely; agents read this file)\n\n",
    );
    draft.push_str("## Stack\n\n");
    if knowledge.detected_languages.is_empty() {
        draft.push_str("- (no languages detected)\n");
    } else {
        draft.push_str(&format!(
            "- Languages: {}\n",
            knowledge.detected_languages.join(", ")
        ));
    }
    for pkg in &knowledge.package_info {
        draft.push_str(&format!(
            "- {} (`{}`): {}\n",
            pkg.manager,
            pkg.file_path,
            if pkg.dependencies.is_empty() {
                "(no dependencies listed)".to_string()
            } else {
                pkg.dependencies.join(", ")
            }
        ));
    }
    draft.push_str("\n## Verification\n\n");
    match &test_command {
        Some(cmd) => draft.push_str(&format!(
            "- Test command (auto-detected): `{}` — keep it green; a red suite blocks the branch.\n",
            cmd
        )),
        None => draft.push_str(
            "- No test command detected — set `[agents.tester] test_command` so runs are verifiable.\n",
        ),
    }
    draft.push_str(
        "\n## Conventions\n\n- (add yours: code style, architecture rules, things agents must never do)\n- Recurring rules also live in `.niki/rules/<topic>.md` (one concern per file) and are injected into every run as binding Standing Rules.\n",
    );

    let agents_path = project_dir.join("AGENTS.md");
    if agents_path.exists() {
        println!("\nAGENTS.md already exists — leaving it untouched. Suggested additions:\n");
        println!("{draft}");
    } else {
        fs::write(&agents_path, &draft)?;
        println!("\nWrote {} (review and extend it).", agents_path.display());
    }
    Ok(())
}

async fn cmd_init_interactive(target_path: &std::path::Path) -> Result<()> {
    println!("Welcome to the NIKI configuration wizard!\n");

    let example_content = include_str!("../../niki.example.toml");

    for (name, label, env_var) in PROVIDERS {
        println!("--- {} ---", label);

        if let Ok(_key) = std::env::var(env_var) {
            println!("  Found {} in your environment", env_var);
            if prompt_yes_no("  Store in OS keyring?") {
                crate::cli::auth::handle(&crate::cli::auth::AuthCommands::Login {
                    provider: Some((*name).to_string()),
                    stdin: false,
                })
                .await?;
                println!(
                    "  Stored in keyring. You can also manually set {} or edit niki.toml.",
                    env_var
                );
            }
        } else if resolve_api_key(name).is_some() {
            println!("  Key already stored in keyring for {}", name);
        } else {
            println!("  {} not found in environment.", env_var);
            if prompt_yes_no("  Enter API key now (will be stored in keyring)?") {
                crate::cli::auth::handle(&crate::cli::auth::AuthCommands::Login {
                    provider: Some((*name).to_string()),
                    stdin: false,
                })
                .await?;
            } else {
                println!(
                    "  Skipping {} — you can add it later with `niki auth login`",
                    name
                );
            }
        }
        println!();
    }

    fs::write(target_path, example_content)?;
    println!("Created niki.toml with provider entries.");
    println!("API keys have been stored in your OS keyring where provided.");
    println!("Run `niki doctor` to verify your setup.");
    Ok(())
}

fn cmd_schema() -> Result<()> {
    let path = std::env::current_dir()?.join("niki.schema.json");
    let schema = crate::config::types::NikiConfig::config_schema_json();
    fs::write(&path, schema)?;
    println!("Exported JSON schema to {}", path.display());
    Ok(())
}

fn prompt_yes_no(message: &str) -> bool {
    print!("{} [y/N]: ", message);
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}
