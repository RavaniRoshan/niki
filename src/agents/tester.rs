//! The Tester role's verification step.
//!
//! Beyond the LLM's analytical `TestReport`, NIKI *actually executes* the
//! project's test suite inside the sandbox and records the real exit code and
//! output as part of every run's audit trail — this is the "verified before you
//! see it" guarantee, not just a model's opinion.

use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;
use crate::sandbox::{ExecOutput, Sandbox};
use std::path::Path;

/// Maximum characters of stdout/stderr we retain in the artifact, to keep the
/// audit trail readable and bounded.
const TEST_OUTPUT_LIMIT: usize = 24_000;

/// What the verifier actually did, and what it found.
///
/// `passed` alone cannot carry this. It is `false` both for a suite that ran and
/// failed and for a project where *nothing ran at all* — no `Cargo.toml`, no
/// `package.json`, no configured `test_command`. A consumer reading only
/// `passed` therefore cannot tell "verified, and broken" from "never verified",
/// and the second is the one that must never be reported as a pass.
///
/// This is the tri-state the pipeline actually has, stated once so the report,
/// the JSON envelope and the branch gate all read the same field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    /// Nothing ran: no test command could be resolved for this project. Not a
    /// failure — an absence of evidence, and recorded as one.
    #[default]
    Unverified,
    /// A command ran and exited `0`.
    Passed,
    /// A command ran and exited non-zero.
    Failed,
    /// A command was resolved but could not be executed at all (no `sh`, the
    /// sandbox refused it, the permission policy denied it).
    Errored,
}

impl VerificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Errored => "errored",
        }
    }

    /// Whether any evidence exists at all. A `false` here means the run said
    /// nothing about the project, which is not the same as saying it was broken.
    pub fn is_verified(&self) -> bool {
        !matches!(self, Self::Unverified)
    }

    /// Whether this outcome must stop delivery of a branch.
    ///
    /// `Unverified` deliberately does not: a repository with no recognisable
    /// manifest is not a repository with failing tests, and blocking every run
    /// in one would be the same shape of outage as the deny-list substring bug
    /// this module's neighbours once shipped.
    pub fn blocks_delivery(&self) -> bool {
        matches!(self, Self::Failed | Self::Errored)
    }
}

/// The result of running the project's real test suite inside the sandbox.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TestExecution {
    /// The command that was executed. Empty when `status` is `Unverified` —
    /// there was no command to run.
    pub command: String,
    /// Process exit code (`0` = pass). `-1` means the command could not be run,
    /// or was never resolved.
    pub exit_code: i64,
    /// Whether the suite passed (exit code == 0). `false` when
    /// `status` is [`VerificationStatus::Unverified`]; read `status` to tell the
    /// two apart.
    pub passed: bool,
    /// What the verifier did, and what it found. `Unverified` is the default so
    /// an artifact written before this field existed reads as "no evidence"
    /// rather than as a silent pass.
    #[serde(default)]
    pub status: VerificationStatus,
    /// Captured standard output (truncated to [`TEST_OUTPUT_LIMIT`]).
    pub stdout: String,
    /// Captured standard error (truncated to [`TEST_OUTPUT_LIMIT`]).
    pub stderr: String,
    /// Whether stdout/stderr were truncated.
    pub truncated: bool,
    /// Optional human note (e.g. why execution was skipped or failed to start).
    pub note: Option<String>,
    /// Mutation-testing result, attached when `[agents.tester] mutation_command`
    /// is configured. Boxed: the audit trail nests one execution inside another.
    #[serde(default)]
    pub mutation: Option<Box<TestExecution>>,
}

/// The manifests [`autodetect_test_command`] recognises, in the order it looks
/// for them. Named so the "unverified" note can tell the user exactly what to
/// add instead of only saying nothing was found.
const AUTODETECTED_MANIFESTS: [&str; 7] = [
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "requirements.txt",
    "go.mod",
    "Gemfile",
];

/// Auto-detect a test command from the project layout when the user has not
/// configured one explicitly.
pub(crate) fn autodetect_test_command(project_path: &Path) -> Option<String> {
    let has = |name: &str| project_path.join(name).exists();
    if has("Cargo.toml") {
        Some("cargo test --locked 2>&1".to_string())
    } else if has("package.json") {
        Some("npm test 2>&1".to_string())
    } else if has("pyproject.toml") || has("setup.py") || has("requirements.txt") {
        Some("pytest 2>&1".to_string())
    } else if has("go.mod") {
        Some("go test ./... 2>&1".to_string())
    } else if has("Gemfile") {
        Some("bundle exec rspec 2>&1".to_string())
    } else {
        None
    }
}

fn resolve_test_command(config: &NikiConfig, project_path: &Path) -> Option<String> {
    match &config.agents.tester.test_command {
        Some(cmd) if !cmd.trim().is_empty() => Some(cmd.trim().to_string()),
        _ => autodetect_test_command(project_path),
    }
}

fn truncate(s: &str) -> (String, bool) {
    if s.len() <= TEST_OUTPUT_LIMIT {
        (s.to_string(), false)
    } else {
        (s.chars().take(TEST_OUTPUT_LIMIT).collect(), true)
    }
}

/// Run the project's test suite inside the sandbox and return the real result.
///
/// Always returns a record, so the caller never has to infer an outcome from
/// `Option::None`. It used to return `None` when no test command could be
/// resolved, and every consumer then had to decide what an absent record means —
/// which is how "no manifest anywhere in the project" became indistinguishable
/// from "the suite ran and failed". That case is now
/// [`VerificationStatus::Unverified`], stated in the artifact.
///
/// Nothing about the pipeline's behaviour changes on that path: an unverified
/// project still produces a branch and still reports no pass (see
/// [`VerificationStatus::blocks_delivery`]), because there is no failure to act
/// on — only the absence of one.
pub async fn run_tests(
    sandbox: &dyn Sandbox,
    config: &NikiConfig,
    project_path: &Path,
) -> Option<TestExecution> {
    let Some(command) = resolve_test_command(config, project_path) else {
        return Some(TestExecution {
            exit_code: -1,
            passed: false,
            status: VerificationStatus::Unverified,
            note: Some(format!(
                "no test command could be resolved for this project: set \
                 `[agents.tester] test_command`, or add a manifest this build recognises \
                 ({})",
                AUTODETECTED_MANIFESTS.join(", ")
            )),
            ..Default::default()
        });
    };

    let out: ExecOutput = match sandbox
        .exec(&["sh", "-lc", &command], Some(&AgentRole::Tester))
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return Some(TestExecution {
                command,
                exit_code: -1,
                passed: false,
                status: VerificationStatus::Errored,
                note: Some(format!("test command could not be executed: {e}")),
                ..Default::default()
            });
        }
    };

    let (stdout, so_trunc) = truncate(&out.stdout);
    let (stderr, se_trunc) = truncate(&out.stderr);
    let passed = out.exit_code == 0;
    let note = if !passed && out.exit_code != -1 {
        Some("test suite reported failures (non-zero exit)".to_string())
    } else {
        None
    };

    Some(TestExecution {
        command,
        exit_code: out.exit_code,
        passed,
        status: if passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        stdout,
        stderr,
        truncated: so_trunc || se_trunc,
        note,
        ..Default::default()
    })
}

/// Run the configured mutation-testing command inside the sandbox, if any.
///
/// There is deliberately no auto-detection: mutation runners differ per
/// ecosystem (`cargo mutants`, `mutmut run`, `stryker run`, …) and each has
/// its own exit-code contract, so this only runs when
/// `[agents.tester] mutation_command` is set explicitly. A non-zero exit
/// (surviving mutants) fails exactly like a failing suite: it is recorded in
/// the audit trail and blocks the branch unless `--force` is passed.
pub async fn run_mutation(
    sandbox: &dyn Sandbox,
    config: &NikiConfig,
    _project_path: &Path,
) -> Option<TestExecution> {
    let command = config.agents.tester.mutation_command.clone()?;

    let out: ExecOutput = match sandbox
        .exec(&["sh", "-lc", &command], Some(&AgentRole::Tester))
        .await
    {
        Ok(o) => o,
        Err(e) => {
            return Some(TestExecution {
                command,
                exit_code: -1,
                passed: false,
                status: VerificationStatus::Errored,
                note: Some(format!("mutation command could not be executed: {e}")),
                ..Default::default()
            });
        }
    };

    let (stdout, so_trunc) = truncate(&out.stdout);
    let (stderr, se_trunc) = truncate(&out.stderr);
    let passed = out.exit_code == 0;
    let note = if !passed && out.exit_code != -1 {
        Some("mutation testing reported surviving mutants (non-zero exit)".to_string())
    } else {
        None
    };

    Some(TestExecution {
        command,
        exit_code: out.exit_code,
        passed,
        status: if passed {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        stdout,
        stderr,
        truncated: so_trunc || se_trunc,
        note,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn autodetect_prefers_explicit_command() {
        let mut cfg = crate::config::NikiConfig::default();
        cfg.agents.tester.test_command = Some("make check".to_string());
        // Even inside a Rust-looking dir, the explicit command wins.
        let dir = PathBuf::from("/tmp/does-not-exist-xyz");
        assert_eq!(
            resolve_test_command(&cfg, &dir),
            Some("make check".to_string())
        );
    }

    #[test]
    fn autodetect_rust_project() {
        let tmp = std::env::temp_dir().join(format!("niki-test-detect-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        let cfg = crate::config::NikiConfig::default();
        assert_eq!(
            resolve_test_command(&cfg, &tmp),
            Some("cargo test --locked 2>&1".to_string())
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn truncate_keeps_short_output() {
        let (s, t) = truncate("hello");
        assert_eq!(s, "hello");
        assert!(!t);
    }

    #[test]
    fn truncate_caps_long_output() {
        let big = "x".repeat(TEST_OUTPUT_LIMIT + 100);
        let (s, t) = truncate(&big);
        assert!(t);
        assert_eq!(s.chars().count(), TEST_OUTPUT_LIMIT);
    }
}
