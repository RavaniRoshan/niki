//! `web_fetch` could never fetch anything.
//!
//! The tool was constructed as `WebFetchTool::new(vec![])`, and
//! `WebFetchTool::is_allowed` treats an **empty allowlist as block all**:
//!
//! ```rust
//! if self.domain_allowlist.is_empty() {
//!     return false; // Empty allowlist = block all
//! }
//! ```
//!
//! So a tool offered to every role, described as "Fetch a web page", refused
//! every URL unconditionally — while `[network] domain_allowlist` sat in the
//! config documented as "Domain allowlist for outbound network egress. When
//! `network_disabled` is `true`, only these domains are allowed", and nothing
//! read it.
//!
//! The default stays block-all. A fetcher that reaches the network by default
//! is a worse defect than one that never does, and the whole point of an
//! allowlist is that the empty case means no.

/// Whether a fetcher built with `allowlist` permits `url`.
///
/// Deciding permission is the whole of the defect, and it is decidable without
/// a socket: a test that made a real request would be slower, flakier, and
/// would prove less about the rule.
fn allowed_by(allowlist: &[&str], url: &str) -> bool {
    let tool = niki::tools::web_fetch::WebFetchTool::new(
        allowlist.iter().map(|s| s.to_string()).collect(),
    );
    tool.is_allowed(url)
}

/// **The defect.** A configured allowlist must let its domains through.
#[test]
fn a_configured_allowlist_permits_its_domains() {
    assert!(
        allowed_by(&["docs.rs"], "https://docs.rs/tokio"),
        "a domain the user listed in `[network] domain_allowlist` must be \\
         fetchable. The tool was constructed with `vec![]`, so this was \\
         unconditionally false and the setting was read by nothing."
    );
}

#[test]
fn an_empty_allowlist_still_blocks_everything() {
    // The safe answer stays safe: this is not a slice that loosens a default.
    for url in [
        "https://docs.rs/tokio",
        "https://example.com",
        "http://localhost:8080",
    ] {
        assert!(
            !allowed_by(&[], url),
            "an empty allowlist must block {url} — block-all is the documented \\
             and the safe default"
        );
    }
}

#[test]
fn a_domain_outside_the_allowlist_is_still_blocked() {
    assert!(
        !allowed_by(&["docs.rs"], "https://evil.example.com/steal"),
        "the allowlist is an allowlist: listing one domain must not open the \\
         network to another"
    );
}

/// And the context must carry it. Without this the tool is back to `vec![]`
/// no matter what the config says — which is the shape of the original bug,
/// one level up.
#[test]
fn the_tool_context_carries_the_configured_allowlist() {
    use niki::runtime::ToolContext;
    let mut config = niki::config::NikiConfig::default();
    config.docker.network_allowlist = vec!["docs.rs".to_string()];

    // The Planner's context, built the way the pipeline builds it.
    let planner = ToolContext {
        agent_id: niki::mission::AgentId("t".into()),
        mission_id: niki::mission::MissionId("t".into()),
        role: "planner".into(),
        project_path: std::env::temp_dir(),
        permissions: std::collections::HashMap::new(),
        permission_mode: "manual".into(),
        fail_closed_headless: false,
        network_allowlist: config.docker.network_allowlist.clone(),
        task_store: None,
    };
    assert_eq!(
        planner.network_allowlist,
        vec!["docs.rs".to_string()],
        "the configured allowlist must reach the tool context"
    );

    // And the Coder's, which is threaded as a plain value for the same reason
    // `permission_mode` is — the loop builds its own context and must not need
    // a borrow of the config to do it.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/orchestrator/pipeline.rs"),
    )
    .expect("pipeline.rs must be readable");
    // Counted, not `contains`. There are seven sites — the research stage's
    // own `ToolContext` plus six stage-start call sites — and a `contains`
    // check reports "found" when *any* of them has it. So emptying one, which
    // is the exact regression this assertion exists for, left it green. Twice.
    const SITES: usize = 7;
    let found = src
        .matches("config.docker.network_allowlist.clone()")
        .count();
    assert_eq!(
        found, SITES,
        "every site that builds a ToolContext or starts a stage must pass the \
         configured `[network] domain_allowlist`. Found {found}, expected \
         {SITES}: one research context and six stage-start sites."
    );

    let coder_ctx = src
        .split("async fn run_coder_tool_loop(")
        .nth(1)
        .and_then(|r| r.split("task_store: None,").next())
        .expect("the Coder's context must exist");
    assert!(
        coder_ctx.contains("network_allowlist,"),
        "and the Coder's context must carry it, threaded as a plain value like \
         every other field it builds itself"
    );

    // `run_parallel_coders` takes the allowlist as a parameter and hands that
    // same one to each of its N coders, rather than each re-reading the config
    // — so N coders cannot end up with N different views of the network.
    // Asserted on the *parameter*, which is the mechanism; the first version
    // looked for a `ToolContext` literal inside `run_parallel_coders` and
    // failed, because it builds none.
    let parallel = src
        .split("async fn run_parallel_coders(")
        .nth(1)
        .expect("run_parallel_coders must exist");
    let head: String = parallel.chars().take(2000).collect();
    assert!(
        head.contains("network_allowlist: Vec<String>"),
        "`run_parallel_coders` must take the allowlist as a parameter, the \
         same way it takes the permission mode"
    );
    assert!(
        parallel.contains("network_allowlist.clone()"),
        "and must share that one value with every coder rather than \
         re-reading the config per coder"
    );
}

/// The tool must not construct itself with an empty list any more.
#[test]
fn the_tool_does_not_hardcode_an_empty_allowlist() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/tools.rs"),
    )
    .expect("tools.rs must be readable");
    assert!(
        !src.contains("WebFetchTool::new(vec![])"),
        "the tool is constructed with an empty allowlist again, and an empty \\
         allowlist is block-all: it can never fetch anything"
    );
    assert!(
        src.contains("_ctx.network_allowlist.clone()"),
        "and it must take the allowlist from the context the run configured"
    );
}
