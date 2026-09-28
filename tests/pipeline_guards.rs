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

/// A reviewer that asks for a revision **and says what to fix**.
///
/// `issue` differs per round on purpose. A round that comes back with the
/// identical critique is stopped deliberately — the harness refuses to pay for
/// a revision round that has nothing new to act on — so scripting the same
/// issue twice would test the stop, not the round limit this test is named for.
fn reviewer_revision_needed_json(issue: &str) -> String {
    let issue_item = json!({
        "severity": "major",
        "category": "correctness",
        "file_path": "src/list.rs",
        "line_range": "1-10",
        "description": issue,
        "suggested_fix": null
    });
    json!({
        "verdict": "revision_needed",
        "overall_assessment": "Logic needs further refinement.",
        "quality_scores": {
            "correctness": 5,
            "code_quality": 6,
            "test_coverage": 4,
            "spec_adherence": 6
        },
        "issues": [issue_item],
        "strengths": [],
        "feedback": {
            "critical_issues": [issue_item],
            "guidance": "Fix it before resubmitting.",
            "keep_unchanged": [],
            "revision_round": 0
        },
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
        .add_tool_call(
            "mock-coder",
            "submit_artifact",
            serde_json::from_str::<serde_json::Value>(&mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            ))
            .expect("diff json"),
        )
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&reviewer_revision_needed_json("round one")),
            150,
            60,
        )
        // Round 1
        .add_tool_call(
            "mock-coder",
            "submit_artifact",
            serde_json::from_str::<serde_json::Value>(&mock_llm::code_diff_json(
                "let end = start + size;",
                "let end = start + size + 1;",
                "src/list.rs",
            ))
            .expect("diff json"),
        )
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&reviewer_revision_needed_json("round two")),
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
    // The SingleAgent fast path runs a solo Coder with no Reviewer, so
    // `reviewer_json` is empty and the Critic must be skipped rather than
    // handed nothing.
    //
    // The topology is set explicitly rather than left to `auto`. This test was
    // written when `auto` collapsed a low-complexity task to a single agent;
    // the heuristic now treats model capability as an input and an *unmeasured*
    // model gets the full pipeline ("the safer side of the trade"). So the
    // test stopped reaching the path it is named after and went red — it was
    // already failing before any of this work, on this branch.
    //
    // Asking for SingleAgent directly states the intent, and stops the test
    // from silently changing meaning whenever the heuristic does.
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("low");

    // The Coder's scripted answer is a `submit_artifact` call, because the
    // Coder stage runs on the tool loop. A prose response would make the loop
    // fall back to the one-shot path, which consumes a *second* scripted
    // response and shifts everything after it by one — which is how this test
    // started failing for a reason that had nothing to do with the Critic.
    let diff: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", diff);

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.critic.enabled = true; // Critic enabled, but reviewer_json will be empty
    harness.config.pipeline.topology = niki::config::TopologyMode::SingleAgent;

    let res = harness.run_pipeline_result().await;
    assert!(
        res.is_ok(),
        "pipeline must succeed without invoking Critic on empty reviewer input: {:?}",
        res.err()
    );
}

/// The Coder really runs on the tool loop.
///
/// This was a source-text assertion — `src.contains("let json = if role ==
/// AgentRole::Coder {")` — for the stated reason that "a stage's execution is
/// not observable from a unit test". It is observable. `MockProvider` simply
/// could not return a tool call, so the loop was unreachable from any test.
///
/// Now it can. The mock Coder answers with a `submit_artifact` tool call and
/// no text. On the one-shot path that is not a valid artifact and the stage
/// fails; on the loop path it is the loop's exit. So a run that produces the
/// diff proves which path ran, from the outside, with no reading of the
/// pipeline's source.
#[tokio::test]
async fn the_coder_stage_answers_by_calling_submit_artifact() {
    let diff: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&medium_spec_json()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", diff)
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
            60,
        );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();

    let res = harness
        .run_pipeline_result()
        .await
        .expect("a Coder that answers with a submit_artifact call must complete the stage");

    let coder_json = res
        .artifacts
        .iter()
        .find(|(role, _)| *role == niki::artifacts::types::AgentRole::Coder)
        .map(|(_, j)| j.clone())
        .expect("the Coder produced an artifact");
    let parsed: serde_json::Value =
        serde_json::from_str(&coder_json).expect("the artifact is JSON, not prose");
    assert_eq!(
        parsed["edits"][0]["replace"], "let end = start + size;",
        "the artifact must be the one the tool call carried"
    );
}

/// A Reviewer that asks for a revision while saying nothing about what to fix
/// does not get another round.
///
/// This is the failure the gate exists for, measured on a live run: a 3B
/// Reviewer returned `revision_needed` with an empty `critical_issues` list —
/// it declined to approve without naming a blocker. The Coder was handed that,
/// could not act on it, and its next output failed to parse, taking down a
/// task whose tests had already passed 5/5.
///
/// Asserted at the pipeline level rather than on the helper, so it covers the
/// wiring: the Coder is called once, the Tester once, the Reviewer once, and
/// the run still produces a branch. The verdict stays `RevisionNeeded` —
/// stopping early must not turn into quietly calling the work good.
#[tokio::test]
async fn a_reviewer_that_asks_for_revision_without_saying_why_gets_no_second_round() {
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("high");

    let silent_review: String = json!({
        "verdict": "revision_needed",
        "overall_assessment": "Not quite right, somehow.",
        "quality_scores": {
            "correctness": 5,
            "code_quality": 6,
            "test_coverage": 4,
            "spec_adherence": 6
        },
        "issues": [{
            "severity": "major",
            "category": "correctness",
            "file_path": "src/list.rs",
            "line_range": "1-10",
            "description": "still off-by-one under edge conditions",
            "suggested_fix": null
        }],
        "strengths": [],
        // Says "revise" and then tells the Coder nothing.
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string();

    let diff: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", diff)
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response("mock-reviewer", &wrap_json(&silent_review), 150, 60);

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.general.max_revision_rounds = 3;

    let res = harness
        .run_pipeline_result()
        .await
        .expect("a silent revision must not fail the run — the branch is still produced");

    assert_eq!(
        res.revision_rounds, 0,
        "no round was worth running: the Reviewer named no actionable issue"
    );
    assert_eq!(
        res.verdict,
        Verdict::RevisionNeeded,
        "stopping early must not quietly turn the run into an approval"
    );
    let coder_calls = res
        .metrics
        .iter()
        .filter(|m| m.role == niki::artifacts::types::AgentRole::Coder)
        .count();
    assert_eq!(
        coder_calls, 1,
        "the Coder must be asked once, not three times"
    );
}

/// The single-agent fast path gets the tool loop too.
///
/// It did not. The loop only existed on the multi-agent path, so the cheaper
/// topology was the one that could not read a file, and the expensive one
/// could — backwards, and against the evidence this project's own architecture
/// notes record: a single agent with a tool loop is the better default, and a
/// single agent without one is what fails on a weak model.
///
/// `solo.md` had no tool protocol either, so pointing the loop at it without
/// teaching the prompt would have produced a loop the model answers in prose.
#[tokio::test]
async fn the_single_agent_fast_path_runs_on_the_tool_loop() {
    let diff: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&medium_spec_json()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", diff);

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.pipeline.topology = niki::config::TopologyMode::SingleAgent;

    let res = harness
        .run_pipeline_result()
        .await
        .expect("a fast-path Coder that submits through the tool must complete");

    let coder_json = res
        .artifacts
        .iter()
        .find(|(role, _)| *role == niki::artifacts::types::AgentRole::Coder)
        .map(|(_, j)| j.clone())
        .expect("the solo Coder produced an artifact");
    let parsed: serde_json::Value = serde_json::from_str(&coder_json).expect("artifact is JSON");
    assert_eq!(
        parsed["edits"][0]["replace"], "let end = start + size;",
        "the artifact must be the one the tool call carried — a prose fallback would not \\
         be this"
    );
}

/// Both prompts have to describe the protocol they are actually given.
#[test]
fn the_solo_prompt_also_describes_the_tool_loop() {
    let template = niki::load_asset("prompts/solo.md").expect("solo.md is embedded");
    let mut env = minijinja::Environment::new();
    env.add_template("solo", &template).expect("parses");

    let render = |tool_loop: bool| {
        env.get_template("solo")
            .expect("template")
            .render(minijinja::context! {
                task_description => "fix the off-by-one",
                project_knowledge => "",
                project_memory => "",
                current_files => "src/list.rs",
                artifact_schema => "{}",
                tool_loop => tool_loop,
            })
            .expect("renders")
    };

    let looped = render(true);
    assert!(
        !looped.contains("Respond with ONLY the raw JSON artifact"),
        "the fast path talks to a tool loop; telling the model to reply with JSON is what \\
         made it produce prose the loop could not accept"
    );
    assert!(
        looped.contains("submit_artifact"),
        "it must name the exit tool"
    );

    let one_shot = render(false);
    assert!(
        one_shot.contains("Respond with ONLY the raw JSON artifact"),
        "the fallback path has no tools, so raw JSON is the right instruction there"
    );
}
