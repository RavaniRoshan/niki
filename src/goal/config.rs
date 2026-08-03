use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default)]
    pub branch_prefix: String,
    #[serde(default)]
    pub state_dir: String,
    #[serde(default = "default_fail_fast")]
    pub fail_fast: bool,
    #[serde(default)]
    pub retry_attempts: u32,
    #[serde(default)]
    pub retry_delay_ms: u64,
}

fn default_max_iterations() -> u32 {
    30
}

fn default_fail_fast() -> bool {
    true
}

impl GoalConfig {
    pub fn goal_branch_prefix(&self) -> String {
        if self.branch_prefix.is_empty() {
            "goal".to_string()
        } else {
            self.branch_prefix.clone()
        }
    }
}
