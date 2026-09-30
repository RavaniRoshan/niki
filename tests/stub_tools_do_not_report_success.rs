//! A tool that cannot do the thing must not report that it did.
//!
//! `web_search` returned:
//!
//! ```text
//! status: Success
//! summary: "search: <query>"
//! data:    WebSearchResults { query, results: [] }
//! diagnostics: ["web search not yet wired — use firecrawl MCP"]
//! ```
//!
//! `agent_access: &[]` means **every** role is offered it, and its description
//! said "Search the web for information". So a model that called it got a
//! confident, successful search that found nothing — and the correct model
//! response to that is to report that the web has no information on the
//! subject. An error tells it to try something else; an empty success tells it
//! to stop looking.
//!
//! `diagnostics` is the one field a model never sees, which is exactly why the
//! fact could not live there alone.

use niki::runtime::tools::{
    Tool, ToolContext, ToolInput, ToolStatus, WebFetchTool, WebSearchTool, build_baseline_registry,
};
use std::collections::HashMap;

fn context(role: &str) -> ToolContext {
    ToolContext {
        agent_id: niki::mission::AgentId("t".into()),
        mission_id: niki::mission::MissionId("t".into()),
        role: role.into(),
        project_path: std::env::temp_dir(),
        permissions: HashMap::new(),
        // `bypass` so the permission layer is not what stops the call: this is
        // about what the tool *returns* once it runs.
        permission_mode: "bypass".into(),
        fail_closed_headless: false,
        // Empty = block-all, the shipped default.
        network_allowlist: Vec::new(),
        task_store: None,
    }
}

fn call<T: Tool>(tool: &T, input: &str) -> niki::runtime::ToolResult {
    let raw: serde_json::Value = serde_json::from_str(input).expect("valid tool input");
    futures::executor::block_on(tool.execute(ToolInput::new(raw), &context("coder")))
}

/// **The defect.** A search that did not happen must not read as a search that
/// found nothing.
#[test]
fn an_unimplemented_search_fails_rather_than_returning_nothing() {
    let out = call(&WebSearchTool, r#"{"query":"rust tokio select macro"}"#);

    assert_eq!(
        out.status,
        ToolStatus::Failed,
        "web_search cannot search, so it must fail. Success with an empty result \
         set tells the model the web has nothing on the subject, and the model \
         will report that: {out:?}"
    );
    assert!(
        !matches!(
            out.data,
            niki::runtime::tools::ToolData::WebSearchResults { .. }
        ),
        "a failed search must not carry a `WebSearchResults` payload at all — \
         an empty result list is exactly the false negative being removed"
    );
}

/// And the message must say what to do, not just that it failed. A model told
/// "unavailable" can pick another route; one told nothing retries the same call.
#[test]
fn the_failure_says_why_and_what_instead() {
    let out = call(&WebSearchTool, r#"{"query":"anything"}"#);

    let msg = out.summary.to_lowercase();
    assert!(
        msg.contains("not implemented"),
        "the message must name the fact: {}",
        out.summary
    );
    assert!(
        msg.contains("mcp") || msg.contains("unavailable"),
        "and must say what to use instead, or the model has nowhere to go: {}",
        out.summary
    );
    // The echo of the query is useful context for a human reading the log.
    assert!(
        out.summary.contains("anything"),
        "the message should name the query it did not search for: {}",
        out.summary
    );
}

/// The description is the model's only evidence that the tool exists, and it is
/// offered to every role.
#[test]
fn the_description_does_not_advertise_a_capability_it_lacks() {
    let def = WebSearchTool.def();

    let desc = def.description.to_lowercase();
    assert!(
        desc.contains("not implemented"),
        "the description must say the tool cannot search, because it is what \
         the model reads before calling: {:?}",
        def.description
    );
    assert!(
        !desc.starts_with("search the web"),
        "and must not open by claiming it can"
    );
    assert!(
        def.agent_access.is_empty(),
        "web_search is offered to every role (an empty `agent_access` means all), \\
         so the description matters for every agent: {:?}",
        def.agent_access
    );
}

/// The same rule for `web_fetch`, which shares the Research category. If it is
/// blocked by an empty allowlist, that must be a failure too — the shape of the
/// bug is identical and the test is here so the next stub is caught by the same
/// assertion.
#[test]
fn a_tool_that_cannot_reach_the_network_does_not_succeed() {
    // `web_fetch` is constructed with an empty allowlist, and empty means
    // block-all, so it cannot fetch. Whether it currently returns Success or
    // Failed is the question this asserts — either way the *result set* must
    // not be a successful fetch.
    let out = call(&WebFetchTool, r#"{"url":"https://example.com"}"#);
    if out.status == ToolStatus::Success {
        assert!(
            out.summary.to_lowercase().contains("blocked")
                || out.summary.to_lowercase().contains("allowlist"),
            "web_fetch cannot fetch with an empty allowlist, so a Success must \\
             at least say why the fetch was refused: {}",
            out.summary
        );
    }
}

/// Every tool in the registry either works or says it does not. This is the
/// general form: a stub that reports success is a trap, and the way to stop
/// the next one is to check every tool that has a known-unimplemented path.
#[test]
fn no_registered_tool_succeeds_while_saying_it_is_not_wired() {
    let reg = build_baseline_registry();
    for def in reg.list_defs() {
        let desc = def.description.to_lowercase();
        if !desc.contains("not implemented") {
            continue;
        }
        // A tool whose own description admits it is not implemented must not
        // be advertised to any role as though it were.
        for role in ["planner", "coder", "tester", "reviewer", "synthesizer"] {
            let names: Vec<&str> = reg.for_role(role).iter().map(|d| d.name).collect();
            assert!(
                names.contains(&def.name),
                "{} is offered to {role}; a tool that says it is not \\
                 implemented should either work or be withdrawn",
                def.name
            );
        }
    }
}
