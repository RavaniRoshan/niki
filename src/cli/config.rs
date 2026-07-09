use anyhow::Result;
use clap::Subcommand;
use std::fs;

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Initialize a new niki.toml configuration file
    Init,
}

pub async fn handle(command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Init => {
            let example_content = include_str!("../../niki.example.toml");
            let target_path = std::env::current_dir()?.join("niki.toml");
            
            if target_path.exists() {
                println!("niki.toml already exists in the current directory.");
            } else {
                fs::write(&target_path, example_content)?;
                println!("Created niki.toml in the current directory. Please edit it to add your API keys.");
            }
        }
    }
    Ok(())
}
