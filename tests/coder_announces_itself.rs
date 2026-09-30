//! The Coder must tell the user it is running, and say which step it is on.
//!
//! Every other stage reaches `display.agent_start` inside `run_agent`. The Coder
//! does not, because it does not go through `run_agent` — it runs a tool loop
//! with its own request construction, and never announced anything.
//!
//! Measured against a live provider, with `RUST_LOG=info` on:
//!
//!     15:49:40Z [Planner] Done (165s, in 1146 / out 1640) — Spec: 1 files to modify
//!     15:51:51Z [Coder] Starting...
//!
//! Two minutes and eleven seconds of nothing between those lines: no stage name,
//! no step, no spinner. And the `[Coder] Starting` that did eventually appear
//! came from the *one-shot fallback*, after the tool loop had already spent its
//! budget producing nothing — so even the one announcement was describing a
//! stage that had already lost ten minutes.
//!
//! The Coder is the longest, most expensive and most failure-prone stage in
//! the product, and the one whose silence is worst, because a run that is
//! working and a run that has hung look identical from the outside.

use std::path::Path;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// The tool loop announces the stage before it starts.
#[test]
fn the_coder_tool_loop_announces_its_stage() {
    let s = src("src/orchestrator/pipeline.rs");
    let body = s
        .split("async fn run_coder_tool_loop(")
        .nth(1)
        .expect("the tool loop exists");
    assert!(
        body.contains("display.agent_start(role)"),
        "the Coder runs a tool loop, not `run_agent`, so it never reached \
         `agent_start`. Every other stage announces itself; the longest one did not."
    );
}

/// And it reports each step, so a long stage is not opaque.
#[test]
fn the_coder_reports_each_step() {
    let s = src("src/runtime/tools.rs");
    let body = s
        .split("pub async fn run_tool_loop_with(")
        .nth(1)
        .expect("the tool loop runner exists");
    assert!(
        body.contains("StageToken"),
        "a stage that can spend minutes inside one call must say which step it is on, \\
         or a working run and a hung run are indistinguishable"
    );
}

/// The one-shot path still announces, or the fix traded one silence for another.
#[test]
fn the_one_shot_path_still_announces() {
    let s = src("src/agents/mod.rs");
    assert!(
        s.contains("display.agent_start(role)"),
        "the one-shot stage path must keep announcing; the Coder was missing it, not \
         replacing it"
    );
}
