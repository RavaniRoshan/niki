//! Enhanced permission system — per-tool allow/ask/deny with pattern matching.
//!
//! Provides granular control over what tools/agents can do, matching
//! the permission models of OpenCode and KiloCode.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Permission decision for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Permission {
    Allow,
    #[default]
    Ask,
    Deny,
}

/// User response to a permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
}

/// Top-level permission *mode* (kimi/Claude Code parity).
///
/// - `Manual`: every sensitive action is prompted (`Ask`). Safe default.
/// - `Auto`: inside a hermetic sandbox, file/shell actions are auto-approved;
///   host-reaching actions (network egress, git push, config writes) still ask.
/// - `DontAsk`: accept all actions without prompting (YOLO mode).
/// - `BypassPermissions`: bypass all permission checks entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum PermissionMode {
    #[default]
    Manual,
    Auto,
    DontAsk,
    BypassPermissions,
}

/// Lifetime over which an approval stays in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum PermissionScope {
    /// Only the current tool call / turn.
    Turn,
    /// The rest of this interactive session.
    #[default]
    Session,
    /// Persisted for this project.
    Project,
    /// Persisted globally for this user.
    User,
}

/// A permission rule with optional pattern matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub permission: Permission,
    pub pattern: Option<String>,
}

/// Tool categories for permission control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissions {
    pub read: Permission,
    pub edit: Permission,
    pub bash: Permission,
    pub glob: Permission,
    pub grep: Permission,
    pub task: Permission,
    pub webfetch: Permission,
}

impl Default for ToolPermissions {
    fn default() -> Self {
        Self {
            read: Permission::Allow,
            edit: Permission::Ask,
            bash: Permission::Ask,
            glob: Permission::Allow,
            grep: Permission::Allow,
            task: Permission::Allow,
            webfetch: Permission::Allow,
        }
    }
}

/// The permission system configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub tools: ToolPermissions,
    pub rules: HashMap<String, PermissionRule>,
    pub auto_approve: bool,
    pub external_directory: Permission,
    pub doom_loop: Permission,
    /// Active permission mode (Manual/Auto/DontAsk/BypassPermissions).
    pub mode: PermissionMode,
    /// Scope at which approvals are promoted.
    pub scope: PermissionScope,
}

/// Permission checker — evaluates whether an action is allowed.
pub struct PermissionChecker {
    config: PermissionConfig,
}

impl PermissionChecker {
    /// Create a new permission checker.
    pub fn new(config: PermissionConfig) -> Self {
        Self { config }
    }

    /// Check if a tool action is permitted, resolving the active
    /// [`PermissionMode`] (Manual/Auto/DontAsk/BypassPermissions) and sandbox context.
    ///
    /// - `DontAsk` accepts everything.
    /// - `Auto` auto-allows file/shell ops that run *inside* a hermetic sandbox,
    ///   but still prompts for host-reaching actions (network egress via
    ///   `webfetch`, etc.); outside the sandbox it falls back to the per-tool
    ///   config.
    /// - `Manual` (and the default) uses the per-tool `Permission` config.
    pub fn resolve_tool(&self, tool: &str, in_sandbox: bool) -> Permission {
        match self.config.mode {
            PermissionMode::DontAsk | PermissionMode::BypassPermissions => {
                return Permission::Allow
            }
            PermissionMode::Auto => {
                let sandbox_safe = matches!(
                    tool,
                    "read" | "edit" | "write" | "bash" | "glob" | "grep" | "task"
                );
                if in_sandbox && sandbox_safe {
                    return Permission::Allow;
                }
                // Host-reaching actions (network egress) always ask, even in
                // Auto mode, to preserve Niki's sandbox differentiator.
                if matches!(tool, "webfetch" | "websearch") {
                    return Permission::Ask;
                }
            }
            PermissionMode::Manual => {}
        }
        self.check_tool(tool)
    }

    /// Check if a tool action is permitted.
    pub fn check_tool(&self, tool: &str) -> Permission {
        match tool {
            "read" => self.config.tools.read,
            "edit" => self.config.tools.edit,
            "bash" => self.config.tools.bash,
            "glob" => self.config.tools.glob,
            "grep" => self.config.tools.grep,
            "task" => self.config.tools.task,
            "webfetch" => self.config.tools.webfetch,
            _ => Permission::Ask,
        }
    }

    /// Check a command against bash permission rules.
    pub fn check_command(&self, command: &str) -> Permission {
        for rule in self.config.rules.values() {
            if let Some(ref pattern) = rule.pattern
                && command.contains(pattern)
            {
                return rule.permission;
            }
        }
        self.config.tools.bash
    }

    /// Check if auto-approve is enabled.
    pub fn auto_approve(&self) -> bool {
        self.config.auto_approve
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_default() {
        let perm = Permission::default();
        assert_eq!(perm, Permission::Ask);
    }

    #[test]
    fn tool_permissions_default() {
        let perms = ToolPermissions::default();
        assert_eq!(perms.read, Permission::Allow);
        assert_eq!(perms.edit, Permission::Ask);
        assert_eq!(perms.bash, Permission::Ask);
    }

    #[test]
    fn checker_check_tool() {
        let config = PermissionConfig {
            tools: ToolPermissions::default(),
            ..Default::default()
        };
        let checker = PermissionChecker::new(config);
        assert_eq!(checker.check_tool("read"), Permission::Allow);
        assert_eq!(checker.check_tool("edit"), Permission::Ask);
        assert_eq!(checker.check_tool("unknown"), Permission::Ask);
    }

    #[test]
    fn checker_check_command() {
        let mut rules = HashMap::new();
        rules.insert(
            "cargo_test".to_string(),
            PermissionRule {
                permission: Permission::Allow,
                pattern: Some("cargo test".to_string()),
            },
        );
        let config = PermissionConfig {
            rules,
            ..Default::default()
        };
        let checker = PermissionChecker::new(config);
        assert_eq!(
            checker.check_command("cargo test --release"),
            Permission::Allow
        );
        assert_eq!(checker.check_command("rm -rf /"), Permission::Ask);
    }

    #[test]
    fn checker_auto_approve() {
        let config = PermissionConfig {
            auto_approve: true,
            ..Default::default()
        };
        let checker = PermissionChecker::new(config);
        assert!(checker.auto_approve());
    }

    #[test]
    fn resolve_tool_dontask_allows_everything() {
        let config = PermissionConfig {
            mode: PermissionMode::DontAsk,
            ..Default::default()
        };
        let checker = PermissionChecker::new(config);
        assert_eq!(checker.resolve_tool("edit", false), Permission::Allow);
        assert_eq!(checker.resolve_tool("webfetch", false), Permission::Allow);
    }

    #[test]
    fn resolve_tool_auto_allows_sandbox_file_shell() {
        let config = PermissionConfig {
            mode: PermissionMode::Auto,
            ..Default::default()
        };
        let checker = PermissionChecker::new(config);
        assert_eq!(checker.resolve_tool("bash", true), Permission::Allow);
        assert_eq!(checker.resolve_tool("edit", true), Permission::Allow);
        // Host-reaching network egress still asks, even in Auto.
        assert_eq!(checker.resolve_tool("webfetch", true), Permission::Ask);
        // Outside the sandbox, falls back to per-tool config (edit = Ask).
        assert_eq!(checker.resolve_tool("edit", false), Permission::Ask);
    }

    #[test]
    fn resolve_tool_manual_uses_per_tool_config() {
        let config = PermissionConfig {
            mode: PermissionMode::Manual,
            ..Default::default()
        };
        let checker = PermissionChecker::new(config);
        assert_eq!(checker.resolve_tool("read", false), Permission::Allow);
        assert_eq!(checker.resolve_tool("edit", false), Permission::Ask);
        assert_eq!(checker.resolve_tool("edit", true), Permission::Ask);
    }
}
