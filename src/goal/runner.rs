use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

use crate::artifacts::types::Verdict;
use crate::config::types::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::goal::state::{GoalCriterion, GoalState, GoalStatus, GoalTask, TaskStatus};
use crate::orchestrator::pipeline::{PipelineResult, Task, execute_pipeline};
use crate::sandbox::docker::ActiveContainers;

pub struct GoalRunner;

impl GoalRunner {
    pub async fn run(
        state: &mut GoalState,
        config: &NikiConfig,
        docker: Option<&bollard::Docker>,
        display: &mut AgenticDisplay,
        containers: ActiveContainers,
    ) -> Result<()> {
        if state.status != GoalStatus::Active {
            return Err(anyhow::anyhow!(
                "Goal {} is not active (status: {})",
                state.slug,
                state.status
            ));
        }

        while state.iterations < state.max_iterations {
            if let Some(halt) = Self::halt_conditions(state) {
                state
                    .context_summary
                    .push_str(&format!("\nHalted: {}\n", halt));
                break;
            }

            if state.current_task >= state.tasks.len() {
                break;
            }

            let task_idx = state.current_task;
            let task_desc = state.tasks[task_idx].desc.clone();
            let task_id = state.tasks[task_idx].id;

            state.tasks[task_idx].status = TaskStatus::InProgress;
            state.save()?;

            state.context_summary.push_str(&format!(
                "\nIteration {}: working on task {}: {}",
                state.iterations + 1,
                task_id,
                task_desc
            ));

            let pipeline_task = Task {
                id: Uuid::new_v4(),
                description: task_desc.clone(),
                project_path: PathBuf::from(&state.scope),
            };

            match execute_pipeline(
                &pipeline_task,
                config,
                docker,
                display,
                containers.clone(),
                false,
            )
            .await
            {
                Ok(result) => {
                    let gate_passed = Self::staged_evidence_gates(&result, config).await;
                    if gate_passed {
                        state.tasks[task_idx].status = TaskStatus::Done;
                        state.context_summary.push_str(&format!(
                            "\n  Task {} completed. Evidence gates passed.",
                            task_id
                        ));
                    } else {
                        state.tasks[task_idx].status = TaskStatus::Blocked;
                        state
                            .negative_knowledge
                            .push(format!("Task {} failed evidence gates", task_id));
                        state.context_summary.push_str(&format!(
                            "\n  Task {} blocked: evidence gates failed.",
                            task_id
                        ));
                    }
                }
                Err(e) => {
                    state.tasks[task_idx].status = TaskStatus::Blocked;
                    state
                        .negative_knowledge
                        .push(format!("Task {} pipeline error: {}", task_id, e));
                    state
                        .context_summary
                        .push_str(&format!("\n  Task {} error: {}", task_id, e));
                }
            }

            state.iterations += 1;
            state.current_task += 1;
            state.save()?;
        }

        if state.tasks.iter().all(|t| t.status == TaskStatus::Done) {
            let results = Self::check_criteria(state);
            let must_pass_all_pass = state
                .criteria
                .iter()
                .zip(results.iter())
                .filter(|(c, _)| c.must_pass)
                .all(|(_, (_, pass))| *pass);
            if must_pass_all_pass {
                state.status = GoalStatus::Complete;
                state.completed_at = Some(chrono::Utc::now().to_rfc3339());
                state
                    .context_summary
                    .push_str("\nGoal completed: all must-pass criteria passed.\n");
                Self::write_completion_log(state)?;
                crate::goal::state::remove_claim_by_goal(&state.id)?;
            } else {
                state
                    .context_summary
                    .push_str("\nAll tasks done but must-pass criteria not met.\n");
            }
        } else {
            state
                .context_summary
                .push_str("\nGoal runner finished with remaining tasks.\n");
        }

        state.save()?;
        Ok(())
    }

    async fn staged_evidence_gates(result: &PipelineResult, _config: &NikiConfig) -> bool {
        if result.verdict == Verdict::Rejected {
            return false;
        }
        !result.final_diff.is_empty()
    }

    pub fn check_criteria(state: &GoalState) -> Vec<(String, bool)> {
        state
            .criteria
            .iter()
            .map(|c| {
                let output = std::process::Command::new("bash")
                    .arg("-c")
                    .arg(&c.check)
                    .output();

                let passed = match output {
                    Ok(out) => {
                        out.status.success()
                            && !String::from_utf8_lossy(&out.stdout).contains("FAIL")
                    }
                    Err(_) => false,
                };

                (c.label.clone(), passed)
            })
            .collect()
    }

    fn write_completion_log(state: &GoalState) -> Result<()> {
        let dir = crate::goal::state::goals_dir();
        let path = dir.join(format!("completion_log_{}.txt", state.id));
        let mut content = String::new();
        content.push_str("=== Goal Completion Report ===\n");
        content.push_str(&format!("Goal: {} ({})\n", state.objective, state.slug));
        content.push_str(&format!("Status: {}\n", state.status));
        content.push_str(&format!(
            "Completed: {}\n",
            state.completed_at.as_deref().unwrap_or("unknown")
        ));
        content.push_str(&format!("Total iterations: {}\n\n", state.iterations));
        content.push_str("--- Criteria Results ---\n");
        let results = Self::check_criteria(state);
        for (c, (label, passed)) in state.criteria.iter().zip(results.iter()) {
            let status = if *passed { "PASS" } else { "FAIL" };
            let gate = if c.must_pass { "[must-pass]" } else { "[optional]" };
            content.push_str(&format!("  {} {} {} (gate={})\n", status, gate, label, c.check));
        }
        content.push_str("\n--- Tasks Completed ---\n");
        for t in &state.tasks {
            let icon = match t.status {
                TaskStatus::Done => "✓",
                TaskStatus::Blocked => "✗",
                TaskStatus::InProgress => "→",
                _ => " ",
            };
            content.push_str(&format!("  {} [{}] {}\n", icon, t.id, t.desc));
        }
        content.push_str(&format!(
            "\n--- Context Summary ---\n{}\n",
            state.context_summary
        ));
        let _ = std::fs::write(&path, content);
        println!("  Completion log: {}", path.display());
        Ok(())
    }

    pub fn halt_conditions(state: &GoalState) -> Option<String> {
        if state.status == GoalStatus::Paused {
            return Some("Goal is paused".to_string());
        }
        if state.status == GoalStatus::Cancelled {
            return Some("Goal is cancelled".to_string());
        }
        if state.iterations >= state.max_iterations {
            return Some(format!("Max iterations ({}) reached", state.max_iterations));
        }
        let violations = Self::scope_violations(state);
        if !violations.is_empty() {
            return Some(format!("Scope violations: {}", violations.join(", ")));
        }
        None
    }

    fn scope_violations(state: &GoalState) -> Vec<String> {
        let mut violations = Vec::new();
        if state.scope_lock.is_empty() || state.scope_lock.iter().all(|s| s == ".") {
            return violations;
        }
        if state.iterations > 0 && !state.negative_knowledge.is_empty() {
            violations.push("blocked tasks detected".to_string());
        }
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::TEST_CWD_LOCK;

    fn make_active_state() -> GoalState {
        GoalState {
            id: "test".to_string(),
            slug: "test".to_string(),
            objective: "Test".to_string(),
            status: GoalStatus::Active,
            branch: "goal/test".to_string(),
            scope: ".".to_string(),
            scope_lock: vec![],
            scope_flex: vec![],
            criteria: vec![],
            tasks: vec![],
            current_task: 0,
            iterations: 0,
            budget_used: 0,
            max_iterations: 30,
            negative_knowledge: vec![],
            context_summary: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
        }
    }

    #[test]
    fn test_halt_conditions_active() {
        let state = make_active_state();
        assert!(GoalRunner::halt_conditions(&state).is_none());
    }

    #[test]
    fn test_halt_conditions_paused() {
        let mut state = make_active_state();
        state.status = GoalStatus::Paused;
        assert!(GoalRunner::halt_conditions(&state).is_some());
    }

    #[test]
    fn test_halt_conditions_cancelled() {
        let mut state = make_active_state();
        state.status = GoalStatus::Cancelled;
        assert!(GoalRunner::halt_conditions(&state).is_some());
    }

    #[test]
    fn test_halt_conditions_max_iterations() {
        let mut state = make_active_state();
        state.iterations = 30;
        assert!(GoalRunner::halt_conditions(&state).is_some());
    }

    #[test]
    fn test_check_criteria_empty() {
        let state = make_active_state();
        let results = GoalRunner::check_criteria(&state);
        assert!(results.is_empty());
    }

    #[test]
    fn test_halt_conditions_with_negative_knowledge_in_scope() {
        let mut state = make_active_state();
        state.scope_lock = vec!["src/".to_string()];
        state.iterations = 1;
        state.negative_knowledge = vec!["something went wrong".to_string()];
        assert!(GoalRunner::halt_conditions(&state).is_some());
    }

    #[test]
    fn test_halt_conditions_no_violation_when_scope_loose() {
        let mut state = make_active_state();
        state.scope_lock = vec![".".to_string()];
        state.iterations = 1;
        state.negative_knowledge = vec!["something went wrong".to_string()];
        assert!(GoalRunner::halt_conditions(&state).is_none());
    }

    #[test]
    fn test_completion_log_written() {
        let tmp = tempfile::TempDir::new().unwrap();
        let _guard = TEST_CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        std::fs::create_dir_all(tmp.path().join(".opencode/goals")).unwrap();

        let mut state = make_active_state();
        state.id = "compl123".to_string();
        state.slug = "test-log".to_string();
        state.completed_at = Some("2026-08-13T00:00:00Z".to_string());
        state.criteria = vec![GoalCriterion {
            label: "Test pass".to_string(),
            check: "true".to_string(),
            must_pass: true,
            coverage_gate: false,
            result: Some("PASS".to_string()),
        }];
        state.tasks = vec![GoalTask {
            id: 1,
            desc: "Do something".to_string(),
            status: TaskStatus::Done,
        }];

        GoalRunner::write_completion_log(&state).unwrap();
        let log_path = tmp.path().join(".opencode/goals/completion_log_compl123.txt");
        assert!(log_path.exists());
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("Goal Completion Report"));
        assert!(content.contains("Test pass"));

        std::env::set_current_dir(original_cwd).unwrap();
    }
}
