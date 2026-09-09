use anyhow::{Result, anyhow};
use clap::Subcommand;
use rpassword::prompt_password;
use std::collections::HashMap;

const SERVICE_NAME: &str = "niki";

pub const PROVIDERS: &[(&str, &str, &str)] = &[
    ("ollama", "Ollama (local)", ""),
    ("anthropic", "Anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OpenAI", "OPENAI_API_KEY"),
    ("google", "Google", "GOOGLE_API_KEY"),
    ("openrouter", "OpenRouter", "OPENROUTER_API_KEY"),
    ("zen", "OpenCode Zen", "OPENCODE_API_KEY"),
    ("kimi", "Kimi Code", "KIMI_API_KEY"),
    ("kilo", "KiloCode Gateway", "KILO_API_KEY"),
    ("nvidia", "NVIDIA NIM", "NVIDIA_API_KEY"),
    ("groq", "Groq", "GROQ_API_KEY"),
    ("deepseek", "DeepSeek", "DEEPSEEK_API_KEY"),
    ("together", "Together", "TOGETHER_API_KEY"),
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

        // Keyless providers (Ollama): no key to store — just report whether
        // the local server is reachable.
        if env_var.is_empty() {
            if ollama_running() {
                println!("  Running locally (127.0.0.1:11434) — no API key needed.");
            } else {
                println!("  Not detected. Install from https://ollama.com and run");
                println!("  `ollama serve`, then pull a model (e.g. `ollama pull qwen2.5-coder`).");
            }
            continue;
        }

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

    println!("\nDone.");
    if providers_to_setup
        .iter()
        .any(|(_, _, env_var)| !env_var.is_empty())
    {
        println!("Credentials are stored securely in your OS keyring.");
    }
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

        if env_var.is_empty() {
            // Keyless local provider: reachability is the status.
            let status = if ollama_running() {
                "running locally (no key needed)".to_string()
            } else {
                "not detected (run `ollama serve`)".to_string()
            };
            println!("{}: {}", label, status);
            continue;
        }

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

/// Reachability probe for the local Ollama server. Shared by the setup
/// wizard, `auth login`, `auth status`, and `doctor` so all four agree.
pub fn ollama_running() -> bool {
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().expect("loopback addr"),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

/// Names of models installed in the local Ollama (`/api/tags`), best-effort.
/// Empty when Ollama is down or the response doesn't parse. Plain
/// `TcpStream` HTTP keeps this usable from sync contexts without a client.
pub fn ollama_models() -> Vec<String> {
    let mut stream = match std::net::TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().expect("loopback addr"),
        std::time::Duration::from_millis(300),
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(800)))
        .ok();
    use std::io::{Read, Write};
    if stream
        .write_all(b"GET /api/tags HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")
        .is_err()
    {
        return Vec::new();
    }
    let mut raw = String::new();
    if stream.read_to_string(&mut raw).is_err() {
        return Vec::new();
    }
    let body = match raw.split_once("\r\n\r\n") {
        Some((_, b)) => b,
        None => return Vec::new(),
    };
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v.get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Pick the installed Ollama model most likely to write code: first name
/// containing `coder`/`code`, else the first installed model, else the
/// well-known small coding default (with a pull hint from the caller).
pub fn preferred_ollama_model() -> (String, bool) {
    let models = ollama_models();
    if let Some(m) = models
        .iter()
        .find(|m| m.contains("coder") || m.contains("code"))
        .or_else(|| models.first())
    {
        (m.clone(), true)
    } else {
        ("qwen2.5-coder:3b".to_string(), false)
    }
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
/// Driven by the PROVIDERS registry so every supported provider resolves.
pub fn resolve_api_key(provider: &str) -> Option<String> {
    let env_var = PROVIDERS
        .iter()
        .find(|(name, _, _)| *name == provider)
        .map(|(_, _, env)| *env)?;

    if let Ok(key) = std::env::var(env_var) {
        return Some(key);
    }

    let entry = keyring::Entry::new(SERVICE_NAME, provider).ok()?;
    entry.get_password().ok()
}
