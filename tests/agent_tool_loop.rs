//! A stage must be able to be a loop and still produce the typed artifact.
//!
//! NIKI's agents were one LLM call each: the harness pasted some file contents
//! in and the model had to emit a whole validated artifact in one shot, with no
//! way to read a file, run a test, or look at its own previous attempt. That is
//! the single biggest reason a small model could not complete a run — and the
//! reason both vendors ship a *loop* as the core rather than a pipeline of
//! one-shot calls.
//!
//! The audit trail is not given up. The loop ends by calling `submit_artifact`,
//! whose input schema **is** the artifact schema, so the contract is unchanged
//! and auditable — but the model reaches it after exploring, and can be
//! corrected, rather than having to satisfy it blind on the first token.

use anyhow::Result;
use niki::llm::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage,
};
use niki::runtime::tools::{LoopOptions, submit_artifact_spec};
use niki::runtime::{LoopMessage, build_baseline_registry, run_tool_loop_with};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A script: read a file, then submit an artifact.
struct ScriptedAgent {
    /// Per-turn: the tool call to make, or `None` to answer in prose.
    script: Vec<Option<(String, serde_json::Value)>>,
    turn: AtomicUsize,
    seen_prompts: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl LlmProvider for ScriptedAgent {
    fn provider_name(&self) -> &str {
        "scripted"
    }

    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk>> + Send>>> {
        unimplemented!("the tool loop uses complete()")
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.seen_prompts
            .lock()
            .expect("lock")
            .push(request.user_message.clone());
        let n = self.turn.fetch_add(1, Ordering::SeqCst);
        let step = self.script.get(n).cloned().flatten();
        Ok(match step {
            Some((name, arguments)) => CompletionResponse {
                content: String::new(),
                model: request.model.clone(),
                usage: TokenUsage::default(),
                tool_calls: vec![niki::llm::provider::ToolCall {
                    id: format!("c{n}"),
                    name,
                    arguments,
                }],
                finish_reason: Some("tool_calls".into()),
            },
            None => CompletionResponse {
                content: "I have finished.".into(),
                model: request.model.clone(),
                usage: TokenUsage::default(),
                tool_calls: Vec::new(),
                finish_reason: Some("stop".into()),
            },
        })
    }
}

fn ctx(dir: &std::path::Path) -> niki::runtime::ToolContext {
    niki::runtime::ToolContext {
        agent_id: niki::mission::AgentId("t".into()),
        mission_id: niki::mission::MissionId("t".into()),
        role: "coder".into(),
        project_path: dir.to_path_buf(),
        permissions: std::collections::HashMap::new(),
        permission_mode: "bypass".into(),
        task_store: None,
    }
}

fn artifact() -> serde_json::Value {
    serde_json::json!({
        "edits": [{ "search": "old", "replace": "new" }],
        "files_changed": [{ "path": "src/lib.rs", "action": "modify", "language": "rust" }],
        "implementation_notes": "replaced",
        "spec_adherence": "matches"
    })
}

fn run(
    script: Vec<Option<(String, serde_json::Value)>>,
) -> (niki::runtime::tools::LoopOutput, Vec<String>) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("src.rs").join("x").as_os_str(), "").ok();
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
    std::fs::write(dir.path().join("src/lib.rs"), "old\n").expect("write");

    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let agent = ScriptedAgent {
        script,
        turn: AtomicUsize::new(0),
        seen_prompts: seen.clone(),
    };
    let registry = build_baseline_registry();
    let c = ctx(dir.path());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let out = rt
        .block_on(run_tool_loop_with(
            LoopOptions {
                submit_artifact: Some(submit_artifact_spec(serde_json::json!({
                    "type": "object",
                    "properties": artifact(),
                    "required": ["edits", "files_changed"],
                }))),
            },
            &agent,
            "m",
            &registry,
            &c,
            vec![LoopMessage::User("add sum()".into())],
            None,
            8,
            None,
            None,
        ))
        .expect("loop");
    let prompts = seen.lock().expect("lock").clone();
    (out, prompts)
}

#[test]
fn an_agent_can_explore_and_then_submit_its_artifact() {
    let out = run(vec![
        Some(("read".into(), serde_json::json!({ "path": "src/lib.rs" }))),
        Some(("grep".into(), serde_json::json!({ "query": "fn" }))),
        Some(("submit_artifact".into(), artifact())),
    ])
    .0;

    assert_eq!(
        out.artifact,
        Some(artifact()),
        "the submitted artifact must survive the loop intact"
    );
    assert_eq!(out.steps, 3, "three turns: read, grep, submit");
    assert_eq!(
        out.tool_calls
            .iter()
            .filter(|(n, ok)| n == "submit_artifact" && *ok)
            .count(),
        1,
        "submit is recorded exactly once, and as a success"
    );
}

#[test]
fn the_tools_the_agent_used_are_fed_back_into_its_next_turn() {
    // The point of a loop rather than a bigger prompt: the model sees what it
    // actually read. Without this the tools are theatre.
    let (out, prompts) = run(vec![
        Some(("read".into(), serde_json::json!({ "path": "src/lib.rs" }))),
        Some(("submit_artifact".into(), artifact())),
    ]);

    assert!(prompts.len() >= 2, "there was a second turn");
    assert!(
        prompts[1].contains("old"),
        "the second turn must contain what the file actually said. Got:\n{}",
        prompts[1]
    );
    assert!(out.artifact.is_some());
}

#[test]
fn an_agent_that_never_submits_produces_no_artifact_rather_than_a_guess() {
    // Fail closed. A loop that times out with nothing submitted must not have
    // the harness invent a plausible artifact from its last prose message.
    let out = run(vec![Some((
        "read".into(),
        serde_json::json!({ "path": "src/lib.rs" }),
    ))])
    .0;
    assert_eq!(
        out.artifact, None,
        "an agent that never submitted must not yield an artifact"
    );
}

#[test]
fn the_submit_tool_carries_the_artifact_schema_verbatim() {
    // The audit trail depends on this: the tool the model calls IS the schema
    // the stage is graded on, not a wrapper around it.
    let schema = serde_json::json!({
        "type": "object",
        "properties": { "edits": { "type": "array" } },
        "required": ["edits"],
    });
    let spec = submit_artifact_spec(schema.clone());
    assert_eq!(spec.name, "submit_artifact");
    assert_eq!(
        spec.parameters, schema,
        "the submit tool's parameters must be the artifact schema itself"
    );
}

/// The Coder stage must actually be a loop.
///
/// The loop and the `submit_artifact` tool existed and were tested in isolation
/// for a while, with `grep -c run_tool_loop_with src/orchestrator/pipeline.rs`
/// returning 0 — built, and wired to nothing. A live multi-agent run against
/// `qwen2.5-coder:3b` still died at the Coder with "Failed to parse artifact
/// JSON", which is what prompted the wiring.
///
/// This is a source-level assertion on purpose. A stage's execution is not
/// observable from a unit test, and "it works on my machine with a good model"
/// is not a property — the same code fails or succeeds depending on the model.
#[test]
fn the_coder_stage_runs_on_the_tool_loop() {
    let src = include_str!("../src/orchestrator/pipeline.rs");
    // BOTH halves: the gate must actually select the Coder, and the function must
    // actually be called from it.
    //
    // The first version asserted only that the function *existed*, so changing
    // `if role == AgentRole::Coder` to `if false && role == ...` -- the exact
    // "built and wired to nothing" state this test exists to prevent -- left it
    // green. Two mutations of the pipeline later proved it, and both "passed".
    assert!(
        src.contains("let json = if role == AgentRole::Coder {"),
        "the Coder stage must be gated on the role, not disabled. Found the loop wired to \
         nothing, or not wired at all."
    );
    assert!(
        src.contains("match run_coder_tool_loop("),
        "the Coder stage must CALL the tool loop, not merely define it"
    );
    assert!(
        src.contains("submit_artifact_spec"),
        "and the loop must end by submitting the typed artifact, so the audit trail survives"
    );
}

/// A model that ignores the tool and answers in prose must fall back to the
/// one-shot path, not fail.
///
/// The loop can only add capability. If it made a previously-working
/// configuration worse — a provider with no tool calling, a model that answers
/// in prose — the change would be a regression dressed as an improvement.
#[test]
fn a_loop_that_submits_nothing_falls_back_instead_of_failing() {
    let out = run(vec![Some((
        "read".into(),
        serde_json::json!({ "path": "src/lib.rs" }),
    ))])
    .0;
    assert_eq!(
        out.artifact, None,
        "a model that never submitted produces no artifact — the caller falls back to the \
         one-shot path on exactly this signal"
    );
}

/// Ollama must surface tool calls, and send the tools.
///
/// This provider returned `tool_calls: Vec::new()` unconditionally, with a
/// comment claiming Ollama has no native tool support. It does — and it is the
/// provider the README's zero-setup path tells a first-time user to install
/// (`ollama pull qwen2.5-coder:3b`). So the Coder's tool loop could never run
/// for the product's headline setup: the model was never shown the tools, and
/// anything it returned was discarded. The loop always fell back to a single
/// call, and the stage failed intermittently for reasons that had nothing to do
/// with the model's ability.
///
/// The whole rest of Phase 1 is downstream of this: a loop whose provider cannot
/// express a tool call is a loop that never runs.
#[test]
fn ollama_sends_and_returns_tool_calls() {
    let src = include_str!("../src/llm/ollama.rs");
    assert!(
        src.contains("payload[\"tools\"]"),
        "the Ollama provider must send the tools it is given, or the model is never told \
         they exist"
    );
    assert!(
        src.contains("parse_tool_calls(&data)"),
        "and must read the calls back instead of discarding them"
    );
    assert!(
        !src.contains("no native tool support on this provider"),
        "that claim was the bug's cover story; it is false and must not come back"
    );
}

#[test]
fn ollama_tool_calls_are_parsed_in_both_shapes() {
    use niki::llm::ollama::parse_tool_calls;
    // Current Ollama: `arguments` is an object.
    let modern = serde_json::json!({
        "message": {
            "content": "",
            "tool_calls": [
                {"function": {"name": "read", "arguments": {"path": "src/lib.rs"}}}
            ]
        }
    });
    let calls = parse_tool_calls(&modern);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read");
    assert_eq!(calls[0].arguments["path"], "src/lib.rs");

    // Older builds sent `arguments` as a JSON string.
    let legacy = serde_json::json!({
        "message": {
            "tool_calls": [
                {"function": {"name": "grep", "arguments": "{\"query\":\"fn\"}"}}
            ]
        }
    });
    let calls = parse_tool_calls(&legacy);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["query"], "fn");

    // No tools, a malformed entry, and a non-array: none of them may panic, and
    // a single bad entry must not discard the good ones beside it.
    assert!(parse_tool_calls(&serde_json::json!({"message": {"content": "hi"}})).is_empty());
    assert!(parse_tool_calls(&serde_json::json!({})).is_empty());
    let mixed = serde_json::json!({
        "message": {
            "tool_calls": [
                {"function": {"arguments": {}}},
                {"function": {"name": "read", "arguments": {"path": "a"}}}
            ]
        }
    });
    let calls = parse_tool_calls(&mixed);
    assert_eq!(
        calls.len(),
        1,
        "one bad entry must not discard the good one"
    );
    assert_eq!(calls[0].name, "read");
}

/// A diagnostic nobody will read is not a diagnostic.
///
/// `main` installs `tracing_subscriber` with `EnvFilter::from_default_env()`,
/// which is ERROR-only unless `RUST_LOG` is set. So a `tracing::warn!` about the
/// Coder's loop submitting an invalid artifact is invisible in exactly the
/// situation it exists for: someone running the binary from a terminal to see
/// why their run failed.
#[test]
fn a_failed_coder_loop_says_so_on_stderr_not_only_in_tracing() {
    let src = include_str!("../src/orchestrator/pipeline.rs");
    let start = src
        .find("run_coder_tool_loop")
        .expect("the function exists");
    let body = &src[start..];
    let branch = body
        .find("failed validation")
        .or_else(|| body.find("did not validate"))
        .expect("the invalid-artifact branch exists");
    let window = &body[branch.saturating_sub(600)..(branch + 900).min(body.len())];
    assert!(
        window.contains("eprintln!"),
        "a failed tool loop must be reported on stderr. A `tracing::warn!` alone is \\
         invisible without RUST_LOG, and this is a CLI."
    );
}

/// A tool call returned as message text is still a tool call.
///
/// Measured on the model this project's README tells a first-time user to
/// install: asked for a `submit_artifact` call, it produced one — as JSON in the
/// message *content*, not in the structured `tool_calls` field. The provider
/// looked only at the structured field, found nothing, and the stage fell back
/// for a reason nothing in the logs explained. `niki doctor --measure` scored
/// that model 0/4 while it was completing full four-agent runs.
#[test]
fn a_tool_call_returned_as_text_is_still_recovered() {
    use niki::llm::ollama::parse_tool_calls_from_content;

    // Ollama-style, fenced — what the model actually emitted.
    let fenced = "```json\n{\n  \"name\": \"submit_artifact\",\n  \"arguments\": {\n    \"edits\": [{\"search\": \"old\", \"replace\": \"new\"}]\n  }\n}\n```";
    let calls = parse_tool_calls_from_content(fenced);
    assert_eq!(
        calls.len(),
        1,
        "a fenced tool call in the content must be recovered"
    );
    assert_eq!(calls[0].name, "submit_artifact");
    assert_eq!(calls[0].arguments["edits"][0]["search"], "old");

    // Bare, unfenced.
    let bare = r#"{"name":"read","arguments":{"path":"src/lib.rs"}}"#;
    let calls = parse_tool_calls_from_content(bare);
    assert_eq!(calls[0].name, "read");

    // The chat-completions spelling, and stringified arguments.
    let alt = r#"{"tool":"grep","parameters":"{\"query\":\"fn\"}"}"#;
    let calls = parse_tool_calls_from_content(alt);
    assert_eq!(calls[0].name, "grep");
    assert_eq!(calls[0].arguments["query"], "fn");
}

#[test]
fn prose_is_never_mistaken_for_a_tool_call() {
    use niki::llm::ollama::parse_tool_calls_from_content;
    // The failure this must not have: inventing a call out of ordinary text.
    for text in [
        "I have updated the function to sum the slice.",
        "",
        "```json\n{\"edits\": [{\"search\": \"a\", \"replace\": \"b\"}]}\n```", // an artifact, not a call
        "The user asked me to read src/lib.rs and I did.",
    ] {
        assert!(
            parse_tool_calls_from_content(text).is_empty(),
            "prose must not become a tool call: {text:?}"
        );
    }
}
