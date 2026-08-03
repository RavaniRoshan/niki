use anyhow::Result;
use std::path::Path;

use crate::goal::state::{GoalState, GoalCriterion, GoalTask, GoalStatus};

pub struct GoalCreator;

impl GoalCreator {
    pub fn create(
        objective: &str,
        scope: Option<&str>,
        max_iterations: u32,
    ) -> Result<GoalState> {
        let slug = slugify(objective);
        let id = generate_id();
        let branch_name = format!("goal/{}-{}", slug, id);

        let scope_lock = match scope {
            Some(s) => vec![s.to_string()],
            None => vec![".".to_string()],
        };

        let state = GoalState {
            id: id.clone(),
            slug: format!("{}-{}", slug, id),
            objective: objective.to_string(),
            status: GoalStatus::Active,
            branch: branch_name,
            scope: scope.unwrap_or(".").to_string(),
            scope_lock,
            scope_flex: vec![],
            criteria: vec![],
            tasks: vec![],
            current_task: 0,
            iterations: 0,
            budget_used: 0,
            max_iterations,
            negative_knowledge: vec![],
            context_summary: format!("Goal: {}\nIteration 0: created.\n", objective),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
        };

        Ok(state)
    }
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-")
}

fn generate_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    chrono::Utc::now().timestamp().hash(&mut hasher);
    format!("{:x}", hasher.finish())[..6].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        let slug = slugify("Fix N+1 queries in the dashboard");
        assert_eq!(slug, "fix-n-1-queries-in-the");
    }

    #[test]
    fn test_slugify_short() {
        let slug = slugify("Build login page");
        assert_eq!(slug, "build-login-page");
    }

    #[test]
    fn test_generate_id_length() {
        let id = generate_id();
        assert_eq!(id.len(), 6);
    }
}