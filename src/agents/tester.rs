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

/// The result of running the project's real test suite inside the sandbox.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestExecution {
    /// The command that was executed.
    pub command: String,
    /// Process exit code (`0` = pass). `-1` means the command could not be run.
    pub exit_code: i64,
    /// Whether the suite passed (exit code == 0).
    pub passed: bool,
    /// Captured standard output (truncated to [`TEST_OUTPUT_LIMIT`]).
    pub stdout: String,
    /// Captured standard error (truncated to [`TEST_OUTPUT_LIMIT`]).
    pub stderr: String,
    /// Whether stdout/stderr were truncated.
    pub truncated: bool,
    /// Optional human note (e.g. why execution was skipped or failed to start).
    pub note: Option<String>,
}

/// Auto-detect a test command from the project layout when the user has not
/// configured one explicitly.
fn autodetect_test_command(project_path: &Path) -> Option<String> {
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
/// Returns `None` when no test command could be resolved (e.g. an empty repo
/// with no recognizable manifest) — the pipeline treats this as "no verification
/// was possible" rather than a failure, so a run is never blocked on it.
pub async fn run_tests(
    sandbox: &dyn Sandbox,
    config: &NikiConfig,
    project_path: &Path,
) -> Option<TestExecution> {
    let command = resolve_test_command(config, project_path)?;

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
                stdout: String::new(),
                stderr: String::new(),
                truncated: false,
                note: Some(format!("test command could not be executed: {e}")),
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
        stdout,
        stderr,
        truncated: so_trunc || se_trunc,
        note,
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
