mod common;

use common::harness::TestHarness;
use common::mock_llm::{self, MockScriptBuilder};
use niki::NikiError;
use niki::artifacts::types::Verdict;
use niki::orchestrator::state::{TaskRecord, TaskStatus};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use uuid::Uuid;

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

fn medium_spec_json() -> String {
    json!({
        "summary": "Implement pagination fix",
        "approach": "Adjust boundary condition in slice helper.",
        "files_to_modify": [
            {
                "path": "src/list.rs",
                "action": "modify",
                "description": "boundary fix"
            }
        ],
        "acceptance_criteria": ["Boundary handled"],
        "constraints": [],
        "estimated_complexity": "medium"
    })
    .to_string()
}

fn reviewer_revision_needed_json() -> String {
    json!({
        "verdict": "revision_needed",
        "overall_assessment": "Logic needs further refinement.",
        "quality_scores": {
            "correctness": 5,
            "code_quality": 6,
            "test_coverage": 4,
            "spec_adherence": 6
        },
        "issues": [
            {
                "severity": "major",
                "category": "correctness",
                "file_path": "src/list.rs",
                "line_range": "1-10",
                "description": "still off-by-one under edge conditions",
                "suggested_fix": null
            }
        ],
        "strengths": [],
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string()
}

#[tokio::test]
async fn test_pipeline_cancel_guard() {
    let builder = MockScriptBuilder::new().add_response(
        "mock-planner",
        &wrap_json(&medium_spec_json()),
        80,
        120,
    );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();

    let task_id = Uuid::new_v4();
    let cancel = Arc::new(AtomicBool::new(true)); // Pre-set cancel flag

    let res = harness.run_pipeline_with_task_id(task_id, cancel).await;
    assert!(res.is_err(), "cancelled pipeline must return Err");

    let err = res.err().unwrap();
    let niki_err = err.downcast_ref::<NikiError>();
    assert!(
        matches!(niki_err, Some(NikiError::Cancelled)),
        "error must be NikiError::Cancelled, got {err:?}"
    );

    let task_dir = harness
        .project_path()
        .join(".niki")
        .join("tasks")
        .join(task_id.to_string());
    let task_json = task_dir.join("task.json");
    assert!(task_json.exists(), "task.json must be written on cancel");

    let bytes = std::fs::read(&task_json).unwrap();
    let record: TaskRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        record.status,
        TaskStatus::Cancelled,
        "task.json status must be Cancelled"
    );
}

#[tokio::test]
async fn test_always_revision_needed_stops_at_max_rounds() {
    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&medium_spec_json()), 80, 120)
        // Round 0
        .add_response(
            "mock-coder",
            &wrap_json(&mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            200,
            80,
        )
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&reviewer_revision_needed_json()),
            150,
            60,
        )
        // Round 1
        .add_response(
            "mock-coder",
            &wrap_json(&mock_llm::code_diff_json(
                "let end = start + size;",
                "let end = start + size + 1;",
                "src/list.rs",
            )),
            200,
            80,
        )
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&reviewer_revision_needed_json()),
            150,
            60,
        );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.general.max_revision_rounds = 2;

    let task_id = Uuid::new_v4();
    let cancel = Arc::new(AtomicBool::new(false));
    let res = harness
        .run_pipeline_with_task_id(task_id, cancel)
        .await
        .expect("pipeline finishes even when max rounds reached");

    assert_eq!(
        res.revision_rounds, 2,
        "must execute exactly 2 revision rounds"
    );
    assert!(
        matches!(res.verdict, Verdict::RevisionNeeded),
        "verdict must be RevisionNeeded"
    );

    let task_dir = harness
        .project_path()
        .join(".niki")
        .join("tasks")
        .join(task_id.to_string());
    let bytes = std::fs::read(task_dir.join("task.json")).unwrap();
    let record: TaskRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(record.revision_rounds, 2);
}

#[tokio::test]
async fn test_tiny_budget_step_exhaustion() {
    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&medium_spec_json()), 80, 120)
        .add_response(
            "mock-coder",
            &wrap_json(&mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            200,
            80,
        );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    // Cap max_steps at 1 (Planner consumes step 1; Coder will exceed it)
    harness.config.budget.max_steps = 1;

    let res = harness.run_pipeline_result().await;
    assert!(res.is_err(), "exceeding max_steps must abort run");
    let err_str = res.err().unwrap().to_string();
    assert!(
        err_str.contains("BudgetExhausted") || err_str.contains("budget"),
        "error must indicate budget exhaustion, got: {err_str}"
    );
}

#[tokio::test]
async fn test_parallel_coder_spend_cap_enforcement() {
    // With 2 parallel coders, each coder costs money. Set max_usd tightly.
    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&medium_spec_json()), 500, 500)
        .add_response(
            "gpt-4o",
            &wrap_json(&mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            1000,
            500,
        )
        .add_response(
            "gpt-4o",
            &wrap_json(&mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            1000,
            500,
        );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider()
        .with_parallel_enabled(2);
    harness.config.agents.coder.model = "gpt-4o".to_string();
    harness.config.docker.extra_packages.clear();
    // Hard spend cap so the parallel coders exceed the ceiling
    harness.config.general.spend_cap_usd = 0.001;

    let res = harness.run_pipeline_result().await;
    assert!(
        res.is_err(),
        "parallel coder exceeding spend cap must abort"
    );
    let err_str = res.err().unwrap().to_string();
    assert!(
        err_str.contains("SpendCapExceeded")
            || err_str.contains("BudgetExhausted")
            || err_str.contains("spend"),
        "error must mention spend cap or budget, got: {err_str}"
    );
}

#[tokio::test]
async fn test_critic_skipped_on_empty_reviewer_input() {
    // SingleAgent fast path (complexity = low) skips Reviewer and runs solo Coder.
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("low");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_response(
            "mock-coder",
            &wrap_json(&mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            200,
            80,
        );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.critic.enabled = true; // Critic enabled, but reviewer_json will be empty

    let res = harness.run_pipeline_result().await;
    assert!(
        res.is_ok(),
        "pipeline must succeed without invoking Critic on empty reviewer input: {:?}",
        res.err()
    );
}
