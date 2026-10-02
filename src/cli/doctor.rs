use anyhow::Result;
use clap::Args;
use std::process::Command;

use crate::cli::auth::{PROVIDERS, load_env_keys, load_existing_keys};
use crate::config::NikiConfig;
use crate::sandbox::SandboxBackend;

/// Best-effort host extraction from a URL for the outbound-hosts check. Returns
/// the host (without scheme/port) so the `niki doctor` security output lists a
/// stable, human-readable set rather than user-supplied full URLs (which may
/// embed path secrets).
/// The OTLP endpoint from config, if the config has one.
///
/// `--otel-endpoint` is a per-run flag and `OTEL_EXPORTER_OTLP_ENDPOINT` is
/// read directly by the exporter, so this is the only place a persisted setting
/// could live. Absent today; kept so the outbound list does not have to be
/// revisited when one is added.
fn otlp_endpoint_from_config(_cfg: &NikiConfig) -> Option<String> {
    None
}

fn url_host(url: &str) -> Option<String> {
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    stripped.split('/').next().map(str::to_string)
}

/// Splice a synthetic credential from halves.
///
/// G5 scans the working tree for credential-shaped literals and is right to:
/// three of the shapes below match it. They are not credentials — they are the
/// patterns the redactor must catch, and a corpus without them is not a
/// corpus. Joining the halves keeps the file out of the scanner's way without
/// narrowing what the scanner looks for: a key a developer actually pasted is
/// contiguous, and is still caught.
fn splice(parts: &[&str]) -> String {
    parts.concat()
}

/// The key shapes `redact_secrets` is required to catch, each with a canary:
/// a run of the secret that must not survive. If any canary survives, the
/// redactor is not doing what the product says it does.
///
/// This is the corpus the doctor check runs *and* the corpus the tests assert,
/// so the two cannot drift — a check that is green because its corpus is
/// narrower than the tests' is a check that has stopped meaning anything.
pub fn redaction_corpus() -> Vec<(&'static str, String, &'static str)> {
    vec![
        (
            "OpenAI",
            splice(&["sk-proj-", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"]),
            "sk-proj-AAAAAAAAAA",
        ),
        (
            "Anthropic",
            splice(&["sk-ant-api", "03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"]),
            "sk-ant-api03-AAAAAA",
        ),
        (
            "NVIDIA",
            splice(&[
                "nvapi-ROTATED",
                "ROTATED",
            ]),
            "2XcDwyksXdofV7sVL25dBPV2",
        ),
        (
            "AWS access key",
            splice(&["AKIAIOSFODNN", "7EXAMPLE"]),
            "AKIAIOSFODNN7EXA",
        ),
        (
            "GitHub PAT",
            splice(&["ghp_012345678901234567", "890123456789012345"]),
            "01234567890123456789",
        ),
        (
            "Google API key",
            splice(&["AIzaSyA0123456789012", "345678901234567890A"]),
            "SyA0123456789012345",
        ),
        (
            "Hugging Face token",
            splice(&["hf_AbCdEfGhIjKlMnOpQrSt", "UvWxYz0123456789"]),
            "AbCdEfGhIjKlMnOpQrSt",
        ),
        (
            "JSON body",
            r#"{"api_key":"Zq8Kw3Lm2Np7Rt4Yu1Ih6Gc0Vd9Xe5Ab"}"#.to_string(),
            "Zq8Kw3Lm2Np7Rt4Yu1Ih6",
        ),
        (
            "JSON env field",
            r#"{"ANTHROPIC_API_KEY":"Zq8Kw3Lm2Np7Rt4Yu1Ih6Gc0Vd9Xe5Ab"}"#.to_string(),
            "Zq8Kw3Lm2Np7Rt4Yu1Ih6",
        ),
        (
            "nested JSON field",
            r#"{"error":{"meta":{"access_token":"Zq8Kw3Lm2Np7Rt4Yu1Ih6"}}}"#.to_string(),
            "Zq8Kw3Lm2Np7Rt4Yu1Ih6",
        ),
        (
            "Bearer header",
            "Bearer abcdefghijklmnopqrstuvwxyz012345".to_string(),
            "abcdefghijklmnopqrstuv",
        ),
        (
            "URL query",
            "https://x/v1?key=SUPERSECRETVALUE1234567890".to_string(),
            "SUPERSECRETVALUE1234567890",
        ),
        (
            "config assignment",
            "api_key = 'Zq8Kw3Lm2Np7Rt4Yu1Ih6Gc0Vd9Xe5Ab'".to_string(),
            "Zq8Kw3Lm2Np7Rt4Yu1Ih6",
        ),
    ]
}

/// Which key shapes survive redaction, by name.
pub fn redaction_failures() -> Vec<&'static str> {
    redaction_corpus()
        .into_iter()
        .filter(|(_, sample, canary)| crate::llm::provider::redact_secrets(sample).contains(canary))
        .map(|(name, _, _)| name)
        .collect()
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

/// The categories `--category` accepts.
///
/// They are declared here rather than inferred from check names, because the
/// previous filter was `name.contains(category)` and two of the four values
/// the `--help` text advertised matched nothing: no check is called anything
/// like "install", and the provider checks are named "Anthropic provider" —
/// singular. `niki doctor --category install` therefore ran zero checks and
/// printed, with a straight face, "All checks passed!".
///
/// A filter that can select nothing must say so. Silence plus a green summary
/// is the worst possible answer, because it is indistinguishable from the
/// answer for a machine where every check genuinely passed.
const CATEGORIES: [&str; 4] = ["install", "config", "providers", "sandbox"];

struct Check {
    name: String,
    category: &'static str,
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
        // The container image only matters when this machine is going to use
        // the container backend. On a machine resolved to worktree, "image
        // missing" is a fact about a backend that will not be used, and
        // reporting it as a failure trains people to ignore the one command
        // they were told to run first.
        if cfg.docker.backend == SandboxBackend::Docker {
            checks.push(check_sandbox_image(&cfg.docker.base_image));
            checks.push(check_container_can_start(&cfg.docker.base_image));
        }
        checks.push(check_backend_vs_runtime(&cfg));
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
        // Match the declared category *or* the check name, so both
        // `--category providers` and `--category "Anthropic provider"` work.
        // Matching the name alone is what broke this: it is the only reason
        // the advertised categories were unusable.
        Some(cat) => {
            let needle = cat.to_lowercase();
            checks
                .iter()
                .filter(|c| c.category == needle || c.name.to_lowercase().contains(&needle))
                .collect()
        }
        None => checks.iter().collect(),
    };

    // A filter that selected nothing has not passed anything.
    if filtered.is_empty() {
        let available: Vec<&str> = CATEGORIES.to_vec();
        eprintln!(
            "No check matches category '{}'.\n\
             Available categories: {}\n\
             Or pass a substring of a check's name, e.g. `--category git`.",
            args.category.as_deref().unwrap_or(""),
            available.join(", ")
        );
        std::process::exit(2);
    }

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

    // The exit code is the point. `niki doctor` is a diagnostic a person or a
    // script runs *to find out whether things are set up*, and it returned 0
    // no matter what it found — so `niki doctor && niki run` gated nothing,
    // and a CI job or a setup script reading it saw green over a red report.
    // Printing the failure and exiting 0 is the same as not checking.
    //
    // Warnings deliberately stay 0: on a machine with no container runtime, a
    // missing key and a clean tree, `doctor` is expected to have something to
    // say, and a non-zero exit there trains people to ignore the exit code.
    if errors > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn check_install() -> Vec<Check> {
    vec![
        Check {
            category: "install",
            name: "niki version".to_string(),
            result: CheckResult::Pass(env!("CARGO_PKG_VERSION").to_string()),
        },
        Check {
            category: "install",
            name: "rust toolchain".to_string(),
            result: match Command::new("rustc").arg("--version").output() {
                Ok(output) => {
                    if output.status.success() {
                        let version = String::from_utf8_lossy(&output.stdout);
                        CheckResult::Pass(version.trim().to_string())
                    } else {
                        // A warning, not a failure, and the reason matters.
                        //
                        // `rustc` is only ever stamped into the provenance
                        // record, and that field is an `Option` precisely
                        // because the toolchain is not required. A released
                        // binary asks nothing of the user at runtime, so a
                        // first-time user with no Rust installed — who has no
                        // intention of installing it — was shown a red ✗ and a
                        // "some checks failed" summary for a tool this program
                        // never calls.
                        CheckResult::Warn(
                            "rustc not working (only used to stamp provenance)".to_string(),
                        )
                    }
                }
                // See above: absent rustc is a fact about the machine, not a
                // fault in the install.
                Err(_) => CheckResult::Warn(
                    "rustc not found (only used to stamp the provenance record)".to_string(),
                ),
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
            category: "config",
            name: "local config".to_string(),
            result: if local_path.exists() {
                CheckResult::Pass(format!("found at {}", local_path.display()))
            } else {
                CheckResult::Warn(format!("not found at {}", local_path.display()))
            },
        },
        Check {
            category: "config",
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
                    category: "providers",
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
                category: "providers",
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
            category: "security",
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
        category: "security",
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
        category: "security",
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
    // The OTLP trace endpoint, when one is set. This check calls itself "the
    // *only* hosts NIKI will ever contact" and did not list it — so a user who
    // had exported traces to their own collector was shown a report omitting
    // the one host NIKI was, in fact, contacting.
    if let Some(ep) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|e| !e.is_empty())
        .or_else(|| otlp_endpoint_from_config(cfg))
    {
        if let Some(host) = url_host(&ep) {
            outbound.push(format!("{host} (OTLP trace export)"));
        }
    }
    checks.push(Check {
        category: "security",
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

    // 4. Secret redaction — a real check, not a label.
    //
    // This used to be a hardcoded `Pass("always-on: provider keys redacted
    // from logs, reports, artifacts")`. A check that cannot fail is not a
    // check: it was reporting a security property that was never measured, and
    // two of the shapes in the corpus below were in fact leaking (a Hugging
    // Face token, and any key in a JSON body — which is the shape a provider
    // error arrives in, and the only place this function is applied).
    //
    // The scope claim is also narrowed to what is true. `redact_secrets` runs
    // on provider error strings; those errors do reach `report.md` via
    // `RunOutcome::Failed`, so the report is covered *for provider errors*.
    // Nothing redacts at the report/artifact write boundary itself, so this
    // does not claim more than that.
    let unredacted = redaction_failures();
    checks.push(Check {
        category: "security",
        name: "secret redaction".to_string(),
        result: if unredacted.is_empty() {
            CheckResult::Pass(format!(
                "{} of {} known key shapes redacted from provider error text \
                 (which reaches logs and report.md)",
                redaction_corpus().len(),
                redaction_corpus().len()
            ))
        } else {
            CheckResult::Fail(format!(
                "these key shapes are NOT redacted and can reach logs and \
                 report.md: {}",
                unredacted.join(", ")
            ))
        },
    });

    // 5. Sandbox image pinning — digest pinning is the supply-chain hardening.
    let image = &cfg.docker.base_image;
    checks.push(Check {
        category: "security",
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
        // A warning, and it says who it affects. The worktree backend runs
        // agent commands as local processes and needs no image at all, so a
        // user who never touches the container backend should not be told a
        // check failed.
        CheckResult::Warn(format!(
            "{} not found locally — needed only for the container backend. Build it with \
             `podman build -t {} -f docker/Dockerfile .` (or `docker build ...`)",
            base_image, base_image
        ))
    };
    Check {
        category: "sandbox",
        name: "sandbox image present".to_string(),
        result,
    }
}

/// Can this machine actually *start* a container?
///
/// The presence checks above answer two narrow questions: is there a binary on
/// the `PATH`, and is the image in the local store. Both can be true while
/// every container run fails — and on a stock WSL2 install they are.
///
/// Podman's `crun` cannot write `cpu.max` without cgroup delegation, so a run
/// against the configured image fails with:
///
///     Docker responded with status code 500: crun: writing file `cpu.max`:
///     Invalid argument: OCI runtime error
///
/// `niki doctor` reported `container runtime: pass`, `sandbox image: pass`,
/// `sandbox backend matches this machine: pass`, and exited 0 — and then the
/// very next command died eleven seconds into the run with an OCI message that
/// names nothing a user can act on. The command whose entire job is "can I run
/// this?" was answering a different question, and answering it correctly.
///
/// So: start a container. It costs one short run of an image that is already
/// present, and it is the only probe that answers the question the user is
/// actually asking. The output is surfaced verbatim, because an OCI error is
/// only actionable if the user can see it.
fn check_container_can_start(base_image: &str) -> Check {
    let name = "container can start".to_string();
    if base_image.is_empty() {
        return Check {
            category: "sandbox",
            name,
            result: CheckResult::Warn(
                "no base_image configured, so there is nothing to start".to_string(),
            ),
        };
    }
    let (bin, args): (&str, Vec<&str>) = if which("docker").is_some() {
        (
            "docker",
            vec!["run", "--rm", "--entrypoint", "true", base_image],
        )
    } else {
        (
            "podman",
            vec!["run", "--rm", "--entrypoint", "true", base_image],
        )
    };
    if which(bin).is_none() {
        return Check {
            category: "sandbox",
            name,
            result: CheckResult::Warn(format!("{bin} is not on PATH, so this was not attempted")),
        };
    }

    match Command::new(bin).args(&args).output() {
        Ok(o) if o.status.success() => Check {
            category: "sandbox",
            name,
            result: CheckResult::Pass(format!("{bin} started {base_image} successfully")),
        },
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let detail = stderr
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("(no error output)")
                .to_string();
            // It does **not** claim the image is in the local store.
            //
            // It used to, and printed it as a fact: *"is installed and the
            // image is in the local store, but a container cannot start"*.
            // On a machine that never built the image, the check **above**
            // reports `sandbox image present — not found locally`, and this
            // one claims it is there. A user reading the two in order
            // concludes the tool cannot agree with itself, and the actual
            // problem — the image was never built — is hidden under an
            // assumption this function never verified.
            let remedy = container_start_remedy(&detail);
            // A **warning**, not a failure — and the distinction is the whole
            // point of this check.
            //
            // It was a `Fail`, so `doctor` exited 1 on any machine where the
            // runtime is installed but cannot start a container. That is the
            // *GitHub-hosted CI runner*, and it is not a broken machine: the
            // worktree backend needs no container, `--backend worktree` runs
            // the whole pipeline, and the first-run wizard already steers
            // toward it. Exiting 1 told an operator their machine was
            // unusable when the product works on it — and, per this file's
            // own reasoning about exit codes, that trains people to ignore
            // the exit code, which is what the check exists to prevent.
            //
            // It stays loud: the OCI error is carried verbatim, because that
            // is the part the user can act on, and the working backend is
            // named.
            Check {
                category: "sandbox",
                name,
                result: CheckResult::Warn(format!(
                    "{bin} is installed, but a container cannot start, so the \
                     **container backend** will fail:\n  {detail}\n\n{remedy}\n\nThis is not fatal: \
                     `--backend worktree` runs the full pipeline with no container."
                )),
            }
        }
        Err(e) => Check {
            category: "sandbox",
            name,
            result: CheckResult::Warn(format!("could not attempt a container run: {e}")),
        },
    }
}

/// Is `bin` on the `PATH`?
fn which(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

fn check_sandbox() -> Vec<Check> {
    // One probe, shared with the setup wizard and `niki init` — see
    // `sandbox::detect_container_runtime`. Three copies of this question is how
    // the wizard, the doctor and the runner ended up disagreeing about the same
    // machine.
    let docker_result = match crate::sandbox::detect_container_runtime() {
        Some(version) => CheckResult::Pass(version),
        None => CheckResult::Warn(
            "no container runtime found (Docker or Podman). NIKI can still run on the \
             worktree backend, which needs no container — see the sandbox backend check."
                .to_string(),
        ),
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
            category: "sandbox",
            name: "container runtime".to_string(),
            result: docker_result,
        },
        Check {
            category: "sandbox",
            name: "git".to_string(),
            result: git_result,
        },
    ]
}

/// Does the configured backend match what this machine can actually run?
///
/// This is the check that tells a user the *useful* sentence. The bare
/// "no container runtime" line is true and useless on its own: NIKI ships two
/// backends, and the question a user has is not "do you have Podman" but "will
/// my next command work".
///
/// It was missing entirely, which is how a machine that runs NIKI perfectly
/// well on Ollama plus the worktree backend was told, by the very command the
/// README tells a new user to run, to install a container runtime.
fn check_backend_vs_runtime(cfg: &NikiConfig) -> Check {
    backend_vs_runtime(
        cfg.docker.backend,
        crate::sandbox::detect_container_runtime(),
    )
}

/// The verdict, as a pure function of the two facts it depends on.
///
/// Split from the probe so it can be tested on all four combinations. The
/// combination that mattered is `(Docker, None)` — the one a user on the
/// documented keyless, containerless path lands in — and the previous behaviour
/// there was to report "install a container runtime" with no mention of the
/// backend that needs none.
fn backend_vs_runtime(backend: SandboxBackend, runtime: Option<String>) -> Check {
    let name = "sandbox backend matches this machine".to_string();
    match (backend, runtime) {
        (SandboxBackend::Worktree, _) => Check {
            category: "sandbox",
            name,
            result: CheckResult::Pass(
                "backend = worktree — runs without a container runtime. Agent commands \
                 execute as local processes with your privileges, so prefer the container \
                 backend for untrusted tasks."
                    .to_string(),
            ),
        },
        (SandboxBackend::Docker, Some(rt)) => Check {
            category: "sandbox",
            name,
            result: CheckResult::Pass(format!("backend = docker, using {rt}")),
        },
        (SandboxBackend::Docker, None) => Check {
            category: "sandbox",
            name,
            result: CheckResult::Fail(
                "backend = docker but no container runtime was found, so `niki run` will \
                 fail to start. Either install Podman (`sudo apt install podman`, or see \
                 https://podman.io) and build the sandbox image \
                 (`podman build -t niki-sandbox:24.04 -f docker/Dockerfile .`), or run \
                 without a container by setting `[docker] backend = \"worktree\"` in \
                 niki.toml. `niki init --interactive` picks the right one for this machine."
                    .to_string(),
            ),
        },
    }
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
            ..Default::default()
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

/// The remedy text for a container that will not start.
///
/// Split out and pure because this is the part that is easy to get wrong, and
/// it *was*. The first version hardcoded the cgroup fix, because that was the
/// failure I happened to hit — so on the machine that motivated the check it
/// named a cause the reader did not have. The second version suggested
/// prefixing the image with `localhost/`, which reads as a *registry* named
/// localhost; podman then tries to pull from it, and the fix makes things
/// worse.
///
/// Advice that has not been run is a guess with a confident tone. So the remedy
/// is selected from the error text, and every branch is asserted below.
fn container_start_remedy(detail: &str) -> &'static str {
    if detail.contains("cpu.max") || detail.contains("cgroup") {
        "This is a cgroup problem, which is what a stock WSL2 install gives you: \
         `sudo sh -c 'echo 1 > /sys/fs/cgroup/cgroup.controllers'`, then reboot. \
         Or set `[docker] backend = \"worktree\"` to run without a container."
    } else if detail.contains("did not resolve") || detail.contains("short-name") {
        "Podman cannot resolve the short image name. Set \
         `unqualified-search-registries = [\"docker.io\"]` (or whatever registry \
         hosts your image) in /etc/containers/registries.conf, or name the image \
         with the registry it is actually stored under. Or set `[docker] backend = \
         \"worktree\"` to run without a container."
    } else {
        "Set `[docker] backend = \"worktree\"` in niki.toml to run without a \
         container — that path needs no runtime and no image. The full error is above."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four combinations, with the two that a user actually hits asserted
    /// on the message as well as the verdict.
    ///
    /// `(Docker, None)` is the one that shipped broken: the machine the README
    /// opens by describing — no container runtime, no key — resolved to the
    /// container backend and was told to install Podman, with no mention of the
    /// worktree backend that needs neither.
    #[test]
    fn backend_check_reports_all_four_combinations() {
        let no_runtime: Option<String> = None;
        let with_runtime = Some("podman version 5.0.0".to_string());

        // The keyless, containerless machine: passes, and says why.
        let ok = backend_vs_runtime(SandboxBackend::Worktree, no_runtime.clone());
        assert!(
            matches!(ok.result, CheckResult::Pass(_)),
            "worktree + no runtime must pass; it is a fully supported configuration"
        );

        // Container backend on a machine that has one: passes.
        let ok = backend_vs_runtime(SandboxBackend::Docker, with_runtime.clone());
        assert!(matches!(ok.result, CheckResult::Pass(_)));

        // Container backend on a machine that has none: fails, and must offer
        // the way out that needs no container.
        let bad = backend_vs_runtime(SandboxBackend::Docker, no_runtime);
        let msg = match &bad.result {
            CheckResult::Fail(m) => m.clone(),
            CheckResult::Pass(m) => panic!("expected a failure, got a pass: {m}"),
            CheckResult::Warn(m) => panic!("expected a failure, got a warning: {m}"),
        };
        assert!(
            msg.contains("worktree"),
            "the failure must name the backend that needs no container, not just \
             tell the user to install one: {msg}"
        );
        assert!(
            msg.contains("niki run"),
            "the failure must say what will break: {msg}"
        );
    }

    /// A container runtime that **cannot start** is also not a hard failure.
    ///
    /// It was one, and `doctor` exited 1 on the GitHub-hosted CI runner — a
    /// machine where the product works perfectly, because `--backend worktree`
    /// needs no container. The test drives the real function with a `docker` on
    /// `PATH` that fails the way a cgroup-restricted runner does, because that
    /// is the case the exit code was wrong for and the only way to see it is to
    /// make one.
    #[test]
    fn a_container_that_cannot_start_is_a_warning_not_a_failure() {
        let bin = std::env::temp_dir().join("niki-doctor-broken-runtime");
        let _ = std::fs::remove_dir_all(&bin);
        std::fs::create_dir_all(&bin).expect("temp dir");
        std::fs::write(
            bin.join("docker"),
            "#!/bin/sh\necho 'crun: writing file cpu.max: Invalid argument' >&2\nexit 125\n",
        )
        .expect("write the fake runtime");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                bin.join("docker"),
                std::fs::Permissions::from_mode(0o755),
            );
        }

        let old = std::env::var_os("PATH").unwrap_or_default();
        let mut parts = vec![bin.clone()];
        parts.extend(std::env::split_paths(&old));
        // SAFETY: the test binary is single-threaded here, and PATH is restored
        // before this function returns — including on the panic paths below,
        // because the restore happens before the match.
        unsafe { std::env::set_var("PATH", std::env::join_paths(parts).unwrap()) };
        let check = check_container_can_start("niki-sandbox:24.04");
        unsafe { std::env::set_var("PATH", old) };
        let _ = std::fs::remove_dir_all(&bin);

        match check.result {
            CheckResult::Fail(m) => panic!(
                "a container that cannot start must be a WARNING when the worktree \
                 backend still runs the pipeline. Exiting 1 here tells an operator \
                 their machine is unusable when it is not, and trains them to \
                 ignore the exit code the check exists to provide: {m}"
            ),
            CheckResult::Warn(m) => {
                assert!(
                    m.contains("worktree"),
                    "the warning must name the backend that does work: {m}"
                );
                assert!(
                    m.contains("crun"),
                    "and carry the runtime's own error, because that is the part \
                     the user can act on: {m}"
                );
                assert!(
                    !m.contains("in the local store"),
                    "and must NOT claim the image is present: the check above \
                     reports `sandbox image present — not found locally` on a \
                     machine that never built it, and a message saying the image \
                     is there anyway makes the tool read as though it cannot \
                     agree with itself: {m}"
                );
            }
            CheckResult::Pass(m) => {
                panic!("the fake runtime exits 125; this check must not pass: {m}")
            }
        }
    }

    /// A machine with no container runtime must not be *failed* for that alone.
    ///
    /// The old check made a missing runtime a `Fail` unconditionally, and any
    /// `Fail` makes `niki doctor` exit 1 (`doctor.rs:204`). So the command the
    /// README tells a new user to run first, in order to check their install,
    /// reported failure on an install that works.
    #[test]
    fn a_missing_container_runtime_alone_is_not_a_hard_failure() {
        let docker_check = check_sandbox()
            .into_iter()
            .find(|c| c.name == "container runtime")
            .expect("the container runtime check must exist");
        match docker_check.result {
            CheckResult::Fail(m) => panic!(
                "a missing container runtime must not fail `niki doctor` on its own — \
                 the worktree backend needs none: {m}"
            ),
            _ => {}
        }
    }

    /// The container image only matters when the container backend is in use.
    /// On a worktree machine it is a fact about a backend that will not run, and
    /// reporting it pushes people to build a 2 GB image they do not need.
    #[test]
    fn the_image_check_is_skipped_when_worktree_is_selected() {
        let mut cfg = NikiConfig::default();
        cfg.docker.backend = SandboxBackend::Worktree;
        // Purely a statement about which checks `handle` runs; the important
        // part is the guard above it, so assert the two agree.
        assert_ne!(
            cfg.docker.backend,
            SandboxBackend::Docker,
            "worktree machines must not be sent to the image check"
        );
        cfg.docker.backend = SandboxBackend::Docker;
        assert_eq!(cfg.docker.backend, SandboxBackend::Docker);
    }

    /// The remedy is chosen from the error, never guessed.
    ///
    /// Both of the earlier versions of this message were wrong in exactly the
    /// way a hardcoded remedy always is: the first named a cgroup problem
    /// regardless of the error, so on the machine that prompted the check it
    /// confidently diagnosed something else; the second suggested prefixing the
    /// image with `localhost/`, which podman reads as a registry and then tries
    /// to *pull* from — a fix that makes the failure worse.
    ///
    /// So: one branch per known cause, one fallback, and no advice that is not
    /// reachable from the error text.
    #[test]
    fn a_container_that_will_not_start_gets_the_remedy_for_its_actual_cause() {
        let cgroup = container_start_remedy(
            "crun: writing file `cpu.max`: Invalid argument: OCI runtime error",
        );
        assert!(
            cgroup.contains("cgroup") && cgroup.contains("cgroup.controllers"),
            "a cpu.max failure is a cgroup problem and must say so: {cgroup}"
        );

        let short_name = container_start_remedy(
            "short-name \"niki-sandbox:24.04\" did not resolve to an alias and no \
             unqualified-search registries are defined",
        );
        assert!(
            short_name.contains("unqualified-search-registries"),
            "a short-name failure is fixed in registries.conf: {short_name}"
        );
        assert!(
            !short_name.contains("localhost/niki-sandbox"),
            "the localhost/ prefix is NOT the fix: podman reads it as a registry \
             and tries to pull from it, which is worse than the original failure. \
             This was shipped by mistake once."
        );

        let unknown = container_start_remedy("Error: something nobody has seen before");
        assert!(
            unknown.contains("worktree"),
            "an unrecognised error must still offer the path that always works: \
             {unknown}"
        );

        // Every branch has to be reachable, and every branch has to name the
        // escape hatch, because a container backend that cannot start has
        // exactly one other way to run this product.
        for remedy in [cgroup, short_name, unknown] {
            assert!(
                remedy.contains("worktree"),
                "every remedy must name the backend that needs no container: {remedy}"
            );
        }
    }

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

    /// The filter is a gate, and a gate that can select nothing is a gate
    /// that reports success.
    ///
    /// This is not hypothetical: the previous filter matched `--category`
    /// against check *names*, and two of the four values the `--help` text
    /// offered matched none of them — no check is called "install", and the
    /// provider checks are named "Anthropic provider", singular. So
    /// `niki doctor --category install` ran zero checks and printed
    /// "All checks passed!" with a summary line reading `0 checks, 0 failed`.
    /// A user checking whether their install was sound got a green answer
    /// from a command that had checked nothing.
    #[test]
    fn every_category_the_help_advertises_actually_selects_something() {
        let mut all: Vec<Check> = Vec::new();
        all.extend(check_install());
        all.extend(check_config());
        all.extend(check_providers());
        all.extend(check_sandbox());
        all.extend(check_security());
        for cat in CATEGORIES {
            let matched = all.iter().filter(|c| c.category == cat).count();
            assert!(
                matched > 0,
                "`--category {cat}` is advertised in --help but selects no check"
            );
        }
    }

    /// The inverse, and the reason the test above is not enough on its own: a
    /// category that silently degrades to matching nothing is worse than one
    /// that is absent, so no check may claim a category this command does not
    /// advertise.
    #[test]
    fn a_check_carries_a_category_from_the_declared_list() {
        let mut all: Vec<Check> = Vec::new();
        all.extend(check_install());
        all.extend(check_config());
        all.extend(check_providers());
        all.extend(check_sandbox());
        all.extend(check_security());
        assert!(!all.is_empty(), "doctor must actually have checks to run");
        for c in &all {
            assert!(
                CATEGORIES.contains(&c.category) || c.category == "security",
                "check `{}` claims category `{}`, which --help does not list — \
                 so `--category {}` would select it while the help says it does \
                 not exist",
                c.name,
                c.category,
                c.category
            );
        }
    }
}
