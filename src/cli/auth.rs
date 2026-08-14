use anyhow::{Result, anyhow};
use clap::Subcommand;
use rpassword::prompt_password;
use std::collections::HashMap;

const SERVICE_NAME: &str = "niki";

pub const PROVIDERS: &[(&str, &str, &str)] = &[
    ("anthropic", "Anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OpenAI", "OPENAI_API_KEY"),
    ("google", "Google", "GOOGLE_API_KEY"),
];

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Store API credentials securely in the OS keyring
    Login {
        /// Provider to configure (default: all)
        #[arg(short, long)]
        provider: Option<String>,
        /// Read API key from stdin instead of prompting
        #[arg(short, long)]
        stdin: bool,
    },
    /// Remove stored credentials
    Logout {
        /// Provider to remove (default: all)
        #[arg(short, long)]
        provider: Option<String>,
    },
    /// Show credential status for each provider
    Status,
}

pub async fn handle(command: &AuthCommands) -> Result<()> {
    match command {
        AuthCommands::Login { provider, stdin } => cmd_login(provider, *stdin),
        AuthCommands::Logout { provider } => cmd_logout(provider),
        AuthCommands::Status => cmd_status(),
    }
}

fn cmd_login(provider: &Option<String>, from_stdin: bool) -> Result<()> {
    let providers_to_setup: Vec<(&str, &str, &str)> = match provider {
        Some(p) => {
            let found = PROVIDERS
                .iter()
                .copied()
                .find(|(name, _, _)| *name == p)
                .ok_or_else(|| {
                    anyhow!(
                        "Unknown provider '{}'. Available: {}",
                        p,
                        available_providers()
                    )
                })?;
            vec![found]
        }
        None => PROVIDERS.to_vec(),
    };

    let existing = load_existing_keys();

    for (name, label, env_var) in &providers_to_setup {
        println!("--- {} ---", label);

        if existing.contains_key(*name) {
            println!("  Already configured (key stored in OS keyring)");
            if !prompt_yes_no("Replace?") {
                continue;
            }
        } else if let Ok(key) = std::env::var(env_var) {
            println!("  Found {} in environment", env_var);
            if !prompt_yes_no("Store in keyring?") {
                continue;
            }
            store_key(name, &key)?;
            println!("  Stored {} API key in OS keyring", label);
            continue;
        }

        let key = if from_stdin {
            let mut buf = String::new();
            std::io::stdin().read_line(&mut buf)?;
            buf.trim().to_string()
        } else {
            prompt_password(format!("Enter {} API key (sk-...): ", label))?
        };

        if key.is_empty() {
            println!("  Skipped");
            continue;
        }

        store_key(name, &key)?;
        println!("  Stored {} API key in OS keyring", label);
    }

    println!("\nDone. Credentials are stored securely in your OS keyring.");
    println!("Run `niki doctor` to verify your setup.");
    Ok(())
}

fn cmd_logout(provider: &Option<String>) -> Result<()> {
    match provider {
        Some(p) => {
            if !PROVIDERS.iter().any(|(name, _, _)| name == p) {
                return Err(anyhow!("Unknown provider '{}'", p));
            }
            delete_key(p)?;
            println!("Removed {} credentials from keyring", p);
        }
        None => {
            let mut count = 0;
            for (name, _, _) in PROVIDERS {
                if delete_key(name).is_ok() {
                    count += 1;
                }
            }
            println!("Removed {} credential(s) from keyring", count);
        }
    }
    Ok(())
}

fn cmd_status() -> Result<()> {
    let existing = load_existing_keys();
    let env_keys = load_env_keys();

    println!("NIKI Credential Status");
    println!("=====================");

    for (name, label, env_var) in PROVIDERS {
        let mut parts = Vec::new();

        if env_keys.contains_key(*name) {
            parts.push("via env var");
        }
        if existing.contains_key(*name) {
            parts.push("keyring");
        }

        let status = if parts.is_empty() {
            "not configured".to_string()
        } else {
            format!("configured ({})", parts.join(", "))
        };

        println!("{}: {} [{}]", label, status, env_var);
    }

    Ok(())
}

fn available_providers() -> String {
    PROVIDERS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<&str>>()
        .join(", ")
}

fn store_key(provider: &str, api_key: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, provider)?;
    entry.set_password(api_key)?;
    Ok(())
}

fn delete_key(provider: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE_NAME, provider)?;
    entry.delete_password()?;
    Ok(())
}

pub fn load_existing_keys() -> HashMap<String, String> {
    let mut keys = HashMap::new();
    for (name, _, _) in PROVIDERS {
        let entry = match keyring::Entry::new(SERVICE_NAME, name) {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if let Ok(key) = entry.get_password() {
            keys.insert(name.to_string(), key);
        }
    }
    keys
}

pub fn load_env_keys() -> HashMap<String, String> {
    let mut keys = HashMap::new();
    for (name, _, env_var) in PROVIDERS {
        if let Ok(key) = std::env::var(env_var) {
            keys.insert(name.to_string(), key);
        }
    }
    keys
}

fn prompt_yes_no(message: &str) -> bool {
    print!("{} [y/N]: ", message);
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Resolve an API key for a provider: check env vars first, then keyring.
pub fn resolve_api_key(provider: &str) -> Option<String> {
    let env_var = match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "google" => "GOOGLE_API_KEY",
        _ => return None,
    };

    if let Ok(key) = std::env::var(env_var) {
        return Some(key);
    }

    let entry = keyring::Entry::new(SERVICE_NAME, provider).ok()?;
    entry.get_password().ok()
}
