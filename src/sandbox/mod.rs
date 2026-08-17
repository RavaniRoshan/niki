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
    })
}

pub mod docker;
pub mod edit_format;
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
    /// Return the working-tree diff produced inside the sandbox.
    async fn get_diff(&self) -> Result<String>;
    /// Run a command inside the sandbox, returning its exit code + output.
    ///
    /// When `role` is provided and a security policy exists for that role, the
    /// command is checked against the deny-list before execution. Denied
    /// commands are rejected with a clear error message.
    async fn exec(&self, cmd: &[&str], role: Option<&AgentRole>) -> Result<ExecOutput>;
    /// Tear the sandbox down (remove containers / worktrees).
    async fn destroy(&self) -> Result<()>;
}

/// Check whether `cmd` is allowed by `policy`. Returns `Ok(())` if allowed,
/// or `Err` with a descriptive message if denied.
pub fn check_command_policy(cmd: &[&str], policy: &SecurityPolicyConfig) -> Result<()> {
    let full_cmd = cmd.join(" ");

    // Allow-list takes precedence: if the command starts with any allowed prefix, skip deny check.
    for allowed in &policy.allowed_commands {
        if full_cmd.starts_with(allowed) {
            return Ok(());
        }
    }

    // The global deny-list is *always* enforced for every role, in addition to
    // any per-role denies. (Previously the per-role policies overrode it, which
    // let the coder/reviewer roles run dangerous commands like `curl | sh`,
    // `mkfs`, `dd`, or `rm -rf`.) See research report S1.
    let mut denied: Vec<String> = policy.denied_commands.clone();
    denied.extend(crate::config::default_global_deny_list());

    // Check deny-list using two strategies:
    // 1. Prefix match on the full joined command (catches "git push --force origin main")
    // 2. Individual argument match (catches "git commit --no-verify")
    // 3. Substring match on the full command (catches "sh -c 'curl | sh'")
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
        // Substring match for patterns like "curl | sh" that may appear inside
        // shell-quoted arguments (e.g. sh -c "curl | sh").
        if full_cmd.contains(denied) {
            return Err(anyhow!(
                "Command denied by security policy: '{}' contains denied pattern '{}'",
                full_cmd,
                denied
            ));
        }
    }

    Ok(())
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
}
