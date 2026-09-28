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
    // The topology is requested rather than inferred. This test is about the
    // solo fast path — it runs no Reviewer, so it cannot report an
    // independently-reviewed approval — and `auto` no longer collapses a
    // low-complexity task to a single agent: an unmeasured model is assumed
    // weak and gets the full chain, which is the safer side of the trade.
    // Asserting on `auto` therefore tested a topology heuristic, not the solo
    // path, and went red when the heuristic changed.
    harness.config.pipeline.topology = niki::config::TopologyMode::SingleAgent;
    let result = harness.run_pipeline().await;
    assert_eq!(format!("{:?}", result.topology), "SingleAgent");
    assert!(
        !result.outcome.is_independently_reviewed(),
        "the solo fast path has no independent reviewer: {:?}",
        result.outcome
    );
    assert!(
        !result.outcome.is_approved(),
        "a self-verified run must not report a bare approval: {:?}",
        result.outcome
    );

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
        // Round 1 needs a patch that can actually apply. The mock cycles its
        // responses, so reusing the round-0 diff meant the Reviewer's revision
        // request re-emitted a SEARCH block for text the round-0 patch had
        // already replaced. The apply failed, and the failure was only a
        // warning on stderr — so the test asserted a clean Approved verdict on
        // a second round that had changed nothing at all.
        .add_response(
            "mock-coder",
            &wrap_json(&mock_llm::code_diff_json(
                "    &items[start..end]",
                "    &items[start..end.min(items.len())]",
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

#[tokio::test]
async fn budget_exhaustion_stops_tiny_run() {
    // Phase 5.5 acceptance: a fixture configured with a tiny budget stops
    // with a typed BudgetExhausted error (cli maps it to task.json Failed —
    // see `cli/run.rs` error arm), regardless of which mechanism would
    // otherwise retry.
    let mut harness = TestHarness::new()
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.budget.max_steps = 1;
    let err = harness.run_pipeline_expect_fail().await;
    let msg = err.to_string();
    assert!(
        msg.contains("budget exhausted"),
        "tiny budget must stop the run, got: {msg}"
    );
    assert!(msg.contains("steps"), "dimension must be named: {msg}");
}

#[tokio::test]
async fn auto_high_risk_yields_multiagent_with_security_auditor() {
    // Phase 5.7 acceptance: Auto + High tier forces the full multi-agent
    // chain and the run produces a SecurityAuditor artifact (the SingleAgent
    // fast-path would have collapsed it away).
    let mut harness = TestHarness::new()
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.risk.mode = niki::config::types::RiskMode::High;
    // A *measured, capable* model, so the low-complexity task would otherwise
    // take the solo fast-path and the risk tier would be the only thing that
    // could turn it back.
    //
    // Without this the run is MultiAgent anyway — an unmeasured model is
    // assumed weak and gets the full chain — so the override never fires, the
    // reason says "capability not measured", and the assertion below failed
    // against a run that satisfied everything it is actually named for. The
    // test was not testing the risk override; it was red.
    // The capability is read from `config.project_dir`, which the harness
    // leaves empty, so the measurement has to be seeded where the run looks.
    harness.config.project_dir = harness.project_path();
    niki::config::capability::save(
        &harness.config.project_dir_hint(),
        niki::config::capability::ModelCapability::Measured {
            passed: 4,
            total: 4,
        },
    )
    .expect("seed a capability measurement");
    // The risk-added stages need mock answers: the Critic reuses the
    // reviewer's model binding (appended as its second response), the
    // SecurityAuditor has its own model.
    let script_text =
        std::fs::read_to_string(&harness.mock_script_path).expect("happy-path script exists");
    let mut script: serde_json::Value = serde_json::from_str(&script_text).unwrap();
    script["models"]["mock-reviewer"]["responses"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "text": wrap_json(&critic_approve_json()),
            "input_tokens": 150,
            "output_tokens": 50,
        }));
    script["models"]["mock-security_auditor"] = serde_json::json!({"responses": [{
        "text": wrap_json(&mock_llm::security_verdict_json()),
        "input_tokens": 100,
        "output_tokens": 40,
    }]});
    std::fs::write(
        &harness.mock_script_path,
        serde_json::to_string_pretty(&script).unwrap(),
    )
    .unwrap();

    let result = harness.run_pipeline().await;
    assert_eq!(format!("{:?}", result.topology), "MultiAgent");
    assert!(
        result
            .artifacts
            .iter()
            .any(|(r, _)| *r == AgentRole::SecurityAuditor),
        "High-risk Auto run must produce a SecurityAuditor artifact"
    );
    assert!(
        result.topology_reason.contains("risk override"),
        "{}",
        result.topology_reason
    );
}

/// A patch that cannot apply must stop the run, not decorate it with a warning.
///
/// This is the whole shape of the defect: a Coder emits a diff whose SEARCH
/// block matches nothing, `apply_patch` fails, the failure is printed to
/// stderr and the run carries on to produce a verdict — about a working tree
/// that does not contain any of the change. Every downstream signal then reads
/// as a success: a branch is cut, the report claims a fix, the JSON envelope
/// says `completed`. The only evidence of the truth is a line in a log the
/// user may never read.
#[tokio::test]
async fn a_patch_that_cannot_apply_stops_the_run() {
    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&broad_spec_json()), 80, 120)
        .add_response(
            "mock-coder",
            // SEARCH text that exists nowhere in the fixture repo.
            &wrap_json(&mock_llm::code_diff_json(
                "this text is not present in any file in this repository",
                "replacement",
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

    let err = harness
        .run_pipeline_result()
        .await
        .expect_err("a patch that never applied must not report a completed run");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("did not apply"),
        "the failure must say what actually went wrong, got: {msg}"
    );
}

/// The solo fast path's bounded repair is the only recovery from a failed
/// apply, and it is bounded on purpose. When the repair fails too, the run
/// must stop — it used to fall through to `verdict = Approved` and hand back a
/// branch containing no change at all, reported as a success.
#[tokio::test]
async fn a_solo_repair_that_also_fails_stops_the_run() {
    let impossible = mock_llm::code_diff_json(
        "this text is not present in any file in this repository",
        "replacement",
        "src/list.rs",
    );
    let builder = MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &wrap_json(&mock_llm::task_spec_json()),
            100,
            100,
        )
        .add_response("mock-coder", &wrap_json(&impossible), 200, 100);
    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();

    let err = harness
        .run_pipeline_result()
        .await
        .expect_err("a run whose change was never written must not report success");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("did not apply") || msg.contains("not a valid code diff"),
        "the failure must name the real cause, got: {msg}"
    );
}
