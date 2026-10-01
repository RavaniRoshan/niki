//! The agent→server call path, against a real MCP server.
//!
//! This is §9.2, the last open item in `ROADMAP.md` that is not a decision.
//! It was recorded as a *feature* rather than a repair, and the reason was
//! concrete: the manager was a local of the discovery block, so the stdio
//! children were killed — by `kill_on_drop(true)` — the moment that block
//! ended. Every server was dead before the Planner's first token, and
//! `McpManager::call_tool` had nothing left to reach. It was not merely
//! unreachable; it was pointing at corpses.
//!
//! So the first slice is the manager living for the run, and the proof is a
//! round trip. `tests/integration/mcp_server_fixture.py` is a real MCP server —
//! newline-delimited JSON-RPC 2.0 over stdio, the protocol
//! `src/mcp/client.rs` speaks. It is scripted because it is an external
//! boundary, and it is a *process* rather than a mock, so the handshake, the
//! framing, the routing by `id` and the error path are all real.

use std::path::PathBuf;

use niki::mcp::{McpManager, McpServerConfig, McpServerType};

/// The fixture's path. It has to exist, and it has to be a `.py` that runs —
/// a fixture that cannot start proves nothing about anything else here.
fn fixture() -> PathBuf {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/integration/mcp_server_fixture.py");
    assert!(
        p.exists(),
        "the MCP fixture must exist at {}: every test here is a conversation \
         with it, and a missing file turns them into assertions about nothing",
        p.display()
    );
    p
}

fn manager(name: &str, env: &[(&str, &str)]) -> McpManager {
    let mut mgr = McpManager::new();
    let mut vars = std::collections::HashMap::new();
    for (k, v) in env {
        vars.insert((*k).to_string(), (*v).to_string());
    }
    mgr.add_server(McpServerConfig {
        name: name.to_string(),
        server_type: McpServerType::Local {
            command: "python3".to_string(),
            args: vec![fixture().display().to_string()],
            env: vars,
        },
        enabled: true,
        timeout_ms: 10_000,
    });
    mgr
}

/// The whole point: discover, then call, over a live process.
///
/// The assert is on the **server's own words** coming back. A test that
/// checked the call returned `Ok` would pass against a client that answered
/// itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discovered_mcp_tool_can_actually_be_called() {
    let mut mgr = manager("fixture", &[]);
    mgr.connect_all().await.expect("the fixture must start");
    assert_eq!(
        mgr.connected_servers(),
        1,
        "a configured, enabled, trusted server must end up with a live \
         connection — if this is 0 the call below would fail for a reason that \
         has nothing to do with the call path"
    );

    let out = mgr
        .call_tool("fixture", "echo", serde_json::json!({"text": "hello"}))
        .await
        .expect("an allowed tool must be callable");
    let rendered = out.to_string();
    assert!(
        rendered.contains("echo: hello"),
        "the answer must come from the *server*, not from the client: {rendered}"
    );
}

/// Governance is the whole reason the default posture is safe, and it has to
/// hold against a server NIKI has never seen.
///
/// `write_note` is not marked read-only, so `read_only` governance — the
/// default — must refuse it. A governance check that only ever ran against a
/// trusted list would be untested exactly where it matters.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_governance_refuses_a_mutating_tool() {
    let mut mgr = manager("fixture", &[]);
    mgr.connect_all().await.expect("the fixture must start");

    // And it is not merely hidden from the summary: the call itself is refused.
    let err = mgr
        .call_tool("fixture", "write_note", serde_json::json!({"body": "x"}))
        .await
        .expect_err("read-only governance must refuse a tool that can write");
    let text = err.to_string();
    assert!(
        text.to_lowercase().contains("read") || text.to_lowercase().contains("govern"),
        "the refusal must say why, so a model can report it rather than \
         retry: {text}"
    );
}

/// A tool that is not on the server is a name error, not a transport hang.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_the_server_never_advertised_is_refused_by_name() {
    let mut mgr = manager("fixture", &[]);
    mgr.connect_all().await.expect("the fixture must start");
    let err = mgr
        .call_tool("fixture", "no_such_tool", serde_json::json!({}))
        .await
        .expect_err("an unknown tool must not reach the server");
    assert!(
        err.to_string().contains("no_such_tool"),
        "and the error must name it: {err}"
    );
}

/// A transport error is reported as a transport error.
///
/// The fixture is told to fail its `tools/call`, so this is a real JSON-RPC
/// error travelling back over the pipe — not a synthesised one. The claim
/// being pinned is that a server-side failure does not reach the model as
/// `Success` with nothing in it, which is the defect `web_search` carried for
/// two batches.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_side_failure_is_an_error_not_an_empty_success() {
    let mut mgr = manager("fixture", &[("MCP_FIXTURE_FAIL", "1")]);
    mgr.connect_all().await.expect("the fixture must start");
    // Discovery still works, so this isolates the call path.
    let err = mgr
        .call_tool("fixture", "echo", serde_json::json!({"text": "hi"}))
        .await
        .expect_err("a JSON-RPC error must surface as an Err");

    // The *chain*, not `to_string()`.
    //
    // `anyhow::Error::to_string()` prints only the outermost context, so the
    // first version of this assertion — which used it — failed against working
    // code. `Display` gives `MCP tools/call 'echo' failed`; the server's own
    // words are in the causes below it, reachable only through `{:#}` or
    // walking `.chain()`.
    //
    // That is not a test artefact, it is a hazard for anything that shows an
    // error to a model: a tool result built from `e.to_string()` would tell the
    // model the call failed and nothing about why, and the model would retry
    // the same call. Anything surfacing an MCP error must use the chain.
    let chain = format!("{err:#}");
    assert!(
        chain.contains("told to fail"),
        "the server's own message must reach the caller through the chain. \
         `to_string()` alone gives: {err}\nThe chain gives: {chain}"
    );
    assert!(
        !err.to_string().contains("told to fail"),
        "and this test is only meaningful because `to_string()` does *not* \
         carry it — if the client ever inlines the cause into the outer \
         context, update this assertion rather than deleting it"
    );
}

/// A held manager keeps its server alive; a dropped one does not.
///
/// The slice this file exists for. Before it, the manager was a local of the
/// discovery block, so a server was killed the moment its tools were listed —
/// and the summary the user saw named tools that were already gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_manager_keeps_its_server_callable() {
    // A *discovery* scope, ended: a manager that listed its tools and then
    // went out of scope, taking its stdio children with it via `kill_on_drop`.
    // This is the shape the pipeline used to have, and the reason a call could
    // not work — not an unreachable function but a dead server.
    {
        let mut discovered = manager("fixture", &[]);
        discovered.connect_all().await.expect("connect");
        let scoped = std::sync::Arc::new(discovered);
        let listed = scoped.tools_summary();
        assert!(
            !listed.is_empty(),
            "the discovery scope must see the tools, or it is not testing the \
             right thing"
        );
    }
    // That scope has ended and its server is gone with it. Now the run-scoped
    // one, and two calls with unrelated work in between.
    let mut run = manager("fixture", &[]);
    run.connect_all().await.expect("connect");
    let run_scoped = std::sync::Arc::new(run);
    for text in ["first", "second"] {
        let out = run_scoped
            .call_tool("fixture", "echo", serde_json::json!({"text": text}))
            .await
            .unwrap_or_else(|e| {
                panic!("a call for {text:?} after the discovery scope failed: {e:#}")
            });
        assert!(
            out.to_string().contains(text),
            "a call made after the discovery scope ended must still reach the \
             server, and the server must still be the one answering: {out}"
        );
    }
}

/// The pipeline holds the manager for the run rather than for the discovery
/// block.
///
/// A behavioural test cannot see this: the defect was a *scope*, and a scope
/// is only observable by making a call from outside it — which the test above
/// does against a manager the test itself holds. So this one reads the source,
/// and it names the region: the old shape was a `let mut mgr` local whose
/// `Arc` was created and dropped inside the same block, which reads the same
/// as the new one to a grep for "McpManager".
#[test]
fn the_pipeline_holds_the_mcp_manager_beyond_discovery() {
    let pipeline = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/orchestrator/pipeline.rs"
    ))
    .expect("pipeline.rs must be readable");
    let start = pipeline
        .find("MCP tool discovery (optional, launch-plan C1)")
        .expect("the discovery block must exist");
    // 2600 characters, measured: the code under test is 1865 past the marker,
    // behind a comment explaining why the old shape was wrong. The first
    // version of this used 2500 and failed on the *clean* tree, because the
    // window stopped inside the comment and never reached the thing it was
    // checking for — the same trap as the `ToolCall` arm in batch 6, and the
    // reason a window is never sized by eye.
    let region: String = pipeline[start..].chars().take(2600).collect();

    // The invariant is the **shape of the block's value**, not the absence of
    // a local. The first version asserted the manager was not a local, which
    // the correct code also is — it is built there and handed to an `Arc`
    // that leaves. What has to be true is that what leaves is the manager.
    assert!(
        !region.contains("let mcp_tools: String = if bare {"),
        "the block's value is a `String` again, so the manager drops at the \
         end of it and every stdio child goes with it — the original defect. \
         The region reads:\n{region}"
    );
    assert!(
        region.contains("Some(std::sync::Arc::new(mgr))"),
        "the `Arc` has to be created here and returned out of the block, or \
         nothing outlives the discovery. The region reads:\n{region}"
    );
    // And the teardown has to be at the end of the run, not at the end of the
    // block — otherwise the servers are killed either way and the `Arc` buys
    // nothing.
    let shutdown_at = pipeline
        .rfind("m.shutdown().await;")
        .expect("the run must shut the servers down");
    let result_at = pipeline
        .rfind("Ok(PipelineResult {")
        .expect("the run must return a result");
    assert!(
        shutdown_at < result_at,
        "shutdown must come before the result is returned, and not somewhere \
         near the discovery block at the top of a 4,000-line function"
    );
    // And the registration itself, in **both** loops that run tools. The gap
    // between "the server is alive" and "the model can call it" is one call,
    // and it is the one a reader skims — and wiring only the Coder's loop
    // would leave a research assistant that cannot search, which is the loop
    // most likely to need one.
    //
    // Counted, not grepped: two registrations, one per loop.
    let registrations = pipeline
        .match_indices("build_registry(&mut registry")
        .count();
    assert_eq!(
        registrations, 2,
        "both tool loops must register the run's MCP tools — the Coder's and \
         the research loop's. Found {registrations}."
    );
    for loop_name in [
        "MCP tools registered with the tool loop",
        "MCP tools registered with the research loop",
    ] {
        assert!(
            pipeline.contains(loop_name),
            "the {loop_name:?} registration is missing or renamed; if the \
             wiring moved, this test should say so rather than go quiet"
        );
    }
}

/// And a graceful shutdown at the end of a run is reachable, which is what
/// makes the servers reapable at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_is_callable_on_a_manager_that_connected() {
    let mut mgr = manager("fixture", &[]);
    mgr.connect_all().await.expect("connect");
    assert_eq!(mgr.connected_servers(), 1);
    mgr.shutdown().await;
    // Best-effort by contract: a server that will not close must not fail the
    // run that already produced its result, so there is nothing to assert on
    // the return. What is asserted is that the call exists and returns.
}

// ---------------------------------------------------------------------------
// The model can actually reach the tool. B7-01 made the servers live; this is
// the other half — a `Tool` in the registry, under a name the loop dispatches.
// ---------------------------------------------------------------------------

use niki::runtime::mcp_tool::{build_registry, qualified_name};
use niki::runtime::tools::Tool;
use niki::runtime::{ToolRegistry, build_baseline_registry};

async fn connected() -> std::sync::Arc<McpManager> {
    let mut mgr = manager("fixture", &[]);
    mgr.connect_all().await.expect("the fixture must start");
    std::sync::Arc::new(mgr)
}

fn ctx() -> niki::runtime::ToolContext {
    niki::runtime::ToolContext {
        agent_id: niki::mission::AgentId("t".into()),
        mission_id: niki::mission::MissionId("t".into()),
        role: "coder".into(),
        project_path: std::env::temp_dir(),
        permissions: std::collections::HashMap::new(),
        permission_mode: "bypass".into(),
        fail_closed_headless: false,
        network_allowlist: Vec::new(),
        task_store: None,
        human_input: None,
        mcp: None,
    }
}

/// A discovered tool is in the registry, under a name the model can be told.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discovered_tool_is_in_the_registry() {
    let mgr = connected().await;
    let mut reg = build_baseline_registry();
    let added = build_registry(&mut reg, mgr);
    assert_eq!(added, 1, "exactly `echo` — `write_note` is not read-only");
    let name = qualified_name("fixture", "echo");
    assert!(reg.get(&name).is_some(), "the registry must carry {name:?}");
}

/// And calling it through the registry reaches the server.
///
/// This is the end-to-end claim: a model that emits `mcp__fixture__echo`
/// gets the server's own answer back. A test that only checked the name was
/// registered would pass with a tool that returned a constant.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn calling_through_the_registry_reaches_the_server() {
    let mgr = connected().await;
    let mut reg = build_baseline_registry();
    build_registry(&mut reg, mgr);
    let name = qualified_name("fixture", "echo");
    let out = reg
        .execute(
            &name,
            niki::runtime::tools::ToolInput::new(serde_json::json!({"text": "via-registry"})),
            &ctx(),
        )
        .await;
    assert_ne!(
        out.status,
        niki::runtime::tools::ToolStatus::Failed,
        "an allowed MCP tool must be callable through the registry: {:?} / {:?}",
        out.summary,
        out.diagnostics
    );
    assert_eq!(
        out.status,
        niki::runtime::tools::ToolStatus::Success,
        "the call failed: {:?} / {:?}",
        out.summary,
        out.diagnostics
    );
    match &out.data {
        niki::runtime::tools::ToolData::Text { text } => assert!(
            text.contains("echo: via-registry"),
            "the answer must be the server's: {text}"
        ),
        other => panic!("a text result must arrive as Text, got {other:?}"),
    }
}

/// The name cannot collide with NIKI's own tools.
///
/// An MCP server is a third party and may call a tool `read` or `bash`. A flat
/// name would let a server's `bash` shadow the real one — and whichever won the
/// `register` race would be the one the model gets, silently.
#[test]
fn an_mcp_tool_cannot_shadow_a_niki_tool() {
    for (server, tool) in [("fixture", "read"), ("bash", "read"), ("x", "write")] {
        let name = qualified_name(server, tool);
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(niki::runtime::tools::ReadTool));
        assert!(
            reg.get(&name).is_none(),
            "{name:?} must not resolve to NIKI's own tool"
        );
    }
    // And the prefix is what makes the space disjoint, stated as a test.
    assert!(qualified_name("fixture", "read").starts_with("mcp__"));
}

/// A server that documents nothing still gets a description the model can act
/// on — and one that does not pretend to know the arguments.
#[test]
fn an_undocumented_tool_says_so() {
    let adapter = niki::runtime::mcp_tool::McpToolAdapter::new(
        std::sync::Arc::new(McpManager::new()),
        niki::mcp::McpTool {
            name: "mystery".into(),
            description: "   ".into(),
            server_name: "fixture".into(),
            input_schema: None,
            read_only: true,
        },
    );
    let d = adapter.def();
    assert!(
        d.description.contains("no documentation"),
        "an undocumented tool must say so rather than advertise a blank: {}",
        d.description
    );
    assert!(
        d.parameters.contains("additionalProperties"),
        "and must not pretend to know the arguments: {}",
        d.parameters
    );
}

/// Read-only is `Allow`; anything else is `Ask`.
///
/// Under the default posture governance already denies the mutating tool, so
/// this is belt and braces — and a user who turns governance *off* still gets
/// a prompt rather than silent third-party writes.
#[test]
fn the_permission_mirrors_the_governance() {
    use niki::runtime::tools::PermissionRequirement;
    let mk = |read_only: bool| {
        niki::runtime::mcp_tool::McpToolAdapter::new(
            std::sync::Arc::new(McpManager::new()),
            niki::mcp::McpTool {
                name: "t".into(),
                description: "d".into(),
                server_name: "s".into(),
                input_schema: None,
                read_only,
            },
        )
        .def()
        .permission
    };
    assert_eq!(mk(true), PermissionRequirement::Allow);
    assert_eq!(mk(false), PermissionRequirement::Ask);
}

/// A tool-level failure inside a successful call is a failure.
///
/// MCP reports this as `isError: true` with HTTP 200 and a well-formed result:
/// the *call* worked, the *tool* did not. A client that only looks at the
/// transport reports `Success` and hands the model a payload that reads like
/// an answer — the exact defect `web_search` shipped for two batches, aimed at
/// a third-party server's output instead of NIKI's own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_level_error_is_not_reported_as_success() {
    let mut mgr = manager("fixture", &[("MCP_FIXTURE_ISERROR", "1")]);
    mgr.connect_all().await.expect("the fixture must start");
    let mut reg = build_baseline_registry();
    build_registry(&mut reg, std::sync::Arc::new(mgr));

    let out = reg
        .execute(
            &qualified_name("fixture", "echo"),
            niki::runtime::tools::ToolInput::new(serde_json::json!({"text": "x"})),
            &ctx(),
        )
        .await;
    assert_eq!(
        out.status,
        niki::runtime::tools::ToolStatus::Failed,
        "isError: true is a tool that failed, whatever the transport said. \
         The result was: {:?}",
        out.data
    );
    assert!(
        out.diagnostics.iter().any(|d| d.contains("isError")),
        "and the diagnostic must name the reason, so a model is not left \
         re-reading a payload that looks like an answer: {:?}",
        out.diagnostics
    );
}
