//! MCP must not leak a process, and must not tell a model to call what it
//! cannot reach.
//!
//! Two defects, both found by asking what the code *does* rather than what it
//! documents.
//!
//! **The stdio children leaked.** `McpManager::shutdown` — the only graceful
//! teardown — has **one caller, and it is a test**. In the pipeline the manager
//! is a local inside the connect block, so it is dropped the moment the block
//! ends, and `tokio::process::Child` does not reap on drop. Every configured
//! MCP server therefore outlived the run that spawned it, and a long session
//! accumulated one process per `[mcp] server`. `kill_on_drop(true)` makes the
//! drop safe; `shutdown()` still sends the graceful `shutdown`/`exit` sequence
//! when something calls it.
//!
//! **The prompt instructed the model to do something impossible.**
//! `tools_for_prompt` ended with
//!
//! > Use these tools via the standard MCP tool call format.
//!
//! and its output went into the agent's system prompt. But
//! `McpManager::call_tool` has **no production caller** — the agent→server
//! execution loop is the follow-up the pipeline's own comment named. So a model
//! was told to use tools it had no mechanism to call, and the likeliest outcome
//! is a fabricated call and a fabricated answer: the same failure as
//! `web_search` returning `Success` with nothing in it, aimed at the model.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// **The leak.** Dropping a connection must not leave a process running.
#[test]
fn a_dropped_mcp_server_does_not_outlive_the_run() {
    let src = read("src/mcp/client.rs");
    let spawn = src
        .split(".spawn()")
        .next()
        .and_then(|r| r.rsplit("let mut child = cmd").next())
        .expect("the spawn site must exist");
    assert!(
        spawn.contains("kill_on_drop(true)"),
        "the stdio child is spawned without `kill_on_drop`, so dropping the \\
         manager leaves the server process running — `McpManager::shutdown` \\
         has no production caller, so nothing else reaps it."
    );
}

/// And the teardown that does exist is still graceful, so the backstop is a
/// backstop and not a replacement.
#[test]
fn shutdown_is_still_the_graceful_path() {
    let src = read("src/mcp/client.rs");
    let shutdown = src
        .split("pub async fn shutdown(")
        .nth(1)
        .and_then(|r| r.split("\n    }").next())
        .expect("shutdown must exist");
    assert!(
        shutdown.contains("\"shutdown\"") && shutdown.contains("\"exit\""),
        "the graceful `shutdown`/`exit` sequence must remain: `kill_on_drop` is \\
         the backstop for paths where nothing calls `shutdown`, not a \\
         replacement for it."
    );
}

/// **The lie.** Nothing may instruct a model to call an MCP tool.
#[test]
fn no_prompt_instructs_a_model_to_call_an_mcp_tool() {
    const INSTRUCTION: &str = "Use these tools via the standard MCP tool call format";
    for rel in ["src/mcp/mod.rs", "src/orchestrator/pipeline.rs"] {
        for (i, line) in read(rel).lines().enumerate() {
            // A comment may *quote* the old instruction to explain why it is
            // gone, and both files do — which is how the first version of this
            // test failed against code that was already correct.
            let code = match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            };
            assert!(
                !code.contains(INSTRUCTION),
                "{rel}:{} still tells the model to use MCP tools through a call \
                 format the runtime does not implement. \
                 `McpManager::call_tool` has no production caller, so a model \
                 that follows the instruction invents the call and the answer.",
                i + 1
            );
        }
    }
}

/// And the replacement must say the fact rather than omit it.
#[test]
fn the_mcp_summary_states_that_the_tools_are_not_callable() {
    let src = read("src/mcp/mod.rs");
    let summary = src
        .split("pub fn tools_summary(")
        .nth(1)
        .and_then(|r| r.split("\n    }").next())
        .expect("tools_summary must exist");
    assert!(
        summary.contains("NOT YET CALLABLE"),
        "the summary must state the limitation. A prompt that lists MCP tools \\
         with no qualification reads as a capability, and a model given a \\
         capability it lacks will try to use it."
    );
    assert!(
        !src.contains("pub fn tools_for_prompt("),
        "`tools_for_prompt` is the function that fed the model prompt and \\
         ended with the impossible instruction; it must not come back under \\
         another name either"
    );
}

/// The gap itself must stay *recorded*, so a future reader does not have to
/// rediscover it — and so the feature is not quietly considered done.
#[test]
fn the_missing_call_path_is_still_recorded_as_missing() {
    // `call_tool` still has no production caller. That is the feature gap, and
    // this test fails when it is closed, at which point the honest summary can
    // be replaced by real tools in the registry.
    let mut callers = 0;
    for rel in [
        "src/orchestrator/pipeline.rs",
        "src/runtime/tools.rs",
        "src/cli/chat.rs",
        "src/cli/run.rs",
        "src/display/tui.rs",
    ] {
        let src = read(rel);
        for (i, line) in src.lines().enumerate() {
            if line.contains("call_tool(") && !line.trim_start().starts_with("//") {
                callers += 1;
                eprintln!("  found a caller at {rel}:{}", i + 1);
            }
        }
    }
    assert_eq!(
        callers, 0,
        "`McpManager::call_tool` now has {callers} production caller(s), so MCP \\
         tools can be invoked. `ROADMAP.md` §9.2 can be closed and the honest \\
         'NOT YET CALLABLE' summary replaced with real registry tools — this \\
         test is the flag that says so."
    );
}
