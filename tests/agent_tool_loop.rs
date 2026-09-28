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
