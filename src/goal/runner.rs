use anyhow::Result;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::artifacts::types::Verdict;
use crate::config::types::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::goal::state::{GoalState, GoalStatus, TaskStatus};
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
                description: Self::description_with_prior_knowledge(state, &task_desc),
                project_path: PathBuf::from(&state.scope),
            };

            let goal_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let task_dir = pipeline_task
                .project_path
                .join(&config.general.output_dir)
                .join("tasks")
                .join(pipeline_task.id.to_string());
            // Before the run, for the hermetic proof — `niki run` and the
            // chat both take it here, because a snapshot taken afterwards
            // describes the change rather than the state it changed.
            let pre_snapshot = crate::safety::snapshot(Path::new(&state.scope)).ok();
            match execute_pipeline(
                &pipeline_task,
                config,
                docker,
                display,
                containers.clone(),
                false,
                goal_cancel.clone(),
                &task_dir,
                None,
                false,
            )
            .await
            {
                Ok(mut result) => {
                    // Deliver, or the iteration's work is thrown away.
                    //
                    // This path called `execute_pipeline` and stopped, so
                    // `niki goal` ran four agents per iteration and kept
                    // nothing: no branch, no patch, no report. The chat and
                    // `niki run` were fixed in T3a/T3 and this one was left,
                    // which is how the product's central promise held on two
                    // of its three front doors and not the third.
                    let branch_name = format!("niki/{}", &pipeline_task.id.to_string()[..8]);
                    let uses_docker = docker.is_some();
                    let delivered = crate::orchestrator::deliver::deliver(
                        crate::orchestrator::deliver::DeliverInput {
                            task: &pipeline_task,
                            config,
                            result: &mut result,
                            project_dir: Path::new(&state.scope),
                            task_dir: &task_dir,
                            branch_name: branch_name.clone(),
                            pre_snapshot: pre_snapshot.as_ref(),
                            uses_docker,
                            dry_run: false,
                            force: false,
                            json_mode: false,
                            display: None,
                        },
                    );
                    if let Err(e) = &delivered {
                        // The agents ran; the deliverable did not land. The
                        // task is Blocked, not Done, and the reason is the
                        // delivery failure rather than a missing gate.
                        state.tasks[task_idx].status = TaskStatus::Blocked;
                        state
                            .negative_knowledge
                            .push(format!("Task {task_id} ran but delivery failed: {e}"));
                        state.context_summary.push_str(&format!(
                            "\n  Task {task_id} blocked: delivery failed ({e})."
                        ));
                        continue;
                    }
                    let delivered = delivered.expect("checked above");
                    if !delivered.branch_created
                        && let Some(note) = &delivered.block_note
                    {
                        state
                            .context_summary
                            .push_str(&format!("\n  Task {task_id} produced no branch: {note}"));
                    }
                    // Phase 5.5: accrue the task's estimated cost into the
                    // goal budget (micro-USD) so the cost halt can fire.
                    let task_usd: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
                    state.budget_used = state
                        .budget_used
                        .saturating_add((task_usd * 1_000_000.0) as u64);
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

    /// The task description, plus everything previous iterations learned.
    ///
    /// The goal loop was accumulating `context_summary` and `negative_knowledge`
    /// faithfully — appending to them, persisting them, and then handing the
    /// pipeline a `Task` built from `task_desc` and `project_path` alone. The
    /// agents were given no memory of what the previous iterations had already
    /// tried and failed at, which is the single thing a multi-iteration runner
    /// exists to prevent: iteration 2 repeating iteration 1's mistake, for want
    /// of any record that it was a mistake.
    ///
    /// So the knowledge is carried on the one field the pipeline already reads.
    /// Nothing is stored that is not used, and the accumulated state and what
    /// the agents see are the same string.
    ///
    /// Iteration 1 gets the description unchanged. A runner that opens with
    /// "nothing has been tried yet" is noise, and noise in a prompt is
    /// expensive.
    fn description_with_prior_knowledge(state: &GoalState, task_desc: &str) -> String {
        if state.context_summary.trim().is_empty() && state.negative_knowledge.is_empty() {
            return task_desc.to_string();
        }

        let mut out = String::new();
        out.push_str(task_desc);

        // The current iteration's own preamble was just pushed onto
        // `context_summary`; echoing it back would list the task in the history
        // of the task.
        //
        // Matching by line, not by trimming a suffix. The first attempt trimmed
        // the trailing marker text, which cannot work when the marker is at the
        // *start* of the last line rather than the end of the string — and the
        // test written for exactly this case caught it.
        let current_marker = format!("Iteration {}: working on task", state.iterations + 1);
        let prior: String = state
            .context_summary
            .lines()
            .filter(|l| !l.trim_start().starts_with(&current_marker))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        if !prior.is_empty() {
            out.push_str(
                "

## What earlier iterations of this goal found

",
            );
            out.push_str(&prior);
        }

        if !state.negative_knowledge.is_empty() {
            out.push_str(
                "

## Approaches that already failed — do not repeat them

",
            );
            for item in state.negative_knowledge.iter().rev().take(10).rev() {
                out.push_str("- ");
                out.push_str(item);
                out.push('\n');
            }
        }

        out
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
            let gate = if c.must_pass {
                "[must-pass]"
            } else {
                "[optional]"
            };
            content.push_str(&format!(
                "  {} {} {} (gate={})\n",
                status, gate, label, c.check
            ));
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
        if state.status == GoalStatus::Drifting {
            return Some("Goal is drifting".to_string());
        }
        if let Some(drift) = state.check_drift() {
            return Some(format!(
                "Goal drift detected (adherence {:.2}, coherence {:.2}, reentry {:.2})",
                drift.goal_adherence, drift.env_coherence, drift.reentry_rate
            ));
        }
        if state.iterations >= state.max_iterations {
            return Some(format!("Max iterations ({}) reached", state.max_iterations));
        }
        // Phase 5.5: cost halt alongside the iteration halt. `budget_used`
        // accrues micro-USD per executed pipeline task (see the Ok arm below).
        if state.max_budget > 0 && state.budget_used >= state.max_budget {
            return Some(format!(
                "Budget exhausted (used {} of {} micro-USD)",
                state.budget_used, state.max_budget
            ));
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
    use crate::goal::state::{GoalCriterion, GoalTask};

    /// A multi-iteration runner exists so that iteration 2 does not repeat
    /// iteration 1's mistake. It could not do that: `context_summary` and
    /// `negative_knowledge` were accumulated, persisted, and then never handed
    /// to the pipeline, which received a `Task` built from the task description
    /// and the project path alone.
    ///
    /// These three cases are the whole contract. If the first one ever comes
    /// back — a first iteration that opens with a history of nothing — the
    /// prompt is carrying a lie in the other direction, and the loop is now
    /// spending tokens to say so.
    #[test]
    fn a_first_iteration_gets_the_description_unchanged() {
        let state = make_active_state();
        assert!(
            state.context_summary.trim().is_empty(),
            "the fixture is supposed to start with no history"
        );
        assert_eq!(
            GoalRunner::description_with_prior_knowledge(&state, "Add /health"),
            "Add /health",
            "a runner that opens with 'nothing has been tried yet' is noise in a prompt"
        );
    }

    #[test]
    fn later_iterations_carry_what_earlier_ones_found() {
        let mut state = make_active_state();
        state.context_summary = "\nIteration 1: working on task t1: Add /health".to_string();
        state.negative_knowledge = vec![
            "Task t1 failed evidence gates".to_string(),
            "Task t0 pipeline error: provider 401".to_string(),
        ];
        state.iterations = 1;

        let desc = GoalRunner::description_with_prior_knowledge(&state, "Add /metrics");

        assert!(
            desc.starts_with("Add /metrics"),
            "the actual task must be first and unchanged:
{desc}"
        );
        assert!(
            desc.contains("failed evidence gates"),
            "what an earlier iteration learned is missing, so iteration 2 will              repeat iteration 1's failure:
{desc}"
        );
        assert!(
            desc.contains("provider 401"),
            "recorded negative knowledge is missing from the prompt:
{desc}"
        );
    }

    /// The current iteration's own preamble is pushed onto `context_summary`
    /// immediately before the task is built. Echoing it back would put the
    /// task in the history of the task, which is worse than omitting it.
    #[test]
    fn the_current_iteration_is_not_reported_as_prior_knowledge() {
        let mut state = make_active_state();
        state.context_summary = "\nIteration 1: working on task t1: Add /health".to_string();
        state.iterations = 1;

        // The runner pushes this line, then builds the task.
        state.context_summary.push_str(&format!(
            "\nIteration {}: working on task {}: {}",
            state.iterations + 1,
            "t2",
            "Add /metrics"
        ));

        let desc = GoalRunner::description_with_prior_knowledge(&state, "Add /metrics");
        let occurrences = desc.matches("Iteration 2: working on task").count();
        assert_eq!(
            occurrences, 0,
            "the current iteration appeared in its own prior-knowledge section:
{desc}"
        );
    }

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
            max_budget: 0,
            max_iterations: 30,
            negative_knowledge: vec![],
            context_summary: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            drift: None,
            fork_dir: None,
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
    fn test_halt_conditions_budget_exhausted() {
        // Phase 5.5: the goal loop halts on budget as well as iterations.
        let mut state = make_active_state();
        state.max_budget = 1_000_000;
        state.budget_used = 1_000_000;
        let halt = GoalRunner::halt_conditions(&state).expect("budget halt fires");
        assert!(halt.contains("Budget exhausted"), "{halt}");
        // Unlimited (0) never halts on cost.
        let mut state = make_active_state();
        state.budget_used = u64::MAX;
        assert!(GoalRunner::halt_conditions(&state).is_none());
    }

    #[test]
    fn test_halt_conditions_drifting_status() {
        let mut state = make_active_state();
        state.status = GoalStatus::Drifting;
        let halt = GoalRunner::halt_conditions(&state).expect("drifting halt fires");
        assert_eq!(halt, "Goal is drifting");
    }

    #[test]
    fn test_halt_conditions_drift_signal_detected() {
        use crate::goal::state::DriftSignals;

        let mut state = make_active_state();
        state.drift = Some(DriftSignals {
            goal_adherence: 0.35, // below 0.5 threshold
            env_coherence: 0.8,
            reentry_rate: 0.1,
            checked_at: "2026-09-21T00:00:00Z".to_string(),
        });
        let halt = GoalRunner::halt_conditions(&state).expect("drift signal halt fires");
        assert!(halt.contains("Goal drift detected"), "{halt}");
        assert!(halt.contains("0.35"), "{halt}");
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
        let log_path = tmp
            .path()
            .join(".opencode/goals/completion_log_compl123.txt");
        assert!(log_path.exists());
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("Goal Completion Report"));
        assert!(content.contains("Test pass"));

        std::env::set_current_dir(original_cwd).unwrap();
    }
}
