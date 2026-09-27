mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use common::harness::TestHarness;
use common::mock_llm::{self, MockScriptBuilder};

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

/// Medium complexity so `Auto` keeps the full multi-agent chain rather than
/// collapsing to the solo fast path.
fn medium_spec() -> String {
    let mut spec: serde_json::Value = serde_json::from_str(&mock_llm::task_spec_json()).unwrap();
    spec["estimated_complexity"] = serde_json::json!("medium");
    spec.to_string()
}

fn multi_agent_script() -> MockScriptBuilder {
    MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&medium_spec()), 80, 120)
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
}

/// Flip `flag` once the run has recorded at least `n` completed stages.
///
/// Polling the run's own `task.json` rather than sleeping makes the trigger
/// deterministic.
///
/// `n` must be 2 for the mid-round test. At `n = 1` the flag lands right after
/// the Planner, and the pre-existing check at the top of the revision loop
/// catches it there — the run stops with one stage recorded for the *wrong*
/// reason, and the test passes whether or not the per-stage check exists. Two
/// stages puts the flip strictly inside a round, past the loop-top check, where
/// only a per-stage check can reach it.
fn cancel_after_n_stages(tasks_dir: std::path::PathBuf, n: usize, flag: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
                for e in entries.flatten() {
                    let tj = e.path().join("task.json");
                    if let Ok(text) = std::fs::read_to_string(&tj)
                        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
                        && v.get("agent_metrics")
                            .and_then(|m| m.as_array())
                            .is_some_and(|a| a.len() >= n)
                    {
                        flag.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
}

fn recorded_stage_count(tasks_dir: &std::path::Path) -> usize {
    std::fs::read_dir(tasks_dir)
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("task.json")).ok())
        .filter_map(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .filter_map(|v| {
            v.get("agent_metrics")
                .and_then(|m| m.as_array())
                .map(|a| a.len())
        })
        .next()
        .unwrap_or(0)
}

/// Cancelling mid-round must stop the run at the *next stage boundary*, not at
/// the end of the round.
///
/// A multi-agent round is Tester → Red → Reviewer → SecurityAuditor: four
/// sequential LLM calls. The flag used to be read only at the top of the round
/// loop, so a user who pressed Esc sat through the entire remainder of the
/// round before anything happened. On a slow model that is minutes of apparent
/// unresponsiveness, and the natural conclusion is that the key does not work.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_mid_round_stops_at_the_next_stage_boundary() {
    let builder = multi_agent_script();
    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();

    let tasks_dir = harness.project_path().join(".niki").join("tasks");
    let cancel = Arc::new(AtomicBool::new(false));
    cancel_after_n_stages(tasks_dir.clone(), 2, cancel.clone());

    let result = harness.run_pipeline_with_cancel(cancel).await;

    eprintln!(
        "PROBE result.is_err={} stages={}",
        result.is_err(),
        recorded_stage_count(&tasks_dir)
    );
    let err = result.expect_err("a cancelled run must not return a PipelineResult");
    assert!(
        format!("{err:#}").contains("ancel"),
        "the error must say the run was cancelled: {err:#}"
    );

    let stages = recorded_stage_count(&tasks_dir);
    // Two stages had finished when the flag was set, so the next stage must not
    // start. The full round records four (Planner + Tester + Reviewer + ...).
    assert_eq!(
        stages, 2,
        "cancellation must land at the next stage boundary; the run recorded \
         {stages} stages, so it ran the rest of the round anyway"
    );
}

/// The solo fast path has no revision loop, so it previously had no place
/// where the flag was ever read — pressing Esc did nothing at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_solo_run_never_starts_the_coder() {
    let builder = multi_agent_script();
    let mut harness = TestHarness::new()
        .with_mock_builder(|_| builder)
        .with_worktree_backend()
        .with_mock_provider();
    harness.config.docker.extra_packages.clear();
    // Force the fast path. The default spec is medium complexity, which `Auto`
    // expands to the full chain — so without this the test would silently
    // exercise the multi-agent round-top check instead of the solo arm, and
    // pass whether or not the solo arm checked anything.
    harness.config.pipeline.topology = niki::config::types::TopologyMode::SingleAgent;

    let tasks_dir = harness.project_path().join(".niki").join("tasks");
    // Flag already set: the run must bail before spending a Coder session.
    let cancel = Arc::new(AtomicBool::new(true));

    let result = harness.run_pipeline_with_cancel(cancel).await;
    eprintln!(
        "PROBE solo is_err={} err={:?}",
        result.is_err(),
        result.as_ref().err().map(|e| format!("{e:#}"))
    );
    assert!(result.is_err(), "a cancelled run must not return a result");

    let stages = recorded_stage_count(&tasks_dir);
    assert_eq!(
        stages, 1,
        "only the Planner should have run; {stages} stages were recorded"
    );
}
