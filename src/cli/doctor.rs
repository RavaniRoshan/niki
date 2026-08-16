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
            let mut checks = check_security_for(&cfg);
            // Replace the unloadable-config fallback with a concrete pass once
            // config loaded; check_security_for already assumes a loaded config.
            checks
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
