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
    run_with(script, None)
}

/// Run the loop against an agent whose first answer is cut off and whose
/// second is whatever `second` says.
fn run_truncated_then_complete(
    truncated: &str,
    second: &str,
) -> (niki::runtime::tools::LoopOutput, Vec<String>) {
    let validate: niki::runtime::ArtifactValidator =
        std::sync::Arc::new(|v: &serde_json::Value| match v.get("edits") {
            Some(e) if e.as_array().is_some_and(|a| !a.is_empty()) => Ok(()),
            _ => Err("no edits".into()),
        });
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
    std::fs::write(dir.path().join("src/lib.rs"), "old\n").expect("write");

    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let agent = ProseAgent {
        prose: truncated.to_string(),
        script: vec![None],
        turn: AtomicUsize::new(0),
        seen_prompts: seen.clone(),
        after_first: Some(second.to_string()),
        finish: Some("length".into()),
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
                validate_artifact: Some(validate),
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
    (out, seen.lock().expect("lock").clone())
}

/// Run the loop against an agent whose only answer is fixed prose.
fn run_with_prose(
    prose: &str,
    script: Vec<Option<(String, serde_json::Value)>>,
) -> (niki::runtime::tools::LoopOutput, Vec<String>) {
    let validate: niki::runtime::ArtifactValidator =
        std::sync::Arc::new(|v: &serde_json::Value| {
            if v.get("edits")
                .map(|e| e.as_array().is_some_and(|a| a.is_empty()))
                == Some(true)
            {
                return Err("`edits` is empty".into());
            }
            match v.get("edits") {
                None => Err("no edits".into()),
                Some(_) => Ok(()),
            }
        });
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
    std::fs::write(dir.path().join("src/lib.rs"), "old\n").expect("write");

    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let agent = ProseAgent {
        prose: prose.to_string(),
        script,
        turn: AtomicUsize::new(0),
        seen_prompts: seen.clone(),
        after_first: None,
        finish: None,
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
                validate_artifact: Some(validate),
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
    (out, seen.lock().expect("lock").clone())
}

/// A scripted agent that answers with fixed prose on its last turn.
struct ProseAgent {
    prose: String,
    script: Vec<Option<(String, serde_json::Value)>>,
    turn: AtomicUsize,
    seen_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    /// What the second and later turns say, when the test needs the model to
    /// change its answer (a truncation retry, for instance).
    after_first: Option<String>,
    finish: Option<String>,
}

#[async_trait::async_trait]
impl LlmProvider for ProseAgent {
    fn provider_name(&self) -> &str {
        "prose"
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
                    id: format!("p{n}"),
                    name,
                    arguments,
                }],
                finish_reason: Some("tool_calls".into()),
            },
            None => CompletionResponse {
                content: match (&self.after_first, n) {
                    (Some(second), n) if n > 0 => second.clone(),
                    _ => self.prose.clone(),
                },
                model: request.model.clone(),
                usage: TokenUsage::default(),
                tool_calls: Vec::new(),
                finish_reason: Some(self.finish.clone().unwrap_or_else(|| "stop".into())),
            },
        })
    }
}

fn run_with(
    script: Vec<Option<(String, serde_json::Value)>>,
    validate_artifact: Option<niki::runtime::ArtifactValidator>,
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
                validate_artifact,
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
/// Superseded by `pipeline_guards::the_coder_stage_answers_by_calling_
/// submit_artifact`.
///
/// This used to assert that the pipeline source contains
/// `let json = if role == AgentRole::Coder {` and `match run_coder_tool_loop(`.
/// A source-text test cannot see the thing it was guarding: `run_coder_tool_loop`
/// loaded its prompt with a bare asset name, failed on the second line, and
/// returned `None` — so the loop was wired to nothing, in the most literal
/// sense, for its entire life, while every word this test looked for sat
/// exactly where it expected them.
///
/// The replacement runs the Coder through the pipeline and checks the artifact
/// it produced, which is the only version of this that can fail for the real
/// reason.
///
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

/// Ollama must send the tools, and read the calls back.
///
/// This provider returned `tool_calls: Vec::new()` unconditionally, with a
/// comment claiming Ollama has no native tool support. It does — and it is the
/// provider the README's zero-setup path tells a first-time user to install
/// (`ollama pull qwen2.5-coder:3b`). So the Coder's tool loop could never run
/// for the product's headline setup: the model was never shown the tools, and
/// anything it returned was discarded.
///
/// The previous version grepped `src/llm/ollama.rs` for `payload["tools"]` —
/// the same weakness the Coder loop's test had, where the string can be
/// present in the file while no request ever carries it. This points the
/// provider at a recording server and asserts on the bytes.
#[tokio::test]
async fn ollama_sends_the_tools_and_returns_the_calls() {
    use niki::config::ProviderConfig;
    use niki::llm::provider::{CompletionRequest, LlmProvider, ToolSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "qwen2.5-coder:3b",
            "message": {
                "content": "",
                "tool_calls": [
                    {"function": {"name": "read", "arguments": {"path": "src/lib.rs"}}}
                ]
            },
            "done": true,
        })))
        .mount(&server)
        .await;

    let provider = niki::llm::ollama::OllamaProvider::new(&ProviderConfig {
        api_key: Some("ollama".into()),
        base_url: Some(server.uri()),
        default_model: "qwen2.5-coder:3b".into(),
    })
    .expect("provider");

    let response = provider
        .complete(CompletionRequest {
            model: "qwen2.5-coder:3b".into(),
            user_message: "read src/lib.rs".into(),
            tools: Some(vec![ToolSpec {
                name: "read".into(),
                description: "Read file content with line numbers".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"]
                }),
            }]),
            ..Default::default()
        })
        .await
        .expect("the provider completes");

    assert_eq!(
        response.tool_calls.len(),
        1,
        "Ollama's tool calls must reach the loop, not be discarded"
    );
    assert_eq!(response.tool_calls[0].name, "read");
    assert_eq!(response.tool_calls[0].arguments["path"], "src/lib.rs");

    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        !requests.is_empty(),
        "the mock server recorded no request — the test proved nothing"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("json body");
    let sent = body
        .get("tools")
        .and_then(|t| t.as_array())
        .unwrap_or_else(|| panic!("the request carried no `tools` array: {body}"));
    assert_eq!(sent.len(), 1, "exactly the tool we offered");
    let name = sent[0]
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or_else(|| panic!("tool name missing: {}", sent[0]));
    assert_eq!(
        name, "read",
        "the model must be told the tool exists, by name"
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

/// The model submitted something the stage cannot accept. The loop must hand
/// the reason back and let it try again — not end the run on the first
/// malformed artifact, and not discard everything the model had already read.
///
/// Measured: a breadth sweep lost four of five runs to
/// `edits[0] has an empty search`. The loop submitted it, the loop returned,
/// the caller fell back to a fresh one-shot call that re-read nothing and
/// produced the same thing — so a first-attempt mistake was unrecoverable.
#[test]
fn a_rejected_artifact_goes_back_to_the_model_with_the_reason() {
    let bad = serde_json::json!({
        "edits": [{ "search": "", "replace": "new" }],
        "files_changed": [{ "path": "src/lib.rs", "action": "modify", "language": "rust" }],
        "implementation_notes": "x",
        "spec_adherence": "y"
    });
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = attempts.clone();
    let validate: niki::runtime::ArtifactValidator =
        std::sync::Arc::new(move |v: &serde_json::Value| {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if v["edits"][0]["search"] == serde_json::Value::String(String::new()) {
                Err("edits[0] has an empty `search`".to_string())
            } else {
                Ok(())
            }
        });

    let (out, prompts) = run_with(
        vec![
            Some(("read".into(), serde_json::json!({ "path": "src/lib.rs" }))),
            Some(("submit_artifact".into(), bad)),
            Some(("submit_artifact".into(), artifact())),
        ],
        Some(validate),
    );

    assert_eq!(
        out.artifact,
        Some(artifact()),
        "the second, corrected submission must be the one that comes back"
    );
    assert_eq!(out.steps, 3, "read, rejected submit, accepted submit");
    assert_eq!(
        attempts.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the validator runs once per submission"
    );
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("REJECTED") && p.contains("edits[0] has an empty `search`")),
        "the model must be shown the reason, not just a refusal: {prompts:?}"
    );
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("old") || p.contains("src/lib.rs")),
        "the earlier exploration stays in context — the correction must not have to start over"
    );
}

/// A rejection that never resolves must end the loop, not spin.
#[test]
fn a_model_that_never_conforms_gives_up_at_the_step_budget() {
    let bad = serde_json::json!({ "edits": [] });
    let validate: niki::runtime::ArtifactValidator =
        std::sync::Arc::new(|_v: &serde_json::Value| Err("never good enough".to_string()));

    let (out, _) = run_with(
        vec![
            Some(("submit_artifact".into(), bad.clone())),
            Some(("submit_artifact".into(), bad.clone())),
            Some(("submit_artifact".into(), bad.clone())),
        ],
        Some(validate),
    );

    assert!(
        out.artifact.is_none(),
        "an artifact that was never accepted must not be reported as the answer"
    );
    assert!(
        out.steps >= 3,
        "the loop used its budget trying, it did not stop at the first rejection"
    );
}

/// Without a validator the loop behaves exactly as before: the first
/// submission is the answer.
#[test]
fn a_loop_without_a_validator_accepts_the_first_submission() {
    let out = run(vec![Some(("submit_artifact".into(), artifact()))]).0;
    assert_eq!(out.artifact, Some(artifact()));
    assert_eq!(out.steps, 1);
}

/// The prompt and the mechanism have to agree.
///
/// The Coder's system prompt ended with "Respond with ONLY the raw JSON
/// artifact" — while the tool loop's actual protocol is a `submit_artifact`
/// call, and the loop accepts nothing else. A model that obeyed the prompt
/// produced prose, the loop returned no artifact, and the run fell back to the
/// one-shot path *without saying so*, because the notice only covered a
/// submission that failed validation and here nothing was ever submitted.
///
/// Measured: five breadth runs against qwen2.5-coder:3b, zero tool calls.
///
/// Rendered rather than grepped: the failure this guards against is the two
/// disagreeing, and reading the rendered text is the only way to see that.
fn render_coder_prompt(tool_loop: bool) -> String {
    let template = niki::load_asset("prompts/coder.md").expect("coder.md is embedded");
    let mut env = minijinja::Environment::new();
    env.add_template("coder", &template)
        .expect("template parses");
    let ctx = minijinja::context! {
        input_artifacts => vec![serde_json::json!({"summary": "add total()"}).to_string()],
        revision_round => 0,
        project_knowledge => "",
        project_memory => "",
        current_files => "src/lib.rs:\n    pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        mcp_tools => "",
        artifact_schema => "{\"type\":\"object\"}",
        tool_loop => tool_loop,
    };
    env.get_template("coder")
        .expect("template")
        .render(ctx)
        .expect("renders")
}

#[test]
fn the_tool_loop_prompt_does_not_tell_the_model_to_reply_in_prose() {
    let looped = render_coder_prompt(true);
    assert!(
        !looped.contains("Respond with ONLY the raw JSON artifact"),
        "in the loop the answer is a tool call; telling the model to reply with JSON is what \\
         made it produce prose the loop could not accept"
    );
    assert!(
        looped.contains("submit_artifact"),
        "the loop prompt must name the tool that ends the loop"
    );
    assert!(
        looped.contains("REJECTED"),
        "the loop prompt must tell the model that a rejection is recoverable and how to recover"
    );
}

#[test]
fn the_one_shot_prompt_still_asks_for_raw_json() {
    // The fallback path is unchanged: it has no tools, so "reply with JSON" is
    // the only correct instruction there.
    let one_shot = render_coder_prompt(false);
    assert!(
        one_shot.contains("Respond with ONLY the raw JSON artifact"),
        "without tools, the one-shot path must still ask for the raw artifact"
    );
    assert!(
        !one_shot.contains("You are running as a tool loop"),
        "the one-shot path must not be told it has tools it was not given"
    );
}

/// A loop that produces nothing has to say so, in words a user can use.
///
/// The notice used to fire only when the loop *submitted* something invalid.
/// A model that never called the tool at all returned empty with no message
/// anywhere, and the run silently became the one-shot path — indistinguishable
/// from a build without the loop. That is how the loop shipped with a prompt
/// that contradicted it and nobody noticed.
///
/// The wording is tested here because that is the part a unit test can see.
/// That it reaches *stderr* is the one thing it cannot, and it is checked
/// separately below against the source — the single non-behavioural assertion
/// left in this file, kept because there is no other way to observe it.
#[test]
fn the_fallback_notice_says_what_happened() {
    use niki::llm::provider::TokenUsage;
    use niki::runtime::tools::LoopOutput;

    let never_engaged = LoopOutput {
        content: "I would add a total() function.".into(),
        steps: 1,
        tool_calls: vec![],
        usage: TokenUsage::default(),
        feedback_turns: 0,
        truncated: false,
        artifact: None,
    };
    let msg = niki::orchestrator::pipeline::coder_loop_fallback_notice(&never_engaged);
    assert!(
        msg.contains("never called submit_artifact"),
        "the message must name what did not happen: {msg}"
    );
    assert!(
        msg.contains("no tool calls at all"),
        "and distinguish 'tried and gave up' from 'never engaged': {msg}"
    );
    assert!(
        msg.contains("describes the fallback, not this"),
        "and say which of the two errors the user is about to read: {msg}"
    );

    // A loop that used tools but never submitted is a different situation, and
    // the message has to say which one it was.
    let explored = LoopOutput {
        content: String::new(),
        steps: 4,
        tool_calls: vec![
            ("read".to_string(), true),
            ("grep".to_string(), true),
            ("bash".to_string(), false),
        ],
        usage: TokenUsage::default(),
        feedback_turns: 0,
        truncated: false,
        artifact: None,
    };
    let msg = niki::orchestrator::pipeline::coder_loop_fallback_notice(&explored);
    assert!(
        msg.contains("it did use: read, grep"),
        "the tools that did work are the clue to why it gave up: {msg}"
    );
    assert!(
        !msg.contains("bash"),
        "a tool that failed is not evidence the loop engaged: {msg}"
    );
}

/// The notice goes to stderr, not only to `tracing`.
///
/// `main` installs an ERROR-only `EnvFilter`, so `tracing::warn!` is invisible
/// unless `RUST_LOG` is set — and this is a CLI, where the user is already
/// looking at stderr. A test cannot observe a running process's stderr
/// (libtest captures the `eprintln!` family before the syscall), so this
/// checks the call exists and says plainly that it is the one thing here that
/// is not behavioural.
#[test]
fn the_fallback_notice_is_written_to_stderr() {
    let src = include_str!("../src/orchestrator/pipeline.rs");
    let start = src
        .find("fn run_coder_tool_loop")
        .expect("the function exists");
    let body = &src[start..];
    let branch = body
        .find("coder_loop_fallback_notice(&out)")
        .expect("the empty-loop branch calls the notice builder");
    let window = &body[branch..(branch + 400).min(body.len())];
    assert!(
        window.contains("eprintln!"),
        "a loop that returned nothing must be reported on stderr; a `tracing::warn!` alone \
         is invisible without RUST_LOG and this is a CLI"
    );
}

/// Every asset the pipeline loads by bare name must actually load.
///
/// `load_asset` resolves a *path*: it splits on the first `/` to pick the
/// embedded directory, and a bare `coder.md` has none — so the embedded lookup
/// missed, the filesystem fallback looked for `$CARGO_MANIFEST_DIR/coder.md`,
/// and it failed. `run_coder_tool_loop` swallowed that with `?` and returned
/// `None`, which the caller reads as "the loop produced no artifact". So the
/// Coder's tool loop returned on its second line, on every run, since the day
/// it was written, and the Coder always fell through to the one-shot path.
///
/// The tests that "covered" the loop were source-text greps. A grep cannot
/// see a function that runs and immediately gives up; it can only confirm the
/// words are still there.
///
/// This pins the *convention* — a bare `role_prompt` name does not load, which
/// is why every call site prefixes it. The call sites themselves are covered
/// behaviourally by `pipeline_guards::the_coder_stage_answers_by_calling_
/// submit_artifact`, which fails if either one drops the prefix.
#[test]
fn a_bare_role_prompt_name_does_not_load() {
    let (prompt_path, schema_path) =
        niki::orchestrator::pipeline::role_prompt(niki::artifacts::types::AgentRole::Coder);
    // Bare name + `prompts/` — exactly what the two call sites do.
    let prompt = niki::load_asset(&format!("prompts/{prompt_path}"))
        .unwrap_or_else(|e| panic!("the Coder prompt must load: {e}"));
    let schema =
        niki::load_asset(schema_path).unwrap_or_else(|e| panic!("the Coder schema must load: {e}"));
    assert!(
        prompt.len() > 500,
        "the Coder prompt is {} bytes — that is not a prompt",
        prompt.len()
    );
    assert!(
        schema.contains("\"edits\""),
        "the Coder schema is not the code-diff schema"
    );
    assert!(
        niki::load_asset(prompt_path).is_err(),
        "this test is meaningless unless a bare name really does fail to load — if that \\
         changes, `load_asset` got friendlier and the call sites should stop prefixing"
    );
}

/// The capability probe must probe something.
///
/// `niki doctor --measure` renders the Coder prompt to test what a model can
/// actually do. It loaded the prompt with a bare name, got an error, and
/// `unwrap_or_default()` turned that into an empty system prompt and an empty
/// schema. The probe was scoring models against a blank. I had previously
/// concluded the probe was "confounded by a trivial ask" — that was the wrong
/// diagnosis, and it was covering for a prompt that was never sent.
#[test]
fn the_capability_probe_renders_a_real_prompt() {
    let probe = niki::agents::render_coder_probe_prompt();
    assert!(
        probe.len() > 500,
        "the probe prompt is {} bytes; a probe with no prompt measures nothing",
        probe.len()
    );
    assert!(
        probe.contains("submit_artifact"),
        "the probe must describe the tool the model has to call"
    );
    assert!(
        probe.contains("edits"),
        "the probe must carry the artifact schema the model has to satisfy"
    );
}

/// A model that wrote the artifact as prose still wrote it.
///
/// Measured on `qwen2.5-coder:3b`, the model this project's README tells
/// first-time users to install: asked to call `submit_artifact`, it produced a
/// correct, schema-valid artifact as a fenced JSON block in the message body
/// and no tool call. The loop threw it away, the stage fell through to a
/// one-shot call, and the run failed with "the model is too small" — which
/// named neither the loop, nor the fact that the model had answered, nor the
/// fact that its answer was right.
///
/// Ollama already recovers *tool calls* that arrive this way. The artifact now
/// gets the same treatment, under the same schema check: prose that merely
/// looks like JSON is still not accepted.
#[test]
fn an_artifact_written_as_prose_is_still_the_artifact() {
    let prose = format!("Here is the change:\n\n```json\n{}\n```\n", artifact());
    let script: Vec<Option<(String, serde_json::Value)>> = vec![None];
    // The scripted agent answers in prose, so drive the loop with one that has
    // no tool call to make.
    let (out, _) = run_with_prose(&prose, script);
    assert_eq!(
        out.artifact,
        Some(artifact()),
        "a schema-valid artifact in the message body is the answer, whatever shape it \
         arrived in"
    );
    assert!(
        out.tool_calls
            .iter()
            .any(|(n, ok)| n == "submit_artifact" && *ok),
        "and the recovery is recorded, so the call log says the artifact arrived"
    );
}

/// Prose that is not a valid artifact is not a submission.
///
/// The recovery is a convenience, not a hole: the same validator that gates a
/// real `submit_artifact` call gates this, so a model that discusses JSON, or
/// emits a half-formed artifact, still gets nothing.
#[test]
fn prose_that_is_not_a_valid_artifact_is_still_refused() {
    let prose = "```json\n{ \"edits\": [] }\n```";
    let (out, _) = run_with_prose(prose, vec![None]);
    assert_eq!(
        out.artifact, None,
        "an artifact with no edits is not an implementation, however it arrived"
    );

    let discussion = "Here is how I would write it: {\"edits\": [";
    let (out, _) = run_with_prose(discussion, vec![None]);
    assert_eq!(
        out.artifact, None,
        "a model discussing JSON is not a model submitting one"
    );
}

/// A response cut off mid-artifact is recoverable, like a truncated tool call.
///
/// Measured on a refactor task: the model produced a correct, schema-shaped
/// artifact and stopped mid-object at the token limit. The loop found no
/// tool call, the recovery found unbalanced braces, and the run fell through
/// to a one-shot call that failed with "Failed to parse artifact JSON" — a
/// message naming neither the truncation nor the fact that the answer was on
/// its way to being right.
///
/// The guard for truncated *tool calls* already existed. This is the same
/// treatment for a truncated answer.
#[test]
fn a_truncated_artifact_is_sent_back_to_be_re_emitted() {
    let truncated = "```json\n{\"edits\": [{\"search\": \"old\", \"replace\": \"new\"}], \
                     \"files_changed\": [{\"path\": \"src/lib.rs\", \"action\": \"modify\", \
                     \"language\": \"rust\"}], \"implementation_notes\": \"renamed the function \
                     and updated every call site, including the one in the module doc, which is";
    let complete = format!("```json\n{}\n```", artifact());

    let (out, prompts) = run_truncated_then_complete(truncated, &complete);
    assert_eq!(
        out.artifact,
        Some(artifact()),
        "the re-emitted artifact is the answer"
    );
    assert_eq!(out.steps, 2, "one truncated answer, one correct re-emit");
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("cut off at the token limit")),
        "the model must be told it was cut off, or it will simply be cut off again: {prompts:?}"
    );
}

/// And when it never fits, the loop says so rather than retrying forever.
#[test]
fn a_truncation_that_never_resolves_is_reported_as_truncation() {
    let truncated = "```json\n{\"edits\": [{\"search\": \"old\", \"replace\": \"new\"}], \
                     \"files_changed\": [{\"path\": \"src/lib.rs\"}], \"implementation_notes\": and \
                     then it kept going";
    let (out, prompts) = run_truncated_then_complete(truncated, truncated);
    assert_eq!(
        out.artifact, None,
        "an artifact that was never completed is not an artifact"
    );
    assert!(
        out.truncated,
        "and the caller has to be able to tell this from a model that simply declined"
    );
    assert!(
        prompts.iter().filter(|p| p.contains("cut off")).count() == 1,
        "exactly one retry: a model that cannot fit the artifact in the budget will not \
         start fitting it on the fourth attempt. {prompts:?}"
    );
}

/// The fallback notice has to name truncation, because the two diagnoses send
/// a user to completely different places — raise the token limit, or change
/// the model.
#[test]
fn the_fallback_notice_distinguishes_truncation_from_a_short_answer() {
    use niki::llm::provider::TokenUsage;
    use niki::runtime::tools::LoopOutput;
    let notice = |truncated: bool| {
        niki::orchestrator::pipeline::coder_loop_fallback_notice(&LoopOutput {
            content: "```json\n{\"edits\": [{\"search\": \"old\", \"repl".into(),
            steps: 1,
            tool_calls: vec![],
            usage: TokenUsage::default(),
            feedback_turns: 0,
            truncated,
            artifact: None,
        })
    };
    assert!(
        notice(true).contains("CUT OFF at the token limit"),
        "a truncated answer must not be reported as an unusable one: {}",
        notice(true)
    );
    assert!(
        !notice(false).contains("CUT OFF"),
        "and a short answer must not be reported as truncated: {}",
        notice(false)
    );
}

/// A response the provider cut off must not be treated as a complete one.
///
/// `StreamChunk` carried only text and usage, so a streaming caller could not
/// tell a finished response from one that stopped at the token limit. The
/// one-shot agent path streams, so it had no truncation guard at all: a
/// half-written artifact went to the JSON repairer and came back as "did not
/// satisfy the artifact requirements", which blames the model for something
/// the token limit did and names none of the real fix.
///
/// Ollama reports `done_reason: "length"` correctly — probed directly against
/// the running server — and the value was being dropped on the floor.
#[tokio::test]
async fn a_truncated_one_shot_response_is_reported_as_truncated() {
    use niki::llm::provider::{
        CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage,
    };

    /// Streams a half-written artifact, then says it was cut off.
    struct Truncated;

    #[async_trait::async_trait]
    impl LlmProvider for Truncated {
        fn provider_name(&self) -> &str {
            "truncated"
        }
        async fn complete(&self, _r: CompletionRequest) -> Result<CompletionResponse> {
            unreachable!("this test drives the streaming path")
        }
        async fn stream(
            &self,
            _r: CompletionRequest,
        ) -> Result<std::pin::Pin<Box<dyn futures::Stream<Item = Result<StreamChunk>> + Send>>>
        {
            Ok(Box::pin(futures::stream::iter(vec![
                Ok(StreamChunk::Text(
                    r#"{"edits": [{"search": "old", "replace": "ne"#.to_string(),
                )),
                Ok(StreamChunk::Finish {
                    reason: "length".to_string(),
                }),
                Ok(StreamChunk::Usage(TokenUsage::default())),
            ])))
        }
    }

    let mut display = niki::display::agent_stream::AgenticDisplay::new();
    let err = niki::agents::run_agent(
        niki::artifacts::types::AgentRole::Coder,
        &Truncated,
        "m",
        "coder.md",
        minijinja::context! {
            input_artifacts => vec![r#"{"summary":"x"}"#.to_string()],
            revision_round => 0,
            project_knowledge => "",
            project_memory => "",
            current_files => "",
            mcp_tools => "",
            tool_loop => false,
        },
        "schemas/code_diff.schema.json",
        &mut display,
        4096,
        0.0,
        None,
    )
    .await
    .expect_err("a response cut off mid-artifact is not an answer");

    let msg = format!("{err:#}");
    assert!(
        msg.contains("truncated"),
        "the failure must say the response was cut off, not that the model was wrong: {msg}"
    );
    assert!(
        !msg.contains("too small"),
        "and must not blame the model for the token limit: {msg}"
    );
}

/// A provider must actually *emit* the stop reason, not merely have somewhere
/// to put it.
///
/// The recogniser (`was_truncated`) is a pure function and a test on it alone
/// passes whether or not any provider ever calls it — a mutation that stops
/// all three streaming providers reporting their stop reason left the suite
/// green. This drives Ollama's real stream over HTTP and asserts a `Finish`
/// chunk comes out, which is the wiring a pure-function test cannot see.
#[tokio::test]
async fn ollama_emits_the_stop_reason_on_its_stream() {
    use niki::config::ProviderConfig;
    use niki::llm::provider::{CompletionRequest, LlmProvider, StreamChunk, ToolCall, ToolSpec};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Two SSE chunks: some text, then the terminating one carrying
    // `done_reason: "length"` — which the live server really sends, probed
    // against `qwen2.5-coder:3b` rather than assumed.
    let sse = concat!(
        "{\"model\":\"qwen2.5-coder:3b\",\"message\":{\"role\":\"assistant\",\"content\":\"partial\"},\"done\":false}\n",
        "{\"model\":\"qwen2.5-coder:3b\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"done_reason\":\"length\",\"eval_count\":7}\n"
    );
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .mount(&server)
        .await;

    let provider = niki::llm::ollama::OllamaProvider::new(&ProviderConfig {
        api_key: Some("ollama".into()),
        base_url: Some(server.uri()),
        default_model: "qwen2.5-coder:3b".into(),
    })
    .expect("provider");

    let mut stream = provider
        .stream(CompletionRequest {
            model: "qwen2.5-coder:3b".into(),
            user_message: "hi".into(),
            tools: Some(vec![ToolSpec {
                name: "read".into(),
                description: "read".into(),
                parameters: serde_json::json!({"type": "object"}),
            }]),
            ..Default::default()
        })
        .await
        .expect("stream");

    use futures::StreamExt;
    let mut finish: Option<String> = None;
    let mut text = String::new();
    let _ = ToolCall {
        id: String::new(),
        name: String::new(),
        arguments: serde_json::Value::Null,
    };
    while let Some(chunk) = stream.next().await {
        match chunk.expect("chunk") {
            StreamChunk::Text(t) => text.push_str(&t),
            StreamChunk::Finish { reason } => finish = Some(reason),
            StreamChunk::Usage(_) => {}
        }
    }

    assert_eq!(text, "partial", "the text still arrives");
    assert_eq!(
        finish.as_deref(),
        Some("length"),
        "the stream must carry the provider's stop reason — without it the guard \
         above is unreachable on this path"
    );
    assert!(
        niki::runtime::tools::was_truncated(finish.as_deref()),
        "and the guard must read it"
    );
}

/// The catalogue must be *fetched*, against the wire shape a real provider
/// sends.
///
/// This is the feature behind the claim that third-party providers are natively
/// supported. It did not exist: `niki recommend` carries a hardcoded table of
/// `claude-opus-4` / `gpt-4o-mini`, which cannot know what an account can
/// reach, and on OpenRouter it is wrong by construction — several hundred
/// models, fully-qualified names, and a per-model effort control the name tells
/// you nothing about.
///
/// Driven over HTTP rather than against a parsed struct, because the parts
/// that break are the parts a unit test cannot see: the URL, the auth header,
/// and what a provider does when the key is wrong.
#[tokio::test]
async fn a_provider_catalogue_is_fetched_over_the_wire() {
    use wiremock::matchers::{header_regex, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    // The OpenAI-compatible shape, including a `:free` tier and a reasoning
    // model — the two axes a user picking from OpenRouter actually cares about.
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .and(header_regex("authorization", "^Bearer sk-or-v1-.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {"id": "anthropic/claude-sonnet-4",
                 "pricing": {"prompt": "0.000003", "completion": "0.000015"}},
                {"id": "openai/o3-mini",
                 "pricing": {"prompt": "0.0000011", "completion": "0.0000044"}},
                {"id": "meta-llama/llama-3.3-70b-instruct:free",
                 "pricing": {"prompt": "0", "completion": "0"}}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let models = niki::cli::catalogue::fetch(
        "openrouter",
        Some(&format!("{}/api/v1", server.uri())),
        Some("sk-or-v1-test"),
    )
    .await
    .expect("the catalogue is fetched");

    assert_eq!(models.len(), 3);
    assert_eq!(models[0].id, "anthropic/claude-sonnet-4");
    assert_eq!(models[0].vendor(), Some("anthropic"));
    assert_eq!(models[0].price_per_mtok, Some((Some(3.0), Some(15.0))));
    assert!(
        models[1]
            .traits
            .contains(&niki::cli::catalogue::ModelTrait::Reasoning),
        "o3 takes an effort control, and that is the axis a user needs flagged"
    );
    assert!(
        models[2]
            .traits
            .contains(&niki::cli::catalogue::ModelTrait::Free),
        "and a :free tier is the other one"
    );
}

/// A key without catalogue access must not look like a provider with no models.
#[tokio::test]
async fn a_refused_key_is_an_error_not_an_empty_catalogue() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": "No auth credentials found"
        })))
        .mount(&server)
        .await;

    let err = niki::cli::catalogue::fetch(
        "openrouter",
        Some(&format!("{}/api/v1", server.uri())),
        Some("sk-or-v1-wrong"),
    )
    .await
    .expect_err("403 is not an empty catalogue");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("403"),
        "the user must see what the provider said, not a generic failure: {msg}"
    );
}

/// A missing key must say how to set it — the first-hour-error rule.
#[tokio::test]
async fn a_catalogue_without_a_key_explains_how_to_set_one() {
    let err = niki::cli::catalogue::fetch("openrouter", Some("https://example.invalid/v1"), None)
        .await
        .expect_err("no key is an error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("OPENROUTER_API_KEY"),
        "and must name the variable, or the user is left guessing: {msg}"
    );
    assert!(
        msg.contains("auth login"),
        "and the command that sets it: {msg}"
    );
}

/// Reasoning effort reaches the wire — and stays off unless asked for.
///
/// Two things have to be true, and the second is the dangerous one:
///
/// * when set, `reasoning_effort` is in the request body, because on a
///   reasoning model it is what drives both price and latency, and a
///   harness that cannot set it is leaving the most important dial
///   untouched;
/// * when unset, the key is **absent**, not null. A provider that does not
///   know the field may reject the whole request, so a harness that sent it
///   by default would turn every run on every non-reasoning provider into a
///   400 — and it would look like the provider was broken.
///
/// The value is never inferred from a model name. The accepted range is a
/// property of the model, so guessing it is exactly the failure above.
#[tokio::test]
async fn reasoning_effort_reaches_the_wire_only_when_set() {
    use niki::config::ProviderConfig;
    use niki::llm::provider::{CompletionRequest, LlmProvider, TokenUsage};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .mount(&server)
        .await;

    let provider = niki::llm::openai::OpenAiProvider::new_named(
        &ProviderConfig {
            api_key: Some("k".into()),
            base_url: Some(server.uri()),
            default_model: "o3-mini".into(),
        },
        "openrouter",
    )
    .expect("provider");

    let request = |effort: Option<&str>| CompletionRequest {
        model: "o3-mini".into(),
        user_message: "hi".into(),
        reasoning_effort: effort.map(str::to_string),
        ..Default::default()
    };

    provider
        .complete(request(Some("high")))
        .await
        .expect("completes");
    provider.complete(request(None)).await.expect("completes");

    let bodies: Vec<serde_json::Value> = server
        .received_requests()
        .await
        .expect("requests")
        .iter()
        .map(|r| serde_json::from_slice(&r.body).expect("json"))
        .collect();
    assert_eq!(bodies.len(), 2, "both requests recorded");

    assert_eq!(
        bodies[0]["reasoning_effort"],
        serde_json::json!("high"),
        "a set effort must reach the provider"
    );
    assert!(
        bodies[1].get("reasoning_effort").is_none(),
        "an unset effort must be absent, not null — a provider that does not know the \\
         field may reject the whole request. Got: {}",
        bodies[1]
    );

    let _ = TokenUsage::default();
}
