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
        // Names nothing and tells the Coder nothing: a model that declines to
        // approve without being able to say why, which is what a small model
        // does readily.
        "issues": [],
        "strengths": [],
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

/// A patch that validates but will not apply is a fixable mistake.
///
/// The artifact was schema-valid and its `search` was not text in the file.
/// The old behaviour ended the run there — after the plan, the code, and a
/// Tester pass had already been paid for. Measured on a refactor task.
///
/// The guard that refused is right and stays: a Tester must never verify a
/// tree that does not contain the change, so the round is abandoned rather
/// than continued. What changes is that the model is told, and asked again.
#[tokio::test]
async fn an_unappliable_patch_asks_the_coder_again_instead_of_ending_the_run() {
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("high");

    // Round 0: a `search` that exists nowhere in the fixture repo.
    let unappliable: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "this text is not in the file and never was",
        "fn total() {}",
        "src/list.rs",
    ))
    .expect("diff json");
    // Round 1: the fix.
    let appliable: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", unappliable)
        .add_tool_call("mock-coder", "submit_artifact", appliable)
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
    harness.config.general.max_revision_rounds = 3;

    let res = harness
        .run_pipeline_result()
        .await
        .expect("an unappliable patch is a fixable mistake, not the end of the run");

    assert!(
        !res.final_diff.is_empty(),
        "the run must end with a real diff, not a silently empty one"
    );
    let coder_calls = res
        .metrics
        .iter()
        .filter(|m| m.role == niki::artifacts::types::AgentRole::Coder)
        .count();
    assert_eq!(
        coder_calls, 2,
        "the Coder must be asked again, exactly once"
    );
    // The Tester must only ever have run against the tree that has the change.
    let tester_calls = res
        .metrics
        .iter()
        .filter(|m| m.role == niki::artifacts::types::AgentRole::Tester)
        .count();
    assert_eq!(
        tester_calls, 1,
        "the Tester must not run against a tree the change never reached"
    );
}

/// An unappliable patch has its own allowance and does not spend the
/// Reviewer's.
///
/// It is a mechanical error — a `search` that is not in the file — not a
/// judgment about quality, so charging it to the revision budget spends a
/// round the Reviewer asked for on something it never asked about.
///
/// The scenario is the one that separates them. `max_revision_rounds = 1`
/// means the Reviewer gets exactly one round of quality feedback. A Coder
/// patch that would not apply comes first and is sent back; if that had
/// charged the budget, the loop would be over before the Reviewer ever ran.
/// It does not, so the Reviewer's round happens and the run closes.
#[tokio::test]
async fn a_patch_that_will_not_apply_does_not_spend_the_reviewers_budget() {
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("high");

    let unappliable: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "this text is not in the file and never was",
        "fn total() {}",
        "src/list.rs",
    ))
    .expect("diff json");
    let appliable: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", unappliable)
        .add_tool_call("mock-coder", "submit_artifact", appliable)
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
    // One quality round. A mechanical failure must not use it up.
    harness.config.general.max_revision_rounds = 1;

    let res = harness
        .run_pipeline_result()
        .await
        .expect("a patch that would not apply must not end the run");

    assert_eq!(
        res.verdict,
        Verdict::Approved,
        "the Reviewer's own round still had to happen, and its verdict still counts"
    );
    let reviewer_calls = res
        .metrics
        .iter()
        .filter(|m| m.role == niki::artifacts::types::AgentRole::Reviewer)
        .count();
    assert_eq!(
        reviewer_calls, 1,
        "the Reviewer must have run: its budget was not spent on the Coder's typo"
    );
}

/// A Reviewer that names the problem but leaves `feedback` empty still gets a
/// round.
///
/// The Coder only ever reads `feedback`, so a verdict of `revision_needed`
/// whose critique lives in `issues` was discarding the only description of the
/// problem. The run then stopped — correctly, for having nothing to act on —
/// with a named defect it would never address. Measured: the knowledge-base
/// testgap run, which ends `RevisionNeeded` while naming a missing test.
///
/// The brief is bridged from the critical and major issues, so the information
/// the Reviewer did supply reaches the Coder. Nits are not carried over: a
/// critique of nits is noise, and it is how a round becomes unable to change
/// anything.
#[tokio::test]
async fn a_reviewer_who_names_the_problem_in_issues_gets_a_round() {
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("high");

    let names_the_problem: String = json!({
        "verdict": "revision_needed",
        "overall_assessment": "Logic is fine but a test is missing.",
        "quality_scores": {
            "correctness": 7, "code_quality": 7, "test_coverage": 3, "spec_adherence": 8
        },
        "issues": [
            {
                "severity": "major",
                "category": "test_gap",
                "file_path": "src/list.rs",
                "line_range": "1-4",
                "description": "no test covers the last-page boundary",
                "suggested_fix": null
            },
            {
                "severity": "nit",
                "category": "style",
                "file_path": null,
                "line_range": null,
                "description": "naming could be tighter",
                "suggested_fix": null
            }
        ],
        "strengths": [],
        "feedback": null,
        "red_reconciliation": null
    })
    .to_string();

    let first: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/list.rs",
    ))
    .expect("diff json");
    let second: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size;",
        "let end = start + size + 1;",
        "src/list.rs",
    ))
    .expect("diff json");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_tool_call("mock-coder", "submit_artifact", first)
        .add_response(
            "mock-tester",
            &wrap_json(&mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response("mock-reviewer", &wrap_json(&names_the_problem), 150, 60)
        .add_tool_call("mock-coder", "submit_artifact", second)
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
    harness.config.general.max_revision_rounds = 3;

    let res = harness
        .run_pipeline_result()
        .await
        .expect("a named problem must be actionable, not a dead end");

    assert_eq!(
        res.verdict,
        Verdict::Approved,
        "the second round must be able to close the finding"
    );
    assert_eq!(
        res.revision_rounds, 1,
        "exactly one revision round: the named problem was worth one"
    );
}

/// A change to a file the specification never named is shown to the Reviewer.
///
/// Measured: a task about `src/broken.rs` produced a Coder artifact that
/// created `src/search.rs` — a file the task never mentioned and the Coder
/// invented — and the run carried on silently, because the artifact schema has
/// no opinion on scope.
///
/// Not an error: touching an adjacent file is often right, and failing the run
/// for it would be worse than saying so. What is not acceptable is the
/// Reviewer approving a diff whose scope it was never shown.
#[test]
fn the_reviewer_is_told_which_files_the_task_never_asked_for() {
    use niki::artifacts::types::TaskSpec;
    use niki::orchestrator::pipeline::scope_drift_note;

    let spec: TaskSpec = serde_json::from_str(&mock_llm::task_spec_json()).expect("spec parses");
    let planned: Vec<&str> = spec
        .files_to_modify
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert!(!planned.is_empty(), "the fixture must plan a file");

    // The measured case: a Coder that created a file the task never mentioned.
    let drifted: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        "src/search.rs",
    ))
    .expect("diff json");
    let note =
        scope_drift_note(&spec, &drifted.to_string()).expect("an unplanned file must be reported");
    assert!(
        note.contains("src/search.rs"),
        "the note must name the file: {note}"
    );
    assert!(
        planned.iter().all(|p| note.contains(p)),
        "and the planned files, or the Reviewer cannot judge the difference: {note}"
    );

    // A change inside the plan says nothing.
    let in_scope: serde_json::Value = serde_json::from_str(&mock_llm::code_diff_json(
        "let end = start + size - 1;",
        "let end = start + size;",
        planned[0],
    ))
    .expect("diff json");
    assert_eq!(
        scope_drift_note(&spec, &in_scope.to_string()),
        None,
        "a change the task asked for is not drift, and saying so on every run would be noise"
    );

    // No spec, no Coder, nothing to compare — silence in all three.
    assert_eq!(
        scope_drift_note(&spec, ""),
        None,
        "no Coder artifact, no note"
    );
}

/// A configured `reasoning_effort` must reach the provider through a real run.
///
/// This is the join the per-hop tests could not see. `reasoning_effort` is
/// configured on an agent, carried on a resolved stage, and read back out of a
/// `CompletionRequest` — and a test that checks the first two cannot see the
/// third. It was compiler-checked and nothing more, which is stated in the
/// commit that added the knob; this is the test that makes the statement
/// true.
///
/// It matters because the value is a dial a user pays for. A configuration
/// that parses and then does nothing is a promise the program does not keep,
/// and the user finds out on their bill.
#[tokio::test(flavor = "multi_thread")]
async fn a_configured_reasoning_effort_reaches_the_provider_on_a_real_run() {
    let mut spec: serde_json::Value = serde_json::from_str(&medium_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("high");

    let builder = MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&spec.to_string()), 80, 120)
        .add_tool_call(
            "mock-coder",
            "submit_artifact",
            serde_json::from_str(&mock_llm::code_diff_json(
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
            &wrap_json(&mock_llm::review_verdict_approved_json()),
            150,
            60,
        );

    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    harness.config.agents.coder.reasoning_effort = Some("high".into());
    // The other roles deliberately leave it unset, so this distinguishes
    // "the Coder's value arrived" from "everything has it".

    niki::llm::mock::record_requests();
    let res = harness
        .run_pipeline_result()
        .await
        .expect("the run completes");
    let sent = niki::llm::mock::take_recorded();

    assert!(!sent.is_empty(), "the mock was asked for something");
    let with_effort: Vec<&str> = sent
        .iter()
        .filter_map(|r| r.reasoning_effort.as_deref())
        .collect();
    assert!(
        with_effort.contains(&"high"),
        "the configured value must be on the wire; recorded efforts: {with_effort:?}"
    );
    assert!(
        sent.iter()
            .any(|r| r.model == "mock-coder" && r.reasoning_effort.as_deref() == Some("high")),
        "and it must be the Coder's, not another stage's: {}",
        sent.iter()
            .map(|r| format!("{}={:?}", r.model, r.reasoning_effort))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert_eq!(
        sent.iter().filter(|r| r.reasoning_effort.is_none()).count(),
        sent.iter()
            .filter(|r| r.model != "mock-coder" && !r.reasoning_effort.is_some())
            .count(),
        "every non-Coder request must leave it unset"
    );
    let _ = res;
}
