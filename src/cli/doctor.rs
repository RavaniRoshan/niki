use anyhow::Result;
use clap::Args;
use std::process::Command;

use crate::cli::auth::{load_existing_keys, load_env_keys, PROVIDERS};

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

    println!("\nSummary: {} checks, {} passed, {} warnings, {} failed",
        filtered.len(),
        filtered.iter().filter(|c| matches!(c.result, CheckResult::Pass(_))).count(),
        warnings,
        errors);

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
                Some(p) if p.exists() => {
                    CheckResult::Pass(format!("found at {}", p.display()))
                }
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
        Err(_) => {
            match Command::new("podman").arg("--version").output() {
                Ok(output) => {
                    if output.status.success() {
                        let version = String::from_utf8_lossy(&output.stdout);
                        CheckResult::Pass(version.trim().to_string())
                    } else {
                        CheckResult::Fail("docker and podman found but both failed".to_string())
                    }
                }
                Err(_) => CheckResult::Fail("Docker or Podman not found (install one for sandbox backend)".to_string()),
            }
        }
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
