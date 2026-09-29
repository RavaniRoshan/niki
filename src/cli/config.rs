use anyhow::Result;
use clap::Subcommand;
use std::fs;

use crate::cli::auth::{PROVIDERS, resolve_api_key};
use crate::config::NikiConfig;

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
    /// Report what is wrong with niki.toml, and exit non-zero if anything is
    Check,
}

pub async fn handle(command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Init { interactive, scan } => cmd_init(*interactive, *scan).await,
        ConfigCommands::Schema => cmd_schema(),
        ConfigCommands::Check => cmd_check(),
    }
}

/// Say what is wrong with the configuration, and exit non-zero if anything is.
///
/// Exists because the error message that points at it did not. Every command
/// that loads config used to swallow a parse failure into the defaults, so a
/// typo in `niki.toml` produced no diagnostic anywhere: the assistant replied
/// as if nothing had been written, the pipeline ran with Anthropic's defaults,
/// and the user had no way to know their file was the problem.
///
/// Two kinds of problem, both silent until now:
///
/// * a **syntax** error, which made `NikiConfig::load` return `Err` and every
///   caller throw it away;
/// * an **unknown section**, which parses perfectly and is then ignored —
///   `[agentz.coder]` with a `z` is not a warning you notice when you are
///   looking at your bill.
///
/// The unknown-section report is the more valuable half. A file that fails to
/// parse tells you it failed. A file that parses and is ignored looks exactly
/// like a file that works.
fn cmd_check() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let local = project_dir.join("niki.toml");
    let global = dirs::home_dir().map(|h| h.join(".config/niki/niki.toml"));

    let mut problems = 0usize;
    let mut checked = 0usize;

    for path in [Some(local), global].into_iter().flatten() {
        if !path.exists() {
            continue;
        }
        checked += 1;
        println!("checking {}", path.display());
        match NikiConfig::load_file_only(&path) {
            Ok(()) => {}
            Err(e) => {
                problems += 1;
                println!("  ✗ {e}");
            }
        }
    }

    if checked == 0 {
        println!("No niki.toml found. Nothing to check — this is fine.");
        return Ok(());
    }

    // Loading the merged config is a second, different check: each file can
    // parse on its own and still be invalid once the defaults are applied.
    match NikiConfig::load(&project_dir) {
        Ok(_) => {
            if problems == 0 {
                println!("\n{checked} file(s) checked, no problems found.");
            }
        }
        Err(e) => {
            problems += 1;
            println!("  ✗ the merged configuration is invalid: {e}");
        }
    }

    if problems > 0 {
        println!(
            "\n{problems} problem(s). The configuration was not applied — commands \
             that load it fall back to defaults."
        );
        std::process::exit(1);
    }
    Ok(())
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

    // Local-first: Ollama needs no key. Probe the default endpoint once with
    // a short timeout; a reachable instance becomes menu option 0.
    let ollama_up = crate::cli::auth::ollama_running();
    if ollama_up {
        println!("  Ollama is running locally (127.0.0.1:11434) — no API key needed.");
    } else {
        println!(
            "  Ollama not detected locally (optional — fully offline models via http://localhost:11434)."
        );
    }
    println!();

    // Harness-style setup: one numbered menu over all providers, annotated
    // with where a key was already found. Pick ONE to configure now instead
    // of answering eleven prompts in a row.
    #[derive(Clone, Copy)]
    enum KeyState {
        Env,
        Keyring,
        Missing,
    }
    let mut menu: Vec<(&str, &str, &str, KeyState)> = Vec::new();
    if ollama_up {
        // Marker entry: no key material involved.
        menu.push(("ollama", "Ollama (local, no key)", "", KeyState::Missing));
    }
    for (name, label, env_var) in PROVIDERS {
        if *name == "ollama" {
            continue; // offered as menu option 0 above when reachable
        }
        let state = if std::env::var(env_var)
            .map(|k| !k.is_empty())
            .unwrap_or(false)
        {
            KeyState::Env
        } else if resolve_api_key(name).is_some() {
            KeyState::Keyring
        } else {
            KeyState::Missing
        };
        menu.push((name, label, env_var, state));
    }
    for (i, (_, label, env_var, state)) in menu.iter().enumerate() {
        let marker = match state {
            KeyState::Env => format!("(via {})", env_var),
            KeyState::Keyring => "(in keyring)".to_string(),
            KeyState::Missing => String::new(),
        };
        println!("  {}) {} {}", i, label, marker);
    }
    println!();
    print!("Pick a provider to set up [0-{}]: ", menu.len() - 1);
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let mut choice = String::new();
    std::io::stdin().read_line(&mut choice).ok();
    let picked = choice
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|i| menu.get(i).copied());

    let picked_name: Option<(&str, Option<String>)> = match picked {
        None => {
            println!("No provider selected — writing niki.toml with provider entries only.");
            println!("Add a key later with `niki auth login` (or set its env var).");
            None
        }
        Some(("ollama", _, _, _)) => {
            let (model, installed) = crate::cli::auth::preferred_ollama_model();
            if installed {
                println!("\nOllama selected — agents will use installed model `{model}`.");
            } else {
                println!("\nOllama selected but no models are installed.");
                println!("Pull one first: `ollama pull {model}`, then re-run the wizard.");
                println!("Writing the config anyway with `{model}` pre-selected.\n");
            }
            Some(("ollama", Some(model)))
        }
        Some((name, label, env_var, _)) => {
            println!("--- {} ---", label);

            if std::env::var(env_var)
                .map(|k| !k.is_empty())
                .unwrap_or(false)
            {
                println!("  Found {} in your environment", env_var);
                if prompt_yes_no("  Store in OS keyring?") {
                    crate::cli::auth::handle(&crate::cli::auth::AuthCommands::Login {
                        provider: Some(name.to_string()),
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
                        provider: Some(name.to_string()),
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
            println!("Set up more any time with `niki auth login --provider <name>`.");
            Some((name, None))
        }
    };

    // Point all four agents at the picked provider so the written config runs
    // as-is. Without this the template's Anthropic defaults survive and the
    // first `niki run` fails with a missing-key error on a fresh machine.
    let mut out = example_content.to_string();
    if let Some((name, model)) = picked_name {
        out = point_agents_at(&out, name, model.as_deref());
        println!("Agents (planner/coder/tester/reviewer) set to `{name}` in niki.toml.");
    }
    fs::write(target_path, out)?;
    println!("Created niki.toml with provider entries.");
    println!("API keys have been stored in your OS keyring where provided.");
    println!("Run `niki doctor` to verify your setup.");
    Ok(())
}

/// Rewrite the four `[agents.*]` sections of a niki.toml template so they use
/// `provider` instead of the template defaults. Commented lines are untouched
/// (they start with `#`, never with `provider`/`model` after trimming).
/// `model` overrides the model lines too (used for Ollama, where
/// provider-specific defaults like `claude-sonnet-4-20250514` would 404);
/// other providers keep the template's models for the user to adjust.
fn point_agents_at(toml_text: &str, provider: &str, model: Option<&str>) -> String {
    const AGENTS: [&str; 4] = [
        "[agents.planner]",
        "[agents.coder]",
        "[agents.tester]",
        "[agents.reviewer]",
    ];
    let mut section = String::new();
    toml_text
        .lines()
        .map(|line| {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                section = t.to_string();
            }
            if AGENTS.contains(&section.as_str()) {
                if t.starts_with("provider ") || t.starts_with("provider=") {
                    return format!("provider = \"{provider}\"");
                }
                if let Some(m) = model
                    && (t.starts_with("model ") || t.starts_with("model="))
                {
                    return format!("model = \"{m}\"");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
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

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r#"[general]
max_revision_rounds = 1

[agents.planner]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[agents.coder]
provider="anthropic"
model="claude-sonnet-4-20250514"

[providers.anthropic]
# provider = "commented-out example line stays"

[agents.tester]
provider = "groq"
model = "llama-3.1-70b-versatile"
"#;

    #[test]
    fn wizard_rewrites_agent_providers_and_keeps_rest() {
        let out = point_agents_at(MINI, "zen", None);
        // All agent sections rewritten (both `=` spacing styles).
        assert_eq!(out.matches("provider = \"zen\"").count(), 3);
        assert!(!out.contains("anthropic\""));
        assert!(!out.contains("groq\""));
        // Commented example and non-agent sections untouched.
        assert!(out.contains("# provider = \"commented-out example line stays\""));
        assert!(out.contains("[providers.anthropic]"));
        // Models untouched when no override given.
        assert!(out.contains("model = \"claude-sonnet-4-20250514\""));
    }

    #[test]
    fn wizard_ollama_override_rewrites_models() {
        let out = point_agents_at(MINI, "ollama", Some("qwen2.5-coder:3b"));
        assert_eq!(out.matches("model = \"qwen2.5-coder:3b\"").count(), 3);
        assert_eq!(out.matches("provider = \"ollama\"").count(), 3);
    }

    #[test]
    fn ollama_model_probe_returns_a_usable_model_either_way() {
        // The old assertion was `assert!(!model.is_empty())`, which cannot
        // fail: the fallback at the call site is a non-empty literal. What
        // actually matters is that the returned name is one a user could pass
        // to `ollama run`, and that the probe reports honestly about where it
        // came from.
        let (model, detected) = crate::cli::auth::preferred_ollama_model();
        assert!(
            !model.trim().is_empty(),
            "the probe must always name a model to fall back on"
        );
        assert!(
            !model.contains(char::is_whitespace),
            "a model name with whitespace cannot be passed to `ollama run`: {model:?}"
        );
        // `detected` is the signal separating "found your installed model"
        // from "guessed a default", and the fallback branch is fully
        // determined: with nothing detected, the answer is the documented
        // default. The detected branch deliberately asserts nothing about the
        // name beyond usability — a locally installed model may legitimately
        // share the default's name, so "detected != default" would be wrong.
        if !detected {
            assert_eq!(
                model, "qwen2.5-coder:3b",
                "with no Ollama detected the probe must return the documented default"
            );
        }
    }
}
