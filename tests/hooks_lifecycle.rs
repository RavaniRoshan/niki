mod common;

use common::harness::TestHarness;

fn hook_config(harness: &mut TestHarness, event: &str, command: &str) {
    harness
        .config
        .hooks
        .commands
        .insert(event.to_string(), vec![command.to_string()]);
}

#[tokio::test]
async fn pre_task_start_block_aborts_before_planner() {
    let mut harness = TestHarness::new();
    hook_config(&mut harness, "PreTaskStart", "exit 2");
    let err = harness.run_pipeline_expect_fail().await;
    assert!(
        err.to_string().contains("hook blocked PreTaskStart"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn pre_agent_start_block_aborts_planner() {
    let mut harness = TestHarness::new().with_mock_provider();
    hook_config(&mut harness, "PreAgentStart", "exit 2");
    let err = harness.run_pipeline_expect_fail().await;
    assert!(
        err.to_string().contains("hook blocked PreAgentStart"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn post_task_stop_runs_on_completion() {
    let mut harness = TestHarness::new().with_mock_provider();
    let marker = harness.project_path().join("hook-marker.log");
    hook_config(
        &mut harness,
        "PostTaskStop",
        &format!("echo stopped >> {}", marker.display()),
    );
    let result = harness.run_pipeline_dry().await;
    assert!(result.final_diff.is_empty());
    let log = std::fs::read_to_string(&marker).expect("hook marker written");
    assert!(log.contains("stopped"), "marker content: {log}");
}

#[tokio::test]
async fn no_hooks_is_behavior_preserving() {
    // Empty [hooks] must not change a dry run's outcome.
    let harness = TestHarness::new().with_mock_provider();
    let result = harness.run_pipeline_dry().await;
    assert!(result.final_diff.is_empty());
}
