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

/// And the summary must say the fact — which changed in batch 7.
///
/// This asserted `NOT YET CALLABLE` for two batches, and it was right: a
/// summary listing MCP tools with no qualification reads as a capability, and
/// a model given a capability it lacks will try to use it. The error was never
/// the qualification, it was the *direction*.
///
/// The tools are callable now, so a summary still saying NOT YET CALLABLE is
/// the same lie pointing the other way: a user reads it, configures a server,
/// watches the agent ignore it, and concludes the feature is broken. This test
/// follows the behaviour, and its failure names both directions.
#[test]
fn the_mcp_summary_says_what_the_model_can_actually_call() {
    let src = read("src/mcp/mod.rs");
    let summary = src
        .split("pub fn tools_summary(")
        .nth(1)
        .and_then(|r| r.split("\n    }").next())
        .expect("tools_summary must exist");
    assert!(
        !summary.contains("NOT YET CALLABLE"),
        "the tools ARE callable now, and a notice that says otherwise sends a \
         user looking for a fault that is not there: {summary}"
    );
    assert!(
        summary.contains("callable"),
        "and the summary must say what is true — that these tools reach the \
         agent loop: {summary}"
    );
    assert!(
        !src.contains("pub fn tools_for_prompt("),
        "`tools_for_prompt` is the function that fed the model prompt and \\
         ended with the impossible instruction; it must not come back under \\
         another name either"
    );
}

/// The flag that watched for this to close. It **fired** in batch 7, which is
/// the mechanism working: it existed so that closing the gap would be a
/// visible event rather than a quiet one.
///
/// Its replacement pins the *other* side, because a feature that is wired can
/// also be unwired without anybody noticing: if the registration goes away,
/// the summary would still say "callable" and the model would have no such
/// tool — the same failure as a summary that was right once and then drifted.
#[test]
fn the_call_path_is_registered_not_merely_present() {
    let mut callers = 0;
    for rel in ["src/runtime/mcp_tool.rs", "src/orchestrator/pipeline.rs"] {
        let src = read(rel);
        for (i, line) in src.lines().enumerate() {
            if line.contains("call_tool(") && !line.trim_start().starts_with("//") {
                callers += 1;
                eprintln!("  found a caller at {rel}:{}", i + 1);
            }
        }
    }
    assert!(
        callers > 0,
        "nothing calls `McpManager::call_tool` again, so no MCP tool can be \
         invoked — and the summary would still tell the user they can be. \
         This is the inverse of the flag that fired when the gap closed."
    );
}
