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
}
