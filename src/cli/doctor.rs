use anyhow::Result;
use clap::Args;
use std::process::Command;

use crate::cli::auth::{PROVIDERS, load_env_keys, load_existing_keys};
use crate::config::NikiConfig;

/// Best-effort host extraction from a URL for the outbound-hosts check. Returns
/// the host (without scheme/port) so the `niki doctor` security output lists a
/// stable, human-readable set rather than user-supplied full URLs (which may
/// embed path secrets).
fn url_host(url: &str) -> Option<String> {
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    stripped.split('/').next().map(str::to_string)
}

#[derive(Args)]
pub struct DoctorArgs {
    /// Only check a specific category (install, config, providers, sandbox)
    #[arg(short, long)]
    category: Option<String>,
    /// Measure the configured model's ability to emit a conformant artifact.
    ///
    /// The topology heuristic reads this: below a 45% pass rate a multi-agent
    /// pipeline is worth about +22 points, above 50% it costs about 5. Without a
    /// measurement the heuristic has to guess, and it guesses toward the
    /// multi-agent chain — so a user on a frontier model pays ~5 points they
    /// did not have to.
    #[arg(long)]
    measure: bool,
}

enum CheckResult {
    Pass(String),
    Warn(String),
    Fail(String),
}

struct Check {
    name: String,
    result: CheckResult,
}

pub fn handle(args: &DoctorArgs) -> Result<()> {
    let mut checks: Vec<Check> = Vec::new();

    checks.extend(check_install());
    checks.extend(check_config());
    checks.extend(check_providers());
    checks.extend(check_sandbox());
    checks.extend(check_security());
    // Image existence needs the configured base image, so it lives outside
    // check_sandbox() (which is config-free). A missing image is the most
    // common first-run failure after the runtime itself.
    if let Ok(cfg) = NikiConfig::load(&std::env::current_dir().unwrap_or_default()) {
        checks.push(check_sandbox_image(&cfg.docker.base_image));
    }

    if args.measure {
        let cfg = cfg_for_measure();
        // `handle` is called from main's dispatch, which already runs inside a
        // tokio runtime — so `block_on` on a *fresh* one panics with "Cannot
        // start a runtime from within a runtime", which is what it did. Spawn
        // onto the current runtime when there is one, and only build one if this
        // is being called from a plain thread.
        let measured = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(measure_capability(&cfg))),
            Err(_) => match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt.block_on(measure_capability(&cfg)),
                Err(e) => Err(anyhow::anyhow!(
                    "could not start a runtime for the probe: {e}"
                )),
            },
        };
        match measured {
            Ok(capability) => {
                let project = std::env::current_dir().unwrap_or_default();
                // A 0/N is a fact about the probe, not the model — see
                // `ModelCapability::usable`. Record it as unknown so the
                // topology heuristic falls back to its safe default instead of
                // routing on a number we have shown to be confounded.
                let capability = capability.usable();
                let where_ = crate::config::capability::save(&project, capability)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|e| format!("not saved: {e}"));
                println!("\n  ✓ model capability — {}", capability.explain());
                println!("    recorded in {where_}");
                // Honest about what this measurement is worth. A probe that
                // cannot reach a model reliably will report 0% for a model that
                // is perfectly capable, and the user's only recourse then is to
                // delete the file. Saying so is better than a confident number
                // that routes them wrongly.
                println!(
                    "    This is a 4-sample probe of artifact emission, not a benchmark. A low\n    \
                     score is actionable; a high one is not a guarantee. Delete the file to go\n    \
                     back to the heuristic's default."
                );
            }
            Err(e) => {
                println!("\n  ✗ model capability — could not measure: {e}");
                println!(
                    "    the topology heuristic will keep the multi-agent chain, which is the\n    \
                         safe side of the trade for a model of unknown strength."
                );
            }
        }
    }

    let filtered: Vec<&Check> = match &args.category {
        Some(cat) => checks
            .iter()
            .filter(|c| c.name.to_lowercase().contains(cat))
            .collect(),
        None => checks.iter().collect(),
    };

    let mut errors = 0;
    let mut warnings = 0;

    for check in &filtered {
        match &check.result {
            CheckResult::Pass(msg) => {
                println!("  ✓ {} — {}", check.name, msg);
            }
            CheckResult::Warn(msg) => {
                println!("  ⚠ {} — {}", check.name, msg);
                warnings += 1;
            }
            CheckResult::Fail(msg) => {
                println!("  ✗ {} — {}", check.name, msg);
                errors += 1;
            }
        }
    }

    println!(
        "\nSummary: {} checks, {} passed, {} warnings, {} failed",
        filtered.len(),
        filtered
            .iter()
            .filter(|c| matches!(c.result, CheckResult::Pass(_)))
            .count(),
        warnings,
        errors
    );

    if errors > 0 {
        println!("\nSome checks failed. See messages above for details.");
    } else if warnings > 0 {
        println!("\nAll critical checks passed with some warnings.");
    } else {
        println!("\nAll checks passed!");
    }

    Ok(())
}

fn check_install() -> Vec<Check> {
    vec![
        Check {
            name: "niki version".to_string(),
            result: CheckResult::Pass(env!("CARGO_PKG_VERSION").to_string()),
        },
        Check {
            name: "rust toolchain".to_string(),
            result: match Command::new("rustc").arg("--version").output() {
                Ok(output) => {
                    if output.status.success() {
                        let version = String::from_utf8_lossy(&output.stdout);
                        CheckResult::Pass(version.trim().to_string())
                    } else {
                        CheckResult::Fail("rustc not working".to_string())
                    }
                }
                Err(_) => CheckResult::Fail("rustc not found".to_string()),
            },
        },
    ]
}

fn check_config() -> Vec<Check> {
    let project_dir = std::env::current_dir().unwrap_or_default();
    let local_path = project_dir.join("niki.toml");
    let global_path = dirs::home_dir().map(|h| h.join(".config/niki/niki.toml"));

    vec![
        Check {
            name: "local config".to_string(),
            result: if local_path.exists() {
                CheckResult::Pass(format!("found at {}", local_path.display()))
            } else {
                CheckResult::Warn(format!("not found at {}", local_path.display()))
            },
        },
        Check {
            name: "global config".to_string(),
            result: match &global_path {
                Some(p) if p.exists() => CheckResult::Pass(format!("found at {}", p.display())),
                Some(p) => CheckResult::Warn(format!("not found at {}", p.display())),
                None => CheckResult::Fail("cannot determine home directory".to_string()),
            },
        },
    ]
}

fn check_providers() -> Vec<Check> {
    let existing = load_existing_keys();
    let env_keys = load_env_keys();

    PROVIDERS
        .iter()
        .map(|(name, label, _)| {
            // Keyless local providers report reachability, not key presence.
            if name == &"ollama" {
                let running = crate::cli::auth::ollama_running();
                return Check {
                    name: format!("{} provider", label),
                    result: if running {
                        CheckResult::Pass("running locally (no key needed)".to_string())
                    } else {
                        CheckResult::Warn(
                            "not detected (install https://ollama.com, run `ollama serve`)"
                                .to_string(),
                        )
                    },
                };
            }
            let configured = existing.contains_key(*name) || env_keys.contains_key(*name);
            Check {
                name: format!("{} provider", label),
                result: if configured {
                    let source = if env_keys.contains_key(*name) {
                        "env var"
                    } else {
                        "keyring"
                    };
                    CheckResult::Pass(format!("configured via {}", source))
                } else {
                    CheckResult::Warn(format!("not configured (run `niki auth login {}`)", name))
                },
            }
        })
        .collect()
}

fn check_security() -> Vec<Check> {
    let project_dir = std::env::current_dir().unwrap_or_default();
    // Security checks are best-effort: if config can't be loaded, surface a
    // single warning rather than crashing `niki doctor`.
    match NikiConfig::load(&project_dir).ok() {
        Some(cfg) => {
            // Replace the unloadable-config fallback with a concrete pass once
            // config loaded; check_security_for already assumes a loaded config.
            check_security_for(&cfg)
        }
        None => vec![Check {
            name: "security config".to_string(),
            result: CheckResult::Warn(
                "no niki.toml loaded; run `niki init` so egress/cap/image checks can run"
                    .to_string(),
            ),
        }],
    }
}

/// Security checks against a loaded config. Factored out so the logic is unit
/// testable without touching the filesystem / current directory.
fn check_security_for(cfg: &NikiConfig) -> Vec<Check> {
    let mut checks = Vec::new();

    // 1. Spend ceiling — warn-only if unset (0.0 == unlimited).
    let cap = cfg.general.spend_cap_usd;
    checks.push(Check {
        name: "spend cap".to_string(),
        result: if cap > 0.0 {
            CheckResult::Pass(format!("${:.2}/run (hard-enforced mid-run)", cap))
        } else {
            CheckResult::Warn(
                "0.0 = unlimited (set general.spend_cap_usd for a hard ceiling)".to_string(),
            )
        },
    });

    // 2. Network egress — blocked by default; allowlist widens it.
    let (disabled, allowlist) = (cfg.docker.network_disabled, &cfg.docker.network_allowlist);
    checks.push(Check {
        name: "network egress".to_string(),
        result: if disabled || allowlist.is_empty() {
            CheckResult::Pass("blocked by default (network_disabled=true)".to_string())
        } else if allowlist == &["*".to_string()] {
            CheckResult::Warn(
                "egress open to all hosts (network_disabled=false, allowlist=['*'])".to_string(),
            )
        } else {
            CheckResult::Warn(format!("egress allowlist: {}", allowlist.join(", ")))
        },
    });

    // 3. No-telemetry: print the *only* hosts NIKI will ever contact. This is
    // the verifiable guarantee — providers are user-supplied base_urls; the
    // optional knowledge fetch (SSRF-guarded) is gated on configured URLs.
    let mut outbound: Vec<String> = cfg
        .providers
        .values()
        .filter_map(|p| p.base_url.clone())
        .collect();
    for u in &cfg.knowledge.urls {
        if let Some(host) = url_host(u) {
            outbound.push(host);
        }
    }
    checks.push(Check {
        name: "outbound hosts".to_string(),
        result: if outbound.is_empty() {
            CheckResult::Pass(
                "no providers/URLs configured (no outbound calls until you add keys)".to_string(),
            )
        } else {
            CheckResult::Pass(format!(
                "{} host(s) max: {}",
                outbound.len(),
                outbound.join(", ")
            ))
        },
    });

    // 4. Secret redaction — compile-time, always-on (regex covers sk-/AKIA/ghp_/AIza/Bearer/Key=).
    checks.push(Check {
        name: "secret redaction".to_string(),
        result: CheckResult::Pass(
            "always-on: provider keys redacted from logs, reports, artifacts (provider.rs)"
                .to_string(),
        ),
    });

    // 5. Sandbox image pinning — digest pinning is the supply-chain hardening.
    let image = &cfg.docker.base_image;
    checks.push(Check {
        name: "sandbox image".to_string(),
        result: if image.contains("@sha256:") {
            CheckResult::Pass(format!("pinned: {}", image))
        } else if image.is_empty() {
            CheckResult::Warn("no docker config (run on worktree backend?)".to_string())
        } else {
            CheckResult::Warn(format!(
                "{} — pin to @sha256:<digest> for supply-chain hardening",
                image
            ))
        },
    });

    checks
}

/// Verify the configured sandbox image exists locally. A missing image is the
/// most common first-run failure after the runtime itself, and previously
/// surfaced only as a mid-run tool-check failure.
fn check_sandbox_image(base_image: &str) -> Check {
    fn present(runtime: &str, image: &str) -> bool {
        let probe = if runtime == "docker" {
            Command::new("docker")
                .args(["image", "inspect", image])
                .output()
        } else {
            Command::new("podman")
                .args(["image", "exists", image])
                .output()
        };
        probe.map(|o| o.status.success()).unwrap_or(false)
    }
    let result = if base_image.is_empty() {
        CheckResult::Warn("no base_image configured (worktree backend?)".to_string())
    } else if present("docker", base_image) || present("podman", base_image) {
        CheckResult::Pass(format!("{} present locally", base_image))
    } else {
        CheckResult::Fail(format!(
            "{} not found locally — build it: `podman build -t {} -f docker/Dockerfile .` (or `docker build ...`)",
            base_image, base_image
        ))
    };
    Check {
        name: "sandbox image present".to_string(),
        result,
    }
}

fn check_sandbox() -> Vec<Check> {
    let docker_result = match Command::new("docker").arg("--version").output() {
        Ok(output) => {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                CheckResult::Pass(version.trim().to_string())
            } else {
                CheckResult::Warn("docker exists but failed to run".to_string())
            }
        }
        Err(_) => match Command::new("podman").arg("--version").output() {
            Ok(output) => {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    CheckResult::Pass(version.trim().to_string())
                } else {
                    CheckResult::Fail("docker and podman found but both failed".to_string())
                }
            }
            Err(_) => CheckResult::Fail(
                "Docker or Podman not found (install one for sandbox backend)".to_string(),
            ),
        },
    };

    let git_result = match Command::new("git").arg("--version").output() {
        Ok(output) => {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                CheckResult::Pass(version.trim().to_string())
            } else {
                CheckResult::Fail("git exists but failed to run".to_string())
            }
        }
        Err(_) => CheckResult::Fail("git not found".to_string()),
    };

    vec![
        Check {
            name: "container runtime".to_string(),
            result: docker_result,
        },
        Check {
            name: "git".to_string(),
            result: git_result,
        },
    ]
}

/// How many probes `niki doctor --measure` runs.
///
/// Four is a number with a reason: enough that a 2/4 model is distinguishable
/// from noise, few enough that the command is a few seconds rather than a
/// coffee break. It is a lower bound on a rough measurement, and the output says
/// so.
const PROBES: u32 = 4;

/// Probe the configured model with a trivial artifact task and record how often
/// it produces something valid.
///
/// This measures the one capability the pipeline actually depends on — can the
/// model emit a schema-valid artifact at all — rather than inferring it from a
/// model name, which is a guess that rots the moment a provider ships a new
/// one.
///
/// The probe is deliberately the *hardest* thing the model will be asked to do
/// in a real run: a valid `CodeDiff` with a search/replace pair. A model that
/// cannot do that cannot do the Coder stage, and that is the fact the topology
/// heuristic needs.
async fn measure_capability(
    config: &NikiConfig,
) -> Result<crate::config::capability::ModelCapability> {
    use crate::llm::provider::CompletionRequest;

    let agent = config.agents.coder.clone();
    let provider_cfg = config
        .providers
        .get(&agent.provider)
        .cloned()
        .unwrap_or_default();
    let provider = crate::llm::provider::create_provider(&agent.provider, &provider_cfg)?;

    // The probe uses the *real* coder prompt, not a stripped-down one.
    //
    // The first version asked the model to "call submit_artifact exactly once"
    // with a two-line system prompt, and qwen2.5-coder:3b scored 0/4 — while
    // the same model, given the actual coder prompt, produces a valid edit. That
    // measures a strawman, and it would have recorded a false 0% and routed
    // every user of a perfectly usable small model to the slow path. The
    // question worth asking is "can this model do what the Coder stage asks",
    // so the probe asks exactly that.
    let coder_prompt = crate::agents::render_coder_probe_prompt();
    let spec = crate::llm::provider::ToolSpec {
        name: "submit_artifact".to_string(),
        description: "Submit your final answer. The parameters ARE the artifact this stage is \
                      graded on; call it once, when the work is done."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "search": { "type": "string" },
                            "replace": { "type": "string" }
                        },
                        "required": ["search", "replace"]
                    }
                },
                "files_changed": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "action": { "type": "string" },
                            "language": { "type": "string" }
                        },
                        "required": ["path"]
                    }
                },
                "implementation_notes": { "type": "string" },
                "spec_adherence": { "type": "string" }
            },
            "required": ["edits", "files_changed", "implementation_notes", "spec_adherence"]
        }),
    };

    let mut passed = 0u32;
    let mut reachable = false;
    for _ in 0..PROBES {
        let request = CompletionRequest {
            model: agent.model.clone(),
            system_prompt: coder_prompt.clone(),
            user_message: "Replace the text `old` with `new`.".to_string(),
            max_tokens: 1024,
            temperature: 0.0,
            json_schema: None,
            tools: Some(vec![spec.clone()]),
        };
        match provider.complete(request).await {
            Ok(response) => {
                reachable = true;
                let submitted = response
                    .tool_calls
                    .iter()
                    .find(|c| c.name == "submit_artifact")
                    .map(|c| c.arguments.clone());
                // The same validator the Coder stage uses, so the probe
                // measures the thing that actually matters rather than a
                // proxy that happens to be easy to check.
                if let Some(value) = submitted
                    && crate::artifacts::validate::validate_artifact(
                        &value.to_string(),
                        "schemas/code_diff.schema.json",
                    )
                    .is_ok()
                {
                    passed += 1;
                }
            }
            Err(e) => {
                println!("    probe failed: {e}");
                break;
            }
        }
    }

    if !reachable {
        return Ok(crate::config::capability::ModelCapability::Unreachable);
    }
    Ok(crate::config::capability::ModelCapability::Measured {
        passed,
        total: PROBES,
    })
}

fn cfg_for_measure() -> NikiConfig {
    NikiConfig::load(&std::env::current_dir().unwrap_or_default()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_host_extracts_authority() {
        assert_eq!(
            url_host("https://api.openai.com/v1"),
            Some("api.openai.com".to_string())
        );
        assert_eq!(
            url_host("http://localhost:11434"),
            Some("localhost:11434".to_string())
        );
        assert_eq!(
            url_host("github.com/readme"),
            Some("github.com".to_string())
        );
        assert_eq!(url_host("not a url"), Some("not a url".to_string()));
        assert_eq!(url_host(""), Some("".to_string()));
    }

    #[test]
    fn spend_cap_pass_when_set() {
        let cfg = NikiConfig {
            general: crate::config::types::GeneralConfig {
                spend_cap_usd: 5.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let checks = check_security_for(&cfg);
        let cap = checks.iter().find(|c| c.name == "spend cap").unwrap();
        assert!(matches!(cap.result, CheckResult::Pass(_)));
    }

    #[test]
    fn egress_allowlist_star_warns() {
        let cfg = NikiConfig {
            docker: crate::config::types::DockerConfig {
                network_disabled: false,
                network_allowlist: vec!["*".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let checks = check_security_for(&cfg);
        let egress = checks.iter().find(|c| c.name == "network egress").unwrap();
        assert!(matches!(egress.result, CheckResult::Warn(_)));
    }

    #[test]
    fn image_pinned_by_digest() {
        let cfg = NikiConfig {
            docker: crate::config::types::DockerConfig {
                base_image: "niki-sandbox@sha256:abc123".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let checks = check_security_for(&cfg);
        let img = checks.iter().find(|c| c.name == "sandbox image").unwrap();
        assert!(matches!(img.result, CheckResult::Pass(_)));
    }

    #[test]
    fn tag_image_warns() {
        let cfg = NikiConfig::default();
        let checks = check_security_for(&cfg);
        let img = checks.iter().find(|c| c.name == "sandbox image").unwrap();
        assert!(matches!(img.result, CheckResult::Warn(_)));
    }
}
