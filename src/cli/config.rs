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
    /// Show every effective value and which layer it came from
    Explain,
}

pub async fn handle(command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Init { interactive, scan } => cmd_init(*interactive, *scan).await,
        ConfigCommands::Schema => cmd_schema(),
        ConfigCommands::Check => cmd_check(),
        ConfigCommands::Explain => cmd_explain(),
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

/// One setting worth explaining: where the value lives, the environment variable that
/// overrides it, and whether printing the value is safe.
///
/// The `secret` flag is not decoration. `ANTHROPIC_API_KEY` is a real setting with a real
/// source, and a report that prints it turns a diagnostic into a credential written to a
/// terminal, a scrollback buffer and a CI log.
struct Tracked {
    /// Dotted path into `niki.toml`, e.g. `general.spend_cap_usd`.
    path: &'static str,
    /// The env var that beats every file, or `None` when nothing does.
    env: Option<&'static str>,
    /// What the loader uses when no file and no env set it.
    default: &'static str,
    secret: bool,
}

const fn setting(path: &'static str, default: &'static str) -> Tracked {
    Tracked {
        path,
        env: None,
        default,
        secret: false,
    }
}

const fn provider_setting(
    path: &'static str,
    env: &'static str,
    default: &'static str,
    secret: bool,
) -> Tracked {
    Tracked {
        path,
        env: Some(env),
        default,
        secret,
    }
}

/// Only settings whose environment override the loader actually honours are listed with one.
///
/// The first version of this table named `NIKI_PLANNER_MODEL` and friends. Nothing reads those
/// variables, so the report would have claimed a file was overridden by an environment that
/// does not exist — a lie a user would act on. The names below are the ones
/// `NikiConfig::apply_env_lookup` reads.
const TRACKED: &[Tracked] = &[
    setting("general.max_revision_rounds", "3"),
    setting("general.output_dir", ".niki"),
    setting("general.spend_cap_usd", "0.0"),
    setting("general.max_diff_lines", "0"),
    setting("general.max_context_chars", "48000"),
    setting("docker.backend", "docker"),
    setting("permissions.mode", "manual"),
    setting("agents.coder.provider", "(provider default)"),
    provider_setting(
        "providers.anthropic.api_key",
        "ANTHROPIC_API_KEY",
        "(unset)",
        true,
    ),
    provider_setting(
        "providers.anthropic.base_url",
        "ANTHROPIC_BASE_URL",
        "(unset)",
        false,
    ),
    provider_setting(
        "providers.anthropic.default_model",
        "ANTHROPIC_MODEL",
        "(provider default)",
        false,
    ),
    provider_setting(
        "providers.openai.api_key",
        "OPENAI_API_KEY",
        "(unset)",
        true,
    ),
    provider_setting(
        "providers.openai.base_url",
        "OPENAI_BASE_URL",
        "(unset)",
        false,
    ),
    provider_setting(
        "providers.openai.default_model",
        "OPENAI_MODEL",
        "(provider default)",
        false,
    ),
];

/// Show every effective value and which layer produced it.
///
/// The reason this exists: layering is invisible until it is wrong. Three files, an environment
/// and a pile of defaults, and a setting that is not doing what the file says has no symptom
/// other than the setting being wrong. `niki config check` says whether a file *parses*; this
/// says what the loader actually used and where it came from.
///
/// The precedence is the loader's own, taken from `NikiConfig::apply_env_lookup` and
/// `NikiConfig::load`: a non-empty environment variable wins where the loader honours one, and
/// otherwise the project file beats the user file beats the built-in default. Reporting a
/// source the loader would not have used is worse than reporting nothing.
fn cmd_explain() -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let project_file = project_dir.join("niki.toml");
    let user_file = dirs::home_dir().map(|h| h.join(".config/niki/niki.toml"));

    let read = |p: &std::path::Path| -> Option<toml::Value> {
        if !p.exists() {
            return None;
        }
        fs::read_to_string(p)
            .ok()
            .and_then(|c| c.parse::<toml::Value>().ok())
    };
    let project = read(&project_file);
    let user = user_file.as_deref().and_then(read);

    println!("NIKI configuration — effective values and where each came from");
    println!();
    println!("  project file: {}", project_file.display());
    println!(
        "  user file:    {}",
        user_file
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into())
    );
    println!();
    println!("  {:<38} {:<24} source", "setting", "value");
    println!("  {}", "-".repeat(78));

    for t in TRACKED {
        // Walk the dotted path. A layer only wins if it actually carries the key, so a file that
        // sets `general.output_dir` does not mask a `docker.backend` in the same file.
        let lookup = |doc: Option<&toml::Value>| -> Option<String> {
            let mut node = doc?;
            for part in t.path.split('.') {
                node = node.get(part)?;
            }
            match node.as_str() {
                Some(s) => Some(s.to_string()),
                None => Some(node.to_string()),
            }
        };

        let env_value = t
            .env
            .and_then(|name| std::env::var(name).ok())
            .filter(|v| !v.is_empty());

        // Env first, then project, then user, then default — the loader's order.
        let (value, source) = if let Some(v) = env_value {
            (v, format!("environment {}", t.env.unwrap_or_default()))
        } else if let Some(v) = lookup(project.as_ref()) {
            (v, format!("{}", project_file.display()))
        } else if let Some(v) = lookup(user.as_ref()) {
            (
                v,
                user_file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "user file".into()),
            )
        } else {
            (t.default.to_string(), "built-in default".to_string())
        };

        // A secret reports that it is set, never what it is.
        let shown = if t.secret && source != "built-in default" {
            "(set)".to_string()
        } else {
            value
        };
        println!("  {:<38} {:<24} {}", t.path, shown, source);
    }

    println!();
    println!(
        "  Precedence: environment, then project file, then user file, then built-in default."
    );
    println!("  Secret values are reported as set or unset, never printed.");
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
    // a short timeout.
    let ollama_up = crate::cli::auth::ollama_running();
    if ollama_up {
        println!("  Ollama is running locally (127.0.0.1:11434) — no API key needed.");
    } else {
        println!(
            "  Ollama not detected locally (optional — fully offline models via http://localhost:11434)."
        );
    }

    // The sandbox backend, detected rather than assumed.
    //
    // The wizard used to write a provider and a model and stop there, leaving
    // `[docker] backend` at its default of `docker`. On a machine with no
    // container runtime — which the README says is a supported way to run NIKI,
    // and which is the *first* thing it advertises — that produced a config the
    // setup wizard declared successful and the very next `niki run` could not
    // use. The failure was legible, but it was three steps too late, and
    // `niki doctor` had already told the user to go install Podman.
    let detected_runtime = crate::sandbox::detect_container_runtime();
    let default_backend = crate::sandbox::default_backend_for_this_machine();
    match &detected_runtime {
        Some(rt) => println!("  Container runtime found: {rt}"),
        None => println!(
            "  No container runtime found — NIKI will use the git-worktree backend, \
             which needs no container and runs agent commands as local processes."
        ),
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
    // Always offer Ollama, whether or not it is up right now.
    //
    // It used to appear only when `ollama_running()` answered, which hid the
    // one provider that needs no key, no account and no spend at exactly the
    // moment it was the right answer: a user who has not started Ollama yet.
    // The choice they need is "how do I get a model", not "what is already
    // running" — so it is listed either way, and the state is reported
    // alongside it.
    menu.push(("ollama", "Ollama (local, no key)", "", KeyState::Missing));
    for (name, label, env_var) in PROVIDERS {
        if *name == "ollama" {
            continue; // offered as menu option 0 above
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
    for (i, (name, label, env_var, state)) in menu.iter().enumerate() {
        let marker = if *name == "ollama" {
            // Reported, not filtered on: the option is always present, and the
            // state tells the user what to do next if nothing is listening yet.
            if ollama_up {
                "(running)".to_string()
            } else {
                "(not detected — run `ollama pull <model>` after setup)".to_string()
            }
        } else {
            match state {
                KeyState::Env => format!("(via {})", env_var),
                KeyState::Keyring => "(in keyring)".to_string(),
                KeyState::Missing => String::new(),
            }
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
    if let Some((name, ref model)) = picked_name {
        out = point_agents_at(&out, name, model.as_deref());
        println!("Agents (planner/coder/tester/reviewer) set to `{name}` in niki.toml.");
    }

    // …and set the sandbox backend to the one this machine can actually run, so
    // the config the wizard just wrote is one the next command can use. The
    // template ships `# backend = "docker"` commented out, meaning "docker" —
    // which is right for a machine with Podman and wrong for every machine
    // without it.
    out = set_backend(&out, default_backend);
    let backend_word = match default_backend {
        crate::sandbox::SandboxBackend::Docker => "docker (container isolation)",
        crate::sandbox::SandboxBackend::Worktree => {
            "worktree (git worktree + local process; no container runtime needed)"
        }
    };
    println!("Sandbox backend set to `{backend_word}` in niki.toml.");

    fs::write(target_path, out)?;
    println!("Created niki.toml with provider entries.");
    println!("API keys have been stored in your OS keyring where provided.");
    println!("Run `niki doctor` to verify your setup.");

    // A wizard that reports success onto a config that cannot run is worse than
    // one that reports the problem, because the user stops reading. The two
    // cases where the file is written but the machine is not ready are named
    // explicitly, and the command exits non-zero so a script can act on it.
    let mut incomplete: Vec<String> = Vec::new();
    if picked_name.is_none() {
        incomplete.push(
            "No provider was configured — the file still points at the template defaults. \
             Run `niki auth login --provider <name>`, or re-run `niki init --interactive`."
                .to_string(),
        );
    }
    if let Some(("ollama", _)) = picked_name
        && !ollama_up
    {
        let (model, installed) = crate::cli::auth::preferred_ollama_model();
        if !installed {
            incomplete.push(format!(
                "Ollama is not running and no model is pulled. Start it and run \
                 `ollama pull {model}` — until then every stage will fail to get a model."
            ));
        }
    }
    if detected_runtime.is_none() {
        println!(
            "\nNote: no container runtime on this machine, so the worktree backend is \
             selected. It runs agent commands as local processes with your privileges; \
             install Podman if you want container isolation."
        );
    }

    if !incomplete.is_empty() {
        println!("\nniki.toml was written, but setup is not finished:");
        for item in &incomplete {
            println!("  - {item}");
        }
        anyhow::bail!(
            "setup incomplete: {} item(s) need attention",
            incomplete.len()
        );
    }
    Ok(())
}

/// Turn the template's commented-out `# backend = "docker"` into a real setting.
///
/// Deliberately a text rewrite rather than a serde round-trip: the file the
/// wizard writes is the full documented template, comments and all, and
/// re-serialising it would strip every one of those comments — which is most of
/// what makes the file useful to read. So the same discipline `point_agents_at`
/// uses: replace the line in place, leave everything else byte-identical.
fn set_backend(toml_text: &str, backend: crate::sandbox::SandboxBackend) -> String {
    let value = match backend {
        crate::sandbox::SandboxBackend::Docker => "docker",
        crate::sandbox::SandboxBackend::Worktree => "worktree",
    };
    let mut replaced = false;
    let mut out = String::with_capacity(toml_text.len() + 32);
    for line in toml_text.lines() {
        let t = line.trim();
        // Either the commented default the template ships, or an existing active
        // setting the user edited before re-running the wizard.
        if t == "# backend = \"docker\""
            || t == "# backend = \"worktree\""
            || t.starts_with("backend = ")
        {
            out.push_str(&format!("backend = \"{value}\"\n"));
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        // No `[docker] backend` line at all. Append a clearly-marked one rather
        // than guessing where the section belongs — an absent line is a
        // template change, and a wrong guess would corrupt the file.
        out.push_str(&format!(
            "\n# Added by `niki init --interactive`: this machine can run the \
             worktree backend without a container runtime.\n[docker]\nbackend = \"{value}\"\n"
        ));
    }
    out
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

    // The template line the backend rewrite targets. If this ever changes, the
    // rewrite silently stops matching and the wizard goes back to writing a
    // config that selects a backend the machine cannot run — which is the exact
    // failure these tests exist to prevent.
    const TEMPLATE_BACKEND_LINE: &str = "# backend = \"docker\"";

    /// The wizard must write a backend the machine can actually run.
    ///
    /// It used to write none at all, so every machine without a container
    /// runtime — the machine the README opens by describing — got a config whose
    /// first `niki run` could not start. That is a one-line omission with a
    /// three-step failure, so it is pinned here.
    #[test]
    fn wizard_writes_a_backend_the_machine_can_run() {
        let template =
            format!("[docker]\nbase_image = \"niki-sandbox:24.04\"\n{TEMPLATE_BACKEND_LINE}\n");
        for backend in [
            crate::sandbox::SandboxBackend::Docker,
            crate::sandbox::SandboxBackend::Worktree,
        ] {
            let out = set_backend(&template, backend);
            let word = match backend {
                crate::sandbox::SandboxBackend::Docker => "docker",
                crate::sandbox::SandboxBackend::Worktree => "worktree",
            };
            assert!(
                out.contains(&format!("backend = \"{word}\"")),
                "the written config does not select {word}:\n{out}"
            );
            assert!(
                !out.contains(TEMPLATE_BACKEND_LINE),
                "the commented default survived, so the active line is absent or \
                 duplicated and TOML parsing would see a duplicate key:\n{out}"
            );
            // Whatever the wizard writes has to be readable, or the wizard is
            // not producing a config but a file.
            let parsed: crate::config::NikiConfig =
                toml::from_str(&out).expect("written config must parse");
            assert_eq!(
                parsed.docker.backend, backend,
                "round-trip lost the backend"
            );
        }
    }

    /// Re-running the wizard must not accumulate settings, and an already-active
    /// `backend = ` line the user wrote by hand must be replaced rather than
    /// duplicated — a duplicate key is a parse error, so this failure would be
    /// "niki.toml is broken" with no obvious cause.
    #[test]
    fn wizard_is_idempotent_and_replaces_a_hand_written_backend() {
        let template = format!("[docker]\n{TEMPLATE_BACKEND_LINE}\n");
        let once = set_backend(&template, crate::sandbox::SandboxBackend::Worktree);
        let twice = set_backend(&once, crate::sandbox::SandboxBackend::Worktree);
        assert_eq!(
            once.matches("backend = ").count(),
            1,
            "expected exactly one active backend line:\n{once}"
        );
        assert_eq!(once, twice, "re-running the wizard changed the file");

        let hand_written = "[docker]\nbackend = \"docker\"\n";
        let replaced = set_backend(hand_written, crate::sandbox::SandboxBackend::Worktree);
        assert_eq!(
            replaced.matches("backend = ").count(),
            1,
            "the hand-written line was duplicated rather than replaced:\n{replaced}"
        );
        assert!(replaced.contains("backend = \"worktree\""));
    }

    /// The full template is what the wizard actually writes. This is the case
    /// that reached users, so it is the case that is pinned.
    #[test]
    fn wizard_output_parses_as_a_whole_config() {
        let example = include_str!("../../niki.example.toml");
        let out = set_backend(
            &point_agents_at(example, "ollama", Some("qwen2.5-coder:3b")),
            crate::sandbox::SandboxBackend::Worktree,
        );
        let parsed: crate::config::NikiConfig =
            toml::from_str(&out).expect("the wizard's output must be a valid niki.toml");
        assert_eq!(
            parsed.docker.backend,
            crate::sandbox::SandboxBackend::Worktree
        );
        // The wizard points every agent at the chosen provider so the first run
        // does not need a second edit.
        for (role, agent) in [
            ("planner", &parsed.agents.planner),
            ("coder", &parsed.agents.coder),
            ("tester", &parsed.agents.tester),
            ("reviewer", &parsed.agents.reviewer),
        ] {
            assert_eq!(
                agent.provider, "ollama",
                "{role} still points at the template default, so the first run needs a second edit"
            );
        }
    }

    /// The rewrite must not eat the documentation. The whole reason the wizard
    /// writes the full template rather than a minimal config is that the
    /// comments are what make it useful to read; a rewrite that dropped them
    /// would quietly remove the reason for the format.
    #[test]
    fn backend_rewrite_preserves_the_rest_of_the_file() {
        let template = format!(
            "# Sandbox backend: \"docker\" (container, default) or \"worktree\".\n\
             [docker]\n\
             # A comment that must survive.\n\
             base_image = \"niki-sandbox:24.04\"\n\
             {TEMPLATE_BACKEND_LINE}\n\
             [security]\n\
             enabled = true\n"
        );
        let out = set_backend(&template, crate::sandbox::SandboxBackend::Worktree);
        assert!(
            out.contains("# A comment that must survive."),
            "comments were dropped:\n{out}"
        );
        assert!(out.contains("base_image = \"niki-sandbox:24.04\""));
        assert!(out.contains("[security]"));
        assert!(out.contains("enabled = true"));
    }

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
