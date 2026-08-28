//! Capture / demo state for marketing assets.
//!
//! Populates a deterministic, fully-populated `AppState` for screenshot and
//! GIF capture without running a real LLM or sandbox. Triggered by the
//! `--demo` flag on `niki run` / `niki chat`, or by the `NIKI_CAPTURE=1`
//! environment variable.
//!
//! The goal is a *visually accurate* snapshot: same widgets, same layout,
//! same colors, same keybindings — but populated with representative data
//! so the captured frame shows a complete, plausible session instead of
//! the empty / shell-prompt state an aborted run would produce.

use std::path::PathBuf;

use crate::artifacts::types::AgentRole;
use crate::display::components::tool_card::{ToolCard, ToolStatus};
use crate::display::state::{
    AppState, ChatLine, HoverTarget, PermissionMode, RunState, StageInfo, StageStatus,
};

/// A representative unified diff used by the Diff page capture.
const DEMO_DIFF: &str = "--- a/src/health.rs
+++ b/src/health.rs
@@ -0,0 +1,18 @@
+use axum::{routing::get, Json, Router};
+use serde::Serialize;
+
+#[derive(Serialize)]
+struct Health { status: &'static str, version: &'static str }
+
+async fn health() -> Json<Health> {
+    Json(Health { status: \"ok\", version: env!(\"CARGO_PKG_VERSION\") })
+}
+
+pub fn router() -> Router {
+    Router::new().route(\"/health\", get(health))
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    #[tokio::test]
+    async fn health_returns_ok() {
+        let v = health().await;
+        assert_eq!(v.0.status, \"ok\");
+    }
+}
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -14,4 +14,5 @@
 axum = \"0.7\"
+axum = { version = \"0.7\", features = [\"json\"] }
 serde = { version = \"1\", features = [\"derive\"] }
";

/// A representative test log for the TestLog page capture.
const DEMO_TEST_LOG: &str = "\x1b[1m\x1b[32mrunning 3 tests\x1b[0m
\x1b[1mtest health::tests::health_returns_ok ... \x1b[0m\x1b[32mok\x1b[0m
\x1b[1mtest result: ok. 1 passed; 0 failed; 0 ignored; measured; 0 filtered out\x1b[0m

\x1b[1m\x1b[32mrunning 1 test\x1b[0m
\x1b[1mtest result: ok. 1 passed; 0 failed\x1b[0m
";

/// A representative verdict / review report for the Verdict page capture.
const DEMO_REPORT: &str = "Reviewer report — niki/3eee5c1e

[PASS] src/health.rs — adds /health endpoint, well-typed, returns JSON {status, version}
[PASS] Cargo.toml — feature flag added correctly
[FAIL] src/main.rs — router not wired in main(); mount point missing
[NOTE] Consider adding a readiness probe at /ready for k8s

Verdict: 2 passed · 1 failed · 1 note
";

/// Cost JSON for the Cost page capture.
const DEMO_COST_JSON: &str = r#"{
  "total_usd": 0.0842,
  "by_agent": [
    {"role": "Planner",  "input_tokens":  1240, "output_tokens":  410, "cost_usd": 0.0091},
    {"role": "Coder",    "input_tokens":  8120, "output_tokens": 1840, "cost_usd": 0.0512},
    {"role": "Tester",   "input_tokens":   940, "output_tokens":  220, "cost_usd": 0.0048},
    {"role": "Reviewer", "input_tokens":  3200, "output_tokens":  680, "cost_usd": 0.0191}
  ]
}"#;

/// Populate a demo state suitable for screenshot / GIF capture.
pub fn apply_demo_state(state: &mut AppState, force_onboarding: bool) {
    // Description + branch
    state.description =
        "Add a /health endpoint that returns {status, version} as JSON.".to_string();
    state.branch_name = "niki/3eee5c1e".to_string();

    // Realistic run state
    state.run_state = RunState::AwaitingApproval;
    state.finished = true;
    state.start_time = Some(std::time::Instant::now() - std::time::Duration::from_secs(42));

    // Pipeline: all four stages done, with realistic metrics
    state.stages = vec![
        StageInfo {
            role: AgentRole::Planner,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript:
                "Plan: split into 1 file (health.rs), 1 Cargo.toml edit, 1 main.rs wire."
                    .to_string(),
            input_tokens: 1240,
            output_tokens: 410,
            cost_usd: 0.0091,
            latency_ms: 2_900,
            summary: vec!["Spec: 1 file to create, 1 to edit, 1 to wire.".to_string()],
            start: None,
            prompt_file: Some("planner.md".to_string()),
            retry_count: 0,
            error_message: None,
        },
        StageInfo {
            role: AgentRole::Coder,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: "Wrote src/health.rs with /health route and a unit test.".to_string(),
            input_tokens: 8120,
            output_tokens: 1840,
            cost_usd: 0.0512,
            latency_ms: 18_200,
            summary: vec!["Created src/health.rs (28 lines), edited Cargo.toml.".to_string()],
            start: None,
            prompt_file: Some("coder.md".to_string()),
            retry_count: 0,
            error_message: None,
        },
        StageInfo {
            role: AgentRole::Tester,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: "cargo test --locked — 2 tests passed.".to_string(),
            input_tokens: 940,
            output_tokens: 220,
            cost_usd: 0.0048,
            latency_ms: 6_100,
            summary: vec!["2 passed · 0 failed".to_string()],
            start: None,
            prompt_file: Some("tester.md".to_string()),
            retry_count: 0,
            error_message: None,
        },
        StageInfo {
            role: AgentRole::Reviewer,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: DEMO_REPORT.to_string(),
            input_tokens: 3200,
            output_tokens: 680,
            cost_usd: 0.0191,
            latency_ms: 11_500,
            summary: vec!["Verdict: 2 passed · 1 failed · 1 note".to_string()],
            start: None,
            prompt_file: Some("reviewer.md".to_string()),
            retry_count: 0,
            error_message: None,
        },
    ];

    // Token / cost / context aggregates
    state.input_tokens = 13_500;
    state.output_tokens = 3_150;
    state.cache_read_tokens = 4_200;
    state.cache_write_tokens = 1_100;
    state.cost = 0.0842;
    state.token_count = state.input_tokens + state.output_tokens;
    state.context_limit = 200_000;
    state.context_usage = state.token_count as f64 / state.context_limit as f64;

    // Page data
    state.diff_content = Some(DEMO_DIFF.to_string());
    state.report_content = Some(DEMO_REPORT.to_string());
    state.cost_json = Some(DEMO_COST_JSON.to_string());
    state.test_log = Some(DEMO_TEST_LOG.to_string());
    state.artifacts_dir = Some(PathBuf::from(".niki/tasks/3eee5c1e/artifacts"));

    // Tool cards: 3 in different states to show the visual treatment
    state.tool_cards = vec![
        ToolCard {
            tool_name: "Bash".to_string(),
            status: ToolStatus::Success { duration_ms: 1_240 },
            summary: "cargo test --locked".to_string(),
            output: Some(
                "   Compiling health v0.1.0\n    Finished test [unoptimized + debuginfo]\n     Running unittests src/main.rs\ntest result: ok. 2 passed; 0 failed".to_string(),
            ),
            expanded: false,
        },
        ToolCard {
            tool_name: "Edit".to_string(),
            status: ToolStatus::Success { duration_ms: 380 },
            summary: "Cargo.toml — add axum json feature".to_string(),
            output: Some("@@ -14,4 +14,5 @@\n axum = \"0.7\"\n+axum = { version = \"0.7\", features = [\"json\"] }\n serde = { version = \"1\", features = [\"derive\"] }".to_string()),
            expanded: true,
        },
        ToolCard {
            tool_name: "Read".to_string(),
            status: ToolStatus::Failed {
                error: "No such file: src/main.rs (only in worktree copy)".to_string(),
            },
            summary: "src/main.rs".to_string(),
            output: None,
            expanded: true,
        },
    ];
    state.current_tool_index = None;
    state.expanded_tools = std::collections::HashSet::from_iter([1usize, 2]);

    // Chat log: a few representative exchanges
    state.chat_log = vec![
        ("user".to_string(), "Add a /health endpoint that returns {status, version} as JSON.".to_string()),
        ("assistant".to_string(), "Plan: 1 new file, 1 Cargo.toml edit, 1 main.rs wire.\nNow writing src/health.rs and updating the dependency list.".to_string()),
        ("tool".to_string(), "Edit Cargo.toml — added axum json feature".to_string()),
        ("tool".to_string(), "Write src/health.rs — 28 lines, /health route + unit test".to_string()),
        ("tool".to_string(), "Bash cargo test — 2 passed".to_string()),
        ("assistant".to_string(), "Reviewer flagged: router not mounted in main(). Diff:".to_string()),
        ("tool".to_string(), "Bash git diff --stat — 2 files changed, 24 insertions(+)".to_string()),
    ];

    // Pre-seed a few chat lines so the chat view has something to show
    state.chat_lines = build_demo_chat_lines();
    state.chat_width.set(120);

    // Misc UI state
    state.hover_target = HoverTarget::None;
    state.permission_mode = PermissionMode::AcceptEdits;
    state.show_thinking = false;
    // Hide the onboarding modal by default. The TUI thread already may have
    // set `state.onboarding` based on the persisted `state.json` file; we
    // always clear it here unless the caller asked for `--force-onboarding`.
    state.onboarding = None;
    state.onboarded = !force_onboarding;
    if force_onboarding {
        state.onboarding = Some(crate::display::onboarding::OnboardingModal::new());
    }

    // Show a transient notice on the status bar so it's visible in the capture
    state.set_notice("Awaiting approval · press y to merge", 60_000);
}

/// Pre-rendered chat lines for the demo state.
fn build_demo_chat_lines() -> Vec<ChatLine> {
    let mut lines = Vec::new();
    let mut add = |text: &str, role: &str, msg_idx: usize| {
        for chunk in text.split('\n') {
            lines.push(ChatLine {
                text: chunk.to_string(),
                rich: None,
                msg_index: msg_idx,
                char_start: 0,
                is_input: false,
                header_stage: None,
            });
        }
        let _ = role;
    };
    add(
        "Add a /health endpoint that returns {status, version} as JSON.",
        "user",
        0,
    );
    add(
        "Plan: 1 new file (src/health.rs), 1 Cargo.toml edit, 1 main.rs wire.",
        "assistant",
        1,
    );
    add("Edit Cargo.toml — added axum json feature", "tool", 2);
    add(
        "Write src/health.rs — 28 lines, /health route + unit test",
        "tool",
        3,
    );
    add("Bash cargo test — 2 passed", "tool", 4);
    add(
        "Reviewer flagged: router not mounted in main().",
        "assistant",
        5,
    );
    add(
        "Bash git diff --stat — 2 files changed, 24 insertions(+)",
        "tool",
        6,
    );
    lines
}

/// Build a `PermissionRequest` for the permission-modal capture.
pub fn build_demo_permission_request() -> crate::display::state::PermissionRequest {
    let (tx, _rx) = std::sync::mpsc::channel();
    crate::display::state::PermissionRequest {
        tool_name: "Bash".to_string(),
        command: "sh -lc 'cargo test --locked 2>&1'".to_string(),
        description: "The agent wants to run the locked test suite before merging.".to_string(),
        params: Some(r#"{"command": "sh -lc 'cargo test --locked 2>&1'"}"#.to_string()),
        response_tx: tx,
    }
}
