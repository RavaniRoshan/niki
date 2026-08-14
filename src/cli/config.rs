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
    },
    /// Export the JSON schema for niki.toml (for editor autocomplete)
    Schema,
}

pub async fn handle(command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Init { interactive } => cmd_init(*interactive).await,
        ConfigCommands::Schema => cmd_schema(),
    }
}

async fn cmd_init(interactive: bool) -> Result<()> {
    let target_path = std::env::current_dir()?.join("niki.toml");

    if target_path.exists() {
        println!("niki.toml already exists in the current directory.");
        println!("Run `niki auth login` to configure credentials via keyring.");
        return Ok(());
    }

    if interactive {
        cmd_init_interactive(&target_path).await?;
    } else {
        let example_content = include_str!("../../niki.example.toml");
        fs::write(&target_path, example_content)?;
        println!("Created niki.toml from template.");
    }

    println!(
        "Edit it to add your API keys, or run `niki auth login` to store them securely in your OS keyring."
    );
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
