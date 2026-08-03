use anyhow::Result;
use std::path::PathBuf;
use uuid::Uuid;

use crate::goal::state::{GoalState, GoalStatus, TaskStatus};
use crate::config::types::NikiConfig;
use crate::orchestrator::pipeline::{execute_pipeline, Task, PipelineResult};
use crate::artifacts::types::Verdict;
use crate::sandbox::docker::ActiveContainers;
use crate::display::agent_stream::AgenticDisplay;

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
                state.context_summary.push_str(&format!("\nHalted: {}\n", halt));
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
            ).await {
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
                        state.negative_knowledge.push(format!(
                            "Task {} failed evidence gates",
                            task_id
                        ));
                        state.context_summary.push_str(&format!(
                            "\n  Task {} blocked: evidence gates failed.",
                            task_id
                        ));
                    }
                }
                Err(e) => {
                    state.tasks[task_idx].status = TaskStatus::Blocked;
                    state.negative_knowledge.push(format!(
                        "Task {} pipeline error: {}",
                        task_id, e
                    ));
                    state.context_summary.push_str(&format!(
                        "\n  Task {} error: {}",
                        task_id, e
                    ));
                }
            }

            state.iterations += 1;
            state.current_task += 1;
            state.save()?;
        }

        if state.tasks.iter().all(|t| t.status == TaskStatus::Done) {
            let results = Self::check_criteria(state);
            let all_pass = results.iter().all(|(_, pass)| *pass);
            if all_pass {
                state.status = GoalStatus::Complete;
                state.completed_at = Some(chrono::Utc::now().to_rfc3339());
                state.context_summary.push_str("\nGoal completed: all criteria passed.\n");
            } else {
                state.context_summary.push_str("\nAll tasks done but criteria not met.\n");
            }
        } else {
            state.context_summary.push_str("\nGoal runner finished with remaining tasks.\n");
        }

        state.save()?;
        Ok(())
    }

    async fn staged_evidence_gates(
        result: &PipelineResult,
        _config: &NikiConfig,
    ) -> bool {
        if result.verdict == Verdict::Rejected {
            return false;
        }

        true
    }

    pub fn check_criteria(state: &GoalState) -> Vec<(String, bool)> {
        state
            .criteria
            .iter()
            .map(|c| {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&c.check)
                    .output();

                let passed = match output {
                    Ok(out) => out.status.success()
                        && !String::from_utf8_lossy(&out.stdout).contains("FAIL"),
                    Err(_) => false,
                };

                (c.label.clone(), passed)
            })
            .collect()
    }

    pub fn halt_conditions(state: &GoalState) -> Option<String> {
        if state.status == GoalStatus::Paused {
            return Some("Goal is paused".to_string());
        }
        if state.status == GoalStatus::Cancelled {
            return Some("Goal is cancelled".to_string());
        }
        if state.iterations >= state.max_iterations {
            return Some(format!(
                "Max iterations ({}) reached",
                state.max_iterations
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
