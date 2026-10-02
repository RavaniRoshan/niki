use crate::config::NikiConfig;
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProviderCommands {
    /// Check health of all configured providers (sends a minimal test request)
    Check,
    /// List the models a provider actually offers.
    ///
    /// The catalogue is fetched, never assumed. `niki recommend` carries a
    /// hardcoded table, which cannot know what your account can reach — and on
    /// OpenRouter that table is wrong by construction: several hundred models,
    /// fully-qualified names, and a per-model effort control that the name
    /// tells you nothing about.
    Models {
        /// Which provider. Defaults to every configured one.
        #[arg(long)]
        provider: Option<String>,
        /// Print one model per line, for scripting.
        #[arg(long)]
        plain: bool,
    },
}

#[derive(clap::Args)]
pub struct ProvidersArgs {
    #[command(subcommand)]
    pub command: ProviderCommands,
}

pub async fn handle(args: &ProvidersArgs) -> Result<()> {
    match &args.command {
        ProviderCommands::Check => handle_check().await,
        ProviderCommands::Models { provider, plain } => {
            handle_models(provider.as_deref(), *plain).await
        }
    }
}

/// Make a model id from a provider's `/models` response safe to print.
///
/// Two controls, both needed, neither sufficient alone.
///
/// `sanitize_for_terminal` strips escape sequences, because a model id is
/// untrusted text that reaches a terminal. `redact_secrets` masks credentials,
/// because the id arrives over a request that carried `Authorization: Bearer
/// <key>` — so a provider, a corporate gateway, or anything else the user
/// points `base_url` at can echo the key straight back into a field the
/// terminal then prints. Without the second control the user's own API key
/// lands in their scrollback and in CI logs, which is what CodeQL's
/// `cleartext-logging` alert at this `println!` is describing.
///
/// The other four provider response surfaces — the error bodies in
/// `anthropic.rs`, `openai.rs`, `google.rs` and `ollama.rs` — already pass
/// through `redact_secrets`. The catalogue was the one surface that did not.
///
/// Expect a second alert here and do not "fix" it by adding redaction to the
/// neighbouring `println!`. CodeQL taints the whole `Vec<CatalogueEntry>`
/// because `fetch` takes the key as a parameter, so `models.len()` inherits
/// that taint and the count-printing statement gets flagged too. That one
/// prints a provider name and an integer; there is no secret-shaped content on
/// the path. See `EVIDENCE.md`.
fn safe_model_id(id: &str) -> String {
    crate::display::sanitize::sanitize_for_terminal(crate::llm::provider::redact_secrets(id))
}

/// List what a provider can actually serve, and say plainly when it cannot.
///
/// The failure modes are the interesting part. A provider with no catalogue
/// endpoint, a key that is not authorised for one, and a base URL that is
/// wrong are three different problems with three different fixes, and a single
/// "could not list models" would send a user to the wrong one every time.
async fn handle_models(provider: Option<&str>, plain: bool) -> Result<()> {
    let config = NikiConfig::load(std::path::Path::new("."))?;

    // Not `config.providers.is_empty()`. That map always holds twelve entries
    // — `apply_env_lookup` seeds a slot for every known provider — so the
    // guard could never fire, and `niki providers models` on a fresh install
    // printed twelve "no key" failures rather than the one useful sentence.
    let configured: Vec<String> = config
        .providers
        .iter()
        .filter(|(_, c)| c.is_configured())
        .map(|(n, _)| n.clone())
        .collect();

    let names: Vec<String> = match provider {
        Some(p) => vec![p.to_string()],
        None if configured.is_empty() => {
            println!(
                "No providers configured. Add one to `niki.toml`, or set \
                 ANTHROPIC_API_KEY, OPENAI_API_KEY or OPENROUTER_API_KEY in the \
                 environment, then run this again."
            );
            return Ok(());
        }
        None => configured,
    };

    let mut any = false;
    for name in names {
        let cfg = config.providers.get(&name);
        let base = cfg
            .and_then(|c| c.base_url.clone())
            .or_else(|| crate::llm::provider::default_base_url(&name).map(str::to_string));
        // Config first, then the env, then the keyring — the same order
        // `auth login` and the request path use, so a model list is fetched
        // with the credential the run would actually use.
        let key = cfg
            .and_then(|c| c.api_key.clone())
            .or_else(|| crate::cli::auth::resolve_api_key(&name));

        match crate::cli::catalogue::fetch(&name, base.as_deref(), key.as_deref()).await {
            Ok(models) if models.is_empty() => {
                if !plain {
                    println!("{name}: catalogue is empty");
                }
            }
            Ok(models) => {
                any = true;
                if plain {
                    for m in &models {
                        let safe_id = safe_model_id(&m.id);
                        println!("{name}\t{}", safe_id);
                    }
                } else {
                    println!("\n{name} — {} model(s):", models.len());
                    for m in &models {
                        let safe_id = safe_model_id(&m.id);
                        let mut notes: Vec<String> = Vec::new();
                        if m.traits
                            .contains(&crate::cli::catalogue::ModelTrait::Reasoning)
                        {
                            notes.push("reasoning (effort control likely)".into());
                        }
                        if m.traits.contains(&crate::cli::catalogue::ModelTrait::Free) {
                            notes.push("free tier".into());
                        }
                        if let Some((Some(p), _)) = m.price_per_mtok {
                            notes.push(format!("${p}/Mtok in"));
                        }
                        if notes.is_empty() {
                            println!("  {}", safe_id);
                        } else {
                            println!("  {}  [{}]", safe_id, notes.join(", "));
                        }
                    }
                }
            }
            Err(e) => {
                if !plain {
                    println!("{name}: {e}");
                }
            }
        }
    }

    if !any && !plain {
        println!(
            "\nNo catalogue could be read. That is normal for a provider without a \
             `/models` endpoint, and it does not stop you using niki — set a model \
             explicitly in niki.toml."
        );
    }
    Ok(())
}

async fn handle_check() -> Result<()> {
    let config = NikiConfig::load(std::path::Path::new("."))?;

    // Same dead guard as in `handle_models`, and it cost more here: every
    // seeded slot was health-checked, so a machine with no keys printed
    // twelve red crosses and `0/12 providers healthy` — which reads as "you
    // have twelve providers and they are all broken" rather than "you have
    // none". `check_provider_health` filters the empty slots too, so both
    // halves of the count agree.
    if !config.providers.values().any(|c| c.is_configured()) {
        println!(
            "No providers configured. Add one to `niki.toml`, or set \
             ANTHROPIC_API_KEY, OPENAI_API_KEY or OPENROUTER_API_KEY in the \
             environment, then run this again."
        );
        return Ok(());
    }

    println!("Checking provider health...\n");

    // Await directly: this runs inside the Tokio runtime, so spawning a
    // nested runtime here panics ("Cannot start a runtime from within a
    // runtime" — found by live-provider verification).
    let results = crate::llm::failover::check_provider_health(&config.providers).await;

    let mut all_ok = true;
    for r in &results {
        let status = if r.ok { "✓" } else { "✗" };
        let latency = format!("{}ms", r.latency_ms);
        // Prefer the model the check actually used over the config default
        // (they differ when a default was resolved, e.g. Ollama).
        let model = r.model.as_deref().unwrap_or_else(|| {
            config
                .providers
                .get(&r.provider)
                .map(|p| p.default_model.as_str())
                .unwrap_or("unknown")
        });

        if r.ok {
            println!(
                "  {} {} ({}) — {} — healthy",
                status, r.provider, model, latency
            );
        } else {
            all_ok = false;
            println!(
                "  {} {} ({}) — {} — {}",
                status,
                r.provider,
                model,
                latency,
                r.error.as_deref().unwrap_or("unknown error")
            );
        }
    }

    println!();
    if all_ok {
        println!("All {} providers healthy.", results.len());
    } else {
        let healthy = results.iter().filter(|r| r.ok).count();
        let total = results.len();
        println!("{}/{} providers healthy.", healthy, total);
    }

    Ok(())
}
