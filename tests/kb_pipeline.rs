mod common;

use common::harness::TestHarness;
use common::mock_llm::{self, MockScriptBuilder};
use niki::artifacts::types::AgentRole;
use serde_json::json;

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

fn broad_spec_json() -> String {
    let files: Vec<serde_json::Value> = (0..12)
        .map(|i| {
            json!({
                "path": format!("src/module{i}.rs"),
                "action": "modify",
                "description": "broad change"
            })
        })
        .collect();
    json!({
        "summary": "Broad refactor across modules",
        "approach": "Touch many files systematically.",
        "files_to_modify": files,
        "acceptance_criteria": ["All modules updated"],
        "constraints": [],
        "estimated_complexity": "medium"
    })
    .to_string()
}

fn critic_approve_json() -> String {
    json!({
        "disposition": "approve",
        "summary": "All cited files exist and issues trace to diff evidence.",
        "unsupported_claims": [],
        "confirmed_findings": ["off-by-one fix verified in diff"]
    })
    .to_string()
}

fn critic_reject_json() -> String {
    json!({
        "disposition": "reject",
        "summary": "The verdict cites a file with no diff evidence.",
        "unsupported_claims": ["issue in src/ghost.rs has no supporting hunk"],
        "confirmed_findings": []
    })
    .to_string()
}

fn reviewer_testgap_json() -> String {
    json!({
        "verdict": "revision_needed",
        "overall_assessment": "Logic is fine but tests are missing.",
        "quality_scores": {
            "correctness": 7,
            "code_quality": 7,
            "test_coverage": 3,
            "spec_adherence": 8
        },
        "issues": [
            {
                "severity": "major",
                "category": "test_gap",
                "file_path": "src/list.rs",
                "line_range": "1-4",
                "description": "no test covers the last-page boundary",
                "suggested_fix": null
            }
        ],
        "strengths": [],
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string()
}

fn task_dir_of(harness: &TestHarness, task_id: &uuid::Uuid) -> std::path::PathBuf {
    harness
        .project_path()
        .join(".niki")
        .join("tasks")
        .join(task_id.to_string())
}

#[tokio::test]
async fn dry_run_writes_provenance_manifest() {
    let harness = TestHarness::new().with_mock_provider();
    let result = harness.run_pipeline_dry().await;
    assert_eq!(result.risk_level, "low");

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(task_dir_of(&harness, &result.task_id).join("manifest.json"))
            .expect("dry run writes manifest.json"),
    )
    .unwrap();
    assert_eq!(manifest["dry_run"], true);
    assert!(
        manifest["active_snapshot"]["snapshot_id"]
            .as_str()
            .unwrap()
            .starts_with("niki-task-")
    );
    assert!(manifest["repo_identity"]["commit_sha"].is_string());
}

#[tokio::test]
async fn full_clean_run_updates_manifest_without_learnings() {
    let mut harness = TestHarness::new()
        .with_worktree_backend()
        .with_mock_provider();
    // The mock pipeline needs only git/node/npm/python3; the default
    // extra_packages (nodejs, ...) vary by platform and are absent in CI.
    harness.config.docker.extra_packages.clear();
    let result = harness.run_pipeline().await;
    assert_eq!(format!("{:?}", result.verdict), "Approved");
    assert_eq!(format!("{:?}", result.topology), "SingleAgent");

    // Mirror run.rs completion: stamp branch (none here) + cost, then reflect.
    let task_dir = task_dir_of(&harness, &result.task_id);
    let cost: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
    niki::orchestrator::provenance::record_completion(
        &task_dir,
        &harness.project_path(),
        None,
        &result.artifacts,
        cost,
    );
    let manifest = niki::orchestrator::provenance::read_manifest(&task_dir).unwrap();
    assert!(!manifest.dry_run);
    assert!(!manifest.artifact_roles.is_empty());

    let written = niki::orchestrator::reflect::record_reflections(
        &harness.project_path(),
        harness.config(),
        &task_dir,
        &result,
    );
    assert_eq!(written, 0, "clean runs record no learnings");
}

#[tokio::test]
async fn testgap_rejection_records_verification_failure() {
    // Medium complexity forces the multi-agent chain (low would collapse to
    // the solo fast-path, which has no Reviewer to learn from).
    let mut spec: serde_json::Value = serde_json::from_str(&mock_llm::task_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("medium");
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
        )
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&reviewer_testgap_json()),
            150,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&mock_llm::review_verdict_approved_json()),
            150,
            50,
        );
    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    // The mock pipeline needs only git/node/npm/python3; the default
    // extra_packages (nodejs, ...) vary by platform and are absent in CI.
    harness.config.docker.extra_packages.clear();
    let result = harness.run_pipeline().await;
    assert_eq!(format!("{:?}", result.verdict), "Approved");

    let task_dir = task_dir_of(&harness, &result.task_id);
    let written = niki::orchestrator::reflect::record_reflections(
        &harness.project_path(),
        harness.config(),
        &task_dir,
        &result,
    );
    assert_eq!(written, 1);
    let learnings =
        std::fs::read_to_string(harness.project_path().join(".niki/learnings.jsonl")).unwrap();
    assert!(learnings.contains("verification_failure"));
    assert!(learnings.contains("last-page boundary"));
}

#[tokio::test]
async fn critic_reject_forces_one_retry_and_learning() {
    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&broad_spec_json()), 80, 120)
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
            &wrap_json(&mock_llm::review_verdict_approved_json()),
            150,
            50,
        )
        .add_response("mock-critic", &wrap_json(&critic_reject_json()), 60, 40)
        .add_response("mock-critic", &wrap_json(&critic_approve_json()), 60, 40);
    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.critic.provider = Some("mock".to_string());
    harness.config.critic.model = Some("mock-critic".to_string());
    // The mock pipeline needs only git/node/npm/python3; the default
    // extra_packages (nodejs, ...) vary by platform and are absent in CI.
    harness.config.docker.extra_packages.clear();

    let result = harness.run_pipeline().await;
    assert_eq!(result.risk_level, "normal");
    assert_eq!(format!("{:?}", result.verdict), "Approved");
    assert_eq!(
        result.revision_rounds, 1,
        "critic-forced retry bumps the round count"
    );

    let reviewers = result
        .artifacts
        .iter()
        .filter(|(r, _)| *r == AgentRole::Reviewer)
        .count();
    let critics = result
        .artifacts
        .iter()
        .filter(|(r, _)| *r == AgentRole::Critic)
        .count();
    assert_eq!(reviewers, 2, "initial review + exactly one retry");
    assert_eq!(critics, 2, "initial critique + closing critique");

    let task_dir = task_dir_of(&harness, &result.task_id);
    let written = niki::orchestrator::reflect::record_reflections(
        &harness.project_path(),
        harness.config(),
        &task_dir,
        &result,
    );
    assert_eq!(written, 1);
    let learnings =
        std::fs::read_to_string(harness.project_path().join(".niki/learnings.jsonl")).unwrap();
    assert!(learnings.contains("review_correction"));
    assert!(learnings.contains("ghost.rs"));
}
