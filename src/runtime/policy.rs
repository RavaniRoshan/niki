//! Policy enforcement — central policy engine governing agent actions.
//!
//! Two levels of policy:
//! 1. Pipeline Policy: controls which agent stages run (see `crate::risk`).
//! 2. Tool Policy: controls which concrete actions/tools an agent can execute.

use crate::artifacts::types::AgentRole;
pub use crate::runtime::tools::{PermissionRequirement, RiskLevel, ToolCategory};
use std::collections::HashMap;

/// A policy violation preventing a tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyViolation {
    PermissionDenied(String),
    RoleNotAllowed(String),
    RiskRestricted(String),
    CommandDenied(String),
    PathDenied(String),
}

impl std::fmt::Display for PolicyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyViolation::PermissionDenied(s) => write!(f, "Permission denied: {}", s),
            PolicyViolation::RoleNotAllowed(s) => write!(f, "Role not allowed: {}", s),
            PolicyViolation::RiskRestricted(s) => write!(f, "Risk restricted: {}", s),
            PolicyViolation::CommandDenied(s) => write!(f, "Command denied: {}", s),
            PolicyViolation::PathDenied(s) => write!(f, "Path denied: {}", s),
        }
    }
}

impl std::error::Error for PolicyViolation {}

/// Policy controlling what tools an agent can execute.
#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub role: AgentRole,
    pub max_risk: RiskLevel,
    pub permission_mode: String,
    pub allowed_categories: Vec<ToolCategory>,
    pub allowed_tools: Option<Vec<String>>,
    pub denied_tools: Vec<String>,
    pub denied_commands: Vec<String>,
    pub denied_paths: Vec<String>,
    pub overrides: HashMap<String, PermissionRequirement>,
}

impl ToolPolicy {
    /// Create a standard policy for a given agent role and environment risk tier.
    pub fn for_role(role: AgentRole, risk_level: RiskLevel, permission_mode: &str) -> Self {
        let mut policy = Self {
            role,
            max_risk: risk_level,
            permission_mode: permission_mode.to_string(),
            allowed_categories: Vec::new(),
            allowed_tools: None,
            denied_tools: Vec::new(),
            denied_commands: vec!["rm -rf /".to_string(), "mkfs".to_string(), "dd".to_string()],
            denied_paths: vec![".git/".to_string()],
            overrides: HashMap::new(),
        };

        match role {
            AgentRole::Planner => {
                // Planner is strictly explore/read-only + knowledge
                policy.allowed_categories = vec![
                    ToolCategory::Explore,
                    ToolCategory::Knowledge,
                    ToolCategory::Research,
                ];
                policy.denied_tools = vec![
                    "write".to_string(),
                    "edit".to_string(),
                    "patch".to_string(),
                    "bash".to_string(),
                ];
            }
            AgentRole::Coder => {
                // Coder can explore, modify, execute bash/tests, knowledge, vcs
                policy.allowed_categories = vec![
                    ToolCategory::Explore,
                    ToolCategory::Modify,
                    ToolCategory::Execute,
                    ToolCategory::Knowledge,
                    ToolCategory::Vcs,
                ];
                // Coder is blocked from git push
                policy.denied_commands.push("git push".to_string());
            }
            AgentRole::Tester => {
                // Tester can explore and execute test commands
                policy.allowed_categories = vec![
                    ToolCategory::Explore,
                    ToolCategory::Execute,
                    ToolCategory::Knowledge,
                    ToolCategory::Vcs,
                ];
                policy.denied_tools =
                    vec!["write".to_string(), "edit".to_string(), "patch".to_string()];
                policy.denied_commands.push("git push".to_string());
                policy.denied_commands.push("git commit".to_string());
            }
            AgentRole::Reviewer
            | AgentRole::SecurityAuditor
            | AgentRole::Red
            | AgentRole::Critic => {
                // Reviewer and audit roles are strictly read-only
                policy.allowed_categories = vec![
                    ToolCategory::Explore,
                    ToolCategory::Knowledge,
                    ToolCategory::Vcs,
                ];
                policy.denied_tools = vec![
                    "write".to_string(),
                    "edit".to_string(),
                    "patch".to_string(),
                    "bash".to_string(),
                ];
                policy.denied_commands.push("git commit".to_string());
                policy.denied_commands.push("git push".to_string());
            }
            AgentRole::Synthesizer => {
                policy.allowed_categories = vec![
                    ToolCategory::Explore,
                    ToolCategory::Modify,
                    ToolCategory::Knowledge,
                ];
            }
        }

        policy
    }

    /// Check if a tool can execute according to policy rules.
    pub fn check_permission(
        &self,
        tool_name: &str,
        category: ToolCategory,
        risk: RiskLevel,
        declared_perm: PermissionRequirement,
    ) -> Result<(), PolicyViolation> {
        // 1. Check explicit tool denials
        if self.denied_tools.iter().any(|d| d == tool_name) {
            return Err(PolicyViolation::RoleNotAllowed(format!(
                "tool '{}' is explicitly denied for role {:?}",
                tool_name, self.role
            )));
        }

        // 2. Check category allow-list
        if !self.allowed_categories.contains(&category) {
            return Err(PolicyViolation::RoleNotAllowed(format!(
                "category '{:?}' is not allowed for role {:?}",
                category, self.role
            )));
        }

        // 3. Check allowed tools list if restricted
        if let Some(ref allowed) = self.allowed_tools {
            if !allowed.iter().any(|a| a == tool_name) {
                return Err(PolicyViolation::RoleNotAllowed(format!(
                    "tool '{}' is not in allowed list for role {:?}",
                    tool_name, self.role
                )));
            }
        }

        // 4. Check risk level ceiling
        if risk > self.max_risk {
            return Err(PolicyViolation::RiskRestricted(format!(
                "tool '{}' risk level {:?} exceeds allowed max {:?}",
                tool_name, risk, self.max_risk
            )));
        }

        // 5. Check permission requirement (Allow/Ask/Deny)
        let effective_perm = self
            .overrides
            .get(tool_name)
            .copied()
            .unwrap_or(declared_perm);

        match effective_perm {
            PermissionRequirement::Allow => Ok(()),
            PermissionRequirement::Deny => Err(PolicyViolation::PermissionDenied(format!(
                "tool '{}' is denied by policy override",
                tool_name
            ))),
            PermissionRequirement::Ask => {
                match self.permission_mode.to_ascii_lowercase().as_str() {
                    "bypass" | "dontask" | "auto" => Ok(()),
                    _ => Err(PolicyViolation::PermissionDenied(format!(
                        "tool '{}' requires approval (Ask) but permission mode '{}' operates headless",
                        tool_name, self.permission_mode
                    ))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planner_policy_blocks_edit() {
        let policy = ToolPolicy::for_role(AgentRole::Planner, RiskLevel::Critical, "manual");
        let res = policy.check_permission(
            "edit",
            ToolCategory::Modify,
            RiskLevel::Medium,
            PermissionRequirement::Allow,
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_coder_policy_allows_edit() {
        let policy = ToolPolicy::for_role(AgentRole::Coder, RiskLevel::Critical, "manual");
        let res = policy.check_permission(
            "edit",
            ToolCategory::Modify,
            RiskLevel::Medium,
            PermissionRequirement::Allow,
        );
        assert!(res.is_ok());
    }
}
