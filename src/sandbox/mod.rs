use anyhow::{Result, anyhow};
use async_trait::async_trait;
use bollard::Docker;
use std::path::Path;
use uuid::Uuid;

use crate::NikiError;
use crate::artifacts::types::AgentRole;
use crate::config::{DockerConfig, NikiConfig, SecurityPolicyConfig};
use crate::permissions::{Permission, PermissionChecker, PermissionConfig, PermissionRule};

/// Map a config permission string ("allow"/"ask"/"deny") to the `Permission` enum.
fn parse_permission(s: &str) -> Permission {
    match s.to_lowercase().as_str() {
        "allow" => Permission::Allow,
        "deny" => Permission::Deny,
        _ => Permission::Ask,
    }
}

/// Build a [`PermissionChecker`] from a security policy so the dead-island
/// granular permission system actually gates command execution. Denied
/// commands become `Deny` rules; everything else falls through to `Ask`
/// (which the headless sandbox treats as allow — interactive prompting is a
/// TUI concern). With an empty deny list this is a no-op (behavior-preserving).
///
/// `[permissions]` config rules are merged on top of the deny-list rules, and
/// `auto_approve` is taken from config instead of being hardcoded.
pub(crate) fn build_permission_checker(
    policy: &SecurityPolicyConfig,
    config: &NikiConfig,
) -> PermissionChecker {
    let mut rules: std::collections::HashMap<String, PermissionRule> =
        std::collections::HashMap::new();
    for denied in &policy.denied_commands {
        rules.insert(
            format!("deny:{}", denied),
            PermissionRule {
                permission: Permission::Deny,
                pattern: Some(denied.clone()),
            },
        );
    }
    // Merge [permissions] rules from config.
    for (i, rc) in config.permissions.rules.iter().enumerate() {
        let key = if rc.action.is_empty() {
            format!("rule_{}", i)
        } else {
            rc.action.clone()
        };
        rules.insert(
            key,
            PermissionRule {
                permission: parse_permission(&rc.permission),
                pattern: rc.pattern.clone(),
            },
        );
    }
    PermissionChecker::new(PermissionConfig {
        tools: crate::permissions::ToolPermissions::default(),
        rules,
        auto_approve: config.permissions.auto_approve,
        external_directory: Permission::Ask,
        doom_loop: Permission::Ask,
        mode: parse_permission_mode(&config.permissions.mode),
        fail_closed_headless: config.permissions.fail_closed_headless,
        ..Default::default()
    })
}

/// Map the `[permissions] mode` string (or `--permission-mode` override) onto
/// [`PermissionMode`]. Unknown values fail closed to `Manual` with a warning —
/// a typo must never silently escalate to a permissive mode.
fn parse_permission_mode(s: &str) -> crate::permissions::PermissionMode {
    use crate::permissions::PermissionMode;
    match s.to_lowercase().as_str() {
        "auto" => PermissionMode::Auto,
        "dontask" | "dont_ask" | "dont-ask" => PermissionMode::DontAsk,
        "bypass" | "bypasspermissions" | "bypass_permissions" => PermissionMode::BypassPermissions,
        "manual" => PermissionMode::Manual,
        other => {
            tracing::warn!(
                target: "niki::permissions",
                "unknown permission mode '{other}' — falling back to manual (fail closed)"
            );
            PermissionMode::Manual
        }
    }
}

pub mod docker;
pub mod edit_format;
pub mod exec;
pub mod worktree;

pub use docker::{ActiveContainers, DockerSandbox, ExecOutput};

/// Which sandbox implementation backs agent execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SandboxBackend {
    /// Containerized isolation via the pre-baked `niki-sandbox` image (default).
    #[default]
    Docker,
    /// Lightweight `git worktree` + local process isolation — no Docker required.
    Worktree,
}

/// Abstraction over an isolated execution environment for one agent stage.
///
/// `DockerSandbox` (container) and `WorktreeSandbox` (git worktree + local
/// process) implement this. The orchestrator talks only to the trait, so the
/// backends are interchangeable — this is what makes alternative sandboxing (#8)
/// a drop-in change.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Fail fast if any required tool binary is missing from the sandbox.
    async fn ensure_tools(&self, tools: &[String]) -> Result<()>;
    /// Apply a unified diff to the sandbox's working copy.
    async fn apply_patch(&self, patch: &str, host_workspace: &Path) -> Result<()>;
    /// Return the working-tree diff produced inside the sandbox, scoped to
    /// `agent_files` (Phase 5.1: pre-existing dirt is never included).
    async fn get_diff(&self, agent_files: &[String]) -> Result<String>;
    /// Run a command inside the sandbox, returning its exit code + output.
    ///
    /// When `role` is provided and a security policy exists for that role, the
    /// command is checked against the deny-list before execution. Denied
    /// commands are rejected with a clear error message.
    async fn exec(&self, cmd: &[&str], role: Option<&AgentRole>) -> Result<ExecOutput>;
    /// The directory an agent's edits actually land in, when it is not the
    /// project directory.
    ///
    /// The Coder has to be shown the *current* contents of the files it is about
    /// to edit, and on the worktree backend those live in the worktree, not in
    /// the project. Nothing bridged the two: every Coder invocation read the
    /// project, so a revision round was shown the file as it looked *before* the
    /// previous round's patch and asked to fix a review of code it could not
    /// see. Round 0 applied by coincidence — worktree and project were
    /// identical — and every round after it targeted text that no longer
    /// existed.
    fn work_root(&self) -> Option<&std::path::Path> {
        None
    }

    /// Tear the sandbox down (remove containers / worktrees).
    async fn destroy(&self) -> Result<()>;
}

/// Whether a deny-list entry is a *pipeline* — a `producer | consumer` string
/// like `curl | sh`.
///
/// Only these are eligible for a substring match anywhere in the command. Every
/// other entry is a command name or a flag, and matching it as a substring
/// matches ordinary text by accident: `dd` denies `git add`, `mkfs` denies any
/// path containing those letters, `--no-verify` denies a commit message that
/// merely mentions it.
///
/// The shape that matters is the pipe with whitespace around it, which is why
/// `"curl | sh"` matches `sh -c "curl | sh"` but `"curl |sh"` would not — the
/// user wrote the separator that way, so the policy does too.
pub fn is_pipeline_pattern(pattern: &str) -> bool {
    pattern.split_whitespace().any(|tok| tok == "|") || pattern.contains(" | ")
}

/// Check whether `cmd` is allowed by `policy`. Returns `Ok(())` if allowed,
/// or `Err` with a descriptive message if denied.
///
/// Phase 5.3: deny ALWAYS wins on overlap, independent of evaluation order.
/// The allow-list is a convenience fast path, never an escalation: an
/// allow-listed prefix that matches a denied pattern is still rejected.
/// Permissive bypasses (DontAsk/BypassPermissions) come only from the
/// explicit `[permissions] mode`, never from overlapping allow entries.
pub fn check_command_policy(cmd: &[&str], policy: &SecurityPolicyConfig) -> Result<()> {
    let full_cmd = cmd.join(" ");

    // Deny first: the global deny-list is *always* enforced for every role,
    // in addition to any per-role denies. (Previously the per-role policies
    // overrode it, which let the coder/reviewer roles run dangerous commands
    // like `curl | sh`, `mkfs`, `dd`, or `rm -rf`.) See research report S1.
    let mut denied: Vec<String> = policy.denied_commands.clone();
    denied.extend(crate::config::default_global_deny_list());

    // Check deny-list using two strategies:
    // 1. Prefix match on the full joined command (catches "git push --force origin main")
    // 2. Individual argument match (catches "git commit --no-verify")
    // 3. Substring match, but only for patterns that can only be a *pipeline*
    //    (see `is_pipeline_pattern`). Unconditional substring matching is what
    //    this used to do, and it was catastrophic: the two-letter entry "dd"
    //    matched any command containing the letters d-d, so `git add`,
    //    `cargo add`, `printf 'adding'` and every path with "add" in it were
    //    denied by the security policy. A deny-list that fires on the letters
    //    of ordinary commands is not a security control, it is an outage — and
    //    because the built-in coder policy explicitly allows `git add`, the two
    //    halves of this function contradicted each other.
    for denied in &denied {
        if full_cmd.starts_with(denied) {
            return Err(anyhow!(
                "Command denied by security policy: '{}' matches denied pattern '{}'",
                full_cmd,
                denied
            ));
        }
        // Check if any individual argument exactly matches the denied pattern
        // (e.g. "--no-verify" as a standalone argument).
        if cmd.iter().any(|arg| arg == denied) {
            return Err(anyhow!(
                "Command denied by security policy: '{}' contains denied argument '{}'",
                full_cmd,
                denied
            ));
        }
        // Substring match, restricted to pipeline patterns.
        if is_pipeline_pattern(denied) && full_cmd.contains(denied) {
            return Err(anyhow!(
                "Command denied by security policy: '{}' contains denied pattern '{}'",
                full_cmd,
                denied
            ));
        }
    }

    // Allow-list fast path (no bypass power: deny already ran above).
    for allowed in &policy.allowed_commands {
        if full_cmd.starts_with(allowed) {
            return Ok(());
        }
    }

    Ok(())
}

/// Truncate long command stdout/stderr to preserve the head (first 15 lines)
/// and tail (last 40 lines), inserting an omission summary banner in the middle.
/// If `text` is within `max_lines` and `max_bytes`, it is returned unchanged.
pub fn truncate_head_tail(text: &str, max_lines: usize, max_bytes: usize) -> String {
    if text.is_empty() || (text.len() <= max_bytes && text.lines().count() <= max_lines) {
        return text.to_string();
    }

    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        if text.len() > max_bytes {
            let mut s = text[..max_bytes.saturating_sub(40)].to_string();
            s.push_str("\n… [output truncated due to byte limit]");
            return s;
        }
        return text.to_string();
    }

    let head_count = 15.min(lines.len());
    let tail_count = 40.min(lines.len().saturating_sub(head_count));
    let omitted = lines.len().saturating_sub(head_count + tail_count);

    let mut result = Vec::with_capacity(head_count + tail_count + 1);
    result.extend_from_slice(&lines[..head_count]);
    if omitted > 0 {
        let banner = format!("… [{} lines omitted]", omitted);
        result.push(&banner);
        result.extend_from_slice(&lines[lines.len() - tail_count..]);
        return result.join("\n");
    }
    result.extend_from_slice(&lines[lines.len() - tail_count..]);
    result.join("\n")
}

/// Create the sandbox for `backend`. `docker` is only required for the Docker
/// backend (pass `None` for worktree).
///
/// `policy` is the security policy for this sandbox's agent role; commands
/// executed via `exec` are checked against it when a role is supplied.
pub async fn create_sandbox(
    backend: SandboxBackend,
    docker: Option<&Docker>,
    agent_role: AgentRole,
    source_repo: &Path,
    task_id: &Uuid,
    config: &DockerConfig,
    niki_config: &NikiConfig,
    policy: SecurityPolicyConfig,
    containers: ActiveContainers,
    event_tx: std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>,
) -> Result<Box<dyn Sandbox>> {
    match backend {
        SandboxBackend::Docker => {
            let d = docker.ok_or_else(|| {
                NikiError::Config("Docker backend selected but Docker is not available".into())
            })?;
            Ok(Box::new(
                DockerSandbox::create(
                    d,
                    agent_role,
                    source_repo,
                    task_id,
                    config,
                    niki_config,
                    policy,
                    containers,
                    event_tx,
                )
                .await?,
            ))
        }
        SandboxBackend::Worktree => Ok(Box::new(
            worktree::WorktreeSandbox::create(
                agent_role,
                source_repo,
                task_id,
                config,
                niki_config,
                policy,
                event_tx,
            )
            .await?,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SecurityPolicyConfig;

    fn test_policy() -> SecurityPolicyConfig {
        SecurityPolicyConfig {
            allowed_commands: vec!["cargo test".into(), "git diff".into()],
            denied_commands: vec![
                "git push --force".into(),
                "rm -rf /".into(),
                "mkfs".into(),
                "dd".into(),
                "curl | sh".into(),
                "--no-verify".into(),
            ],
            max_exec_seconds: 300,
        }
    }

    #[test]
    fn allowed_command_passes() {
        let policy = test_policy();
        assert!(check_command_policy(&["cargo", "test", "--lib"], &policy).is_ok());
        assert!(check_command_policy(&["git", "diff"], &policy).is_ok());
    }

    #[test]
    fn denied_command_rejected() {
        let policy = test_policy();
        let err = check_command_policy(&["git", "push", "--force", "origin", "main"], &policy);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("denied"));
    }

    #[test]
    fn deny_rm_rf_root() {
        let policy = test_policy();
        let err = check_command_policy(&["rm", "-rf", "/"], &policy);
        assert!(err.is_err());
    }

    #[test]
    fn deny_mkfs() {
        let policy = test_policy();
        let err = check_command_policy(&["mkfs", "/dev/sda"], &policy);
        assert!(err.is_err());
    }

    #[test]
    fn deny_dd() {
        let policy = test_policy();
        let err = check_command_policy(&["dd", "if=/dev/zero", "of=/dev/sda"], &policy);
        assert!(err.is_err());
    }

    #[test]
    fn deny_curl_pipe_sh() {
        let policy = test_policy();
        let err = check_command_policy(&["sh", "-c", "curl | sh"], &policy);
        assert!(err.is_err());
    }

    #[test]
    fn deny_no_verify() {
        let policy = test_policy();
        let err = check_command_policy(&["git", "commit", "--no-verify"], &policy);
        assert!(err.is_err());
    }

    #[test]
    fn unknown_command_allowed_when_not_denied() {
        let policy = test_policy();
        // "ls" is not in allowed_commands or denied_commands — should pass
        assert!(check_command_policy(&["ls", "-la"], &policy).is_ok());
    }

    #[test]
    fn tester_policy_blocks_git_push() {
        let policy = crate::config::types::default_tester_policy();
        assert!(check_command_policy(&["git", "push", "origin", "main"], &policy).is_err());
    }

    #[test]
    fn tester_policy_allows_cargo_test() {
        let policy = crate::config::types::default_tester_policy();
        assert!(check_command_policy(&["cargo", "test", "--lib"], &policy).is_ok());
    }

    #[test]
    fn coder_policy_allows_git_commit() {
        let policy = crate::config::types::default_coder_policy();
        assert!(check_command_policy(&["git", "commit", "-m", "fix"], &policy).is_ok());
    }

    #[test]
    fn coder_policy_blocks_git_push() {
        let policy = crate::config::types::default_coder_policy();
        assert!(check_command_policy(&["git", "push"], &policy).is_err());
    }

    #[test]
    fn reviewer_policy_blocks_git_commit() {
        let policy = crate::config::types::default_reviewer_policy();
        assert!(check_command_policy(&["git", "commit", "-m", "fix"], &policy).is_err());
    }

    #[test]
    fn permission_mode_mapping_fails_closed() {
        use crate::permissions::PermissionMode;
        assert_eq!(parse_permission_mode("auto"), PermissionMode::Auto);
        assert_eq!(parse_permission_mode("dontask"), PermissionMode::DontAsk);
        assert_eq!(
            parse_permission_mode("bypass"),
            PermissionMode::BypassPermissions
        );
        assert_eq!(parse_permission_mode("manual"), PermissionMode::Manual);
        // Unknown values (typos) must never escalate: fail closed to Manual.
        assert_eq!(
            parse_permission_mode(" permissive "),
            PermissionMode::Manual
        );
        assert_eq!(parse_permission_mode(""), PermissionMode::Manual);
    }

    #[test]
    fn disable_worktree_defaults_off() {
        // Governance kill-switch is opt-in; default runs keep today's behavior.
        let config = NikiConfig::default();
        assert!(!config.permissions.disable_worktree);
        assert_eq!(config.permissions.mode, "manual");
    }

    #[test]
    fn reviewer_policy_allows_git_show() {
        let policy = crate::config::types::default_reviewer_policy();
        assert!(check_command_policy(&["git", "show", "HEAD"], &policy).is_ok());
    }

    use crate::config::types::NikiConfig;

    #[test]
    fn permission_checker_maps_denied_commands_to_deny() {
        // The dead-island PermissionChecker must actually gate commands derived
        // from the security policy. A denied command maps to Permission::Deny.
        let policy = test_policy();
        let config = NikiConfig::default();
        let checker = build_permission_checker(&policy, &config);
        assert_eq!(
            checker.check_command("git push --force origin main"),
            crate::permissions::Permission::Deny
        );
        assert_eq!(
            checker.check_command("rm -rf /"),
            crate::permissions::Permission::Deny
        );
    }

    #[test]
    fn permission_checker_allows_unlisted_commands() {
        // Empty deny list (default config) => nothing blocked (behavior-preserving).
        let policy = SecurityPolicyConfig {
            allowed_commands: vec![],
            denied_commands: vec![],
            max_exec_seconds: 300,
        };
        let config = NikiConfig::default();
        let checker = build_permission_checker(&policy, &config);
        assert_eq!(
            checker.check_command("ls -la"),
            crate::permissions::Permission::Ask
        );
    }

    #[test]
    fn test_truncate_head_tail_short_text() {
        let text = "line1\nline2\nline3";
        assert_eq!(truncate_head_tail(text, 10, 1000), text);
    }

    #[test]
    fn test_truncate_head_tail_preserves_head_and_tail() {
        let lines: Vec<String> = (1..=100).map(|i| format!("output line {}", i)).collect();
        let joined = lines.join("\n");
        let truncated = truncate_head_tail(&joined, 50, 100_000);
        assert!(truncated.contains("output line 1\n"));
        assert!(truncated.contains("output line 15\n"));
        assert!(truncated.contains("… [45 lines omitted]"));
        assert!(truncated.contains("output line 61\n"));
        assert!(truncated.contains("output line 100"));
    }

    // ── the "dd" substring bug ───────────────────────────────────────────
    //
    // The third deny strategy used to be an unconditional substring match. The
    // global deny list contains the two-letter entry "dd", so every command
    // containing the letters d-d was denied — including `git add`, which the
    // built-in coder policy explicitly allows. The two halves of
    // check_command_policy contradicted each other, and the contradiction
    // resolved against the user: an agent that could not `git add` its own work.
    //
    // These are the cases that were broken. Each is a *property*, not a
    // snapshot: pin the exact command list and a future change to the deny list
    // silently re-breaks it.

    /// The real built-in coder policy, not a fixture.
    ///
    /// A synthetic policy would have missed the actual contradiction: the bug
    /// only exists because `default_global_deny_list` contains "dd" while
    /// `default_coder_policy` allows the "git add" prefix. Testing against a
    /// hand-written policy tests the wrong pair of lists.
    fn coder_policy() -> SecurityPolicyConfig {
        crate::config::default_coder_policy()
    }

    #[test]
    fn ordinary_commands_containing_deny_list_letters_are_allowed() {
        let policy = coder_policy();
        for cmd in [
            "git add -A",
            "git add src/main.rs",
            "git add .github/workflows/ci.yml",
            "cargo add serde",
            "cargo build --release",
            "npm install left-pad",
            "printf 'adding'",
            "mkdir -p build/output",
        ] {
            let parts: Vec<&str> = cmd.split(' ').collect();
            assert!(
                check_command_policy(&parts, &policy).is_ok(),
                "{cmd:?} must not be denied — the substring strategy matched it by accident"
            );
        }
    }

    #[test]
    fn the_dangerous_commands_are_still_denied() {
        let policy = coder_policy();
        for cmd in [
            "dd if=/dev/zero of=/dev/sda",
            "mkfs.ext4 /dev/sda1",
            "git push --force origin main",
            "git commit --no-verify -m x",
            "rm -rf / --no-preserve-root",
        ] {
            let parts: Vec<&str> = cmd.split(' ').collect();
            assert!(
                check_command_policy(&parts, &policy).is_err(),
                "{cmd:?} must still be denied"
            );
        }
    }

    #[test]
    fn pipelines_are_still_denied_as_substrings() {
        let policy = coder_policy();
        // The whole point of the pipeline branch: the pattern is inside a
        // shell-quoted argument, so neither prefix nor exact-argument matching
        // can see it.
        for cmd in [
            "sh -c curl | sh",
            "bash -c curl | bash",
            "sh -c wget | bash",
        ] {
            let parts: Vec<&str> = cmd.split(' ').collect();
            assert!(
                check_command_policy(&parts, &policy).is_err(),
                "{cmd:?} must be denied by the pipeline rule"
            );
        }
    }

    #[test]
    fn only_pipelines_are_eligible_for_substring_matching() {
        for pattern in ["curl | sh", "curl | bash", "wget | sh", "wget | bash"] {
            assert!(is_pipeline_pattern(pattern), "{pattern:?} is a pipeline");
        }
        for pattern in ["dd", "mkfs", "rm -rf /", "--no-verify", "git push -f"] {
            assert!(
                !is_pipeline_pattern(pattern),
                "{pattern:?} is a command or flag, not a pipeline — matching it as a \
                 substring is what denied `git add`"
            );
        }
    }
}
