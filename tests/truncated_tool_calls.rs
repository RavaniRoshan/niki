//! A response cut off at the token limit carries tool-call arguments that are
//! silently half-written JSON.
//!
//! Executing those is the worst kind of wrong: a `write` with a truncated
//! path, a `bash` with a truncated command — and a run that *looks* successful,
//! because the tool returned Ok. There is no error to notice afterwards.
//!
//! Codex guards against this by failing every tool call carried by a message
//! that stopped on `length` (`agent-loop.ts:263-269`,
//! `failToolCallsFromTruncatedMessage`). NIKI could not: `CompletionResponse`
//! did not carry the reason at all, so the guard had nothing to test. It does
//! now, and this is the guard's test.

use anyhow::Result;
use niki::llm::provider::{
    CompletionRequest, CompletionResponse, LlmProvider, StreamChunk, TokenUsage,
};
use niki::runtime::run_tool_loop;
use niki::runtime::tools::was_truncated;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A provider that always asks for one tool call, with whatever arguments it is
/// told to, and claims whatever finish reason it is told to.
struct TruncatingProvider {
    finish_reason: String,
    calls: Arc<AtomicUsize>,
    /// A path INSIDE the project directory.
    ///
    /// The first version of this test pointed the truncated call at
    /// `/etc/pas`, and passed — because `resolve_tool_path` refuses paths
    /// outside the project, not because the truncation guard did anything. The
    /// test would have kept passing with the guard deleted. This has to be a
    /// path the sandbox would otherwise happily write to, so the only thing
    /// standing between the model and the filesystem is the guard.
    target: String,
}

#[async_trait::async_trait]
impl LlmProvider for TruncatingProvider {
    fn provider_name(&self) -> &str {
        "truncating"
    }

    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk>> + Send>>> {
        // The guard is on the non-streaming path, which is the one the tool
        // loop uses. Unimplemented rather than faked: a double that pretends
        // to stream would let a future refactor move the loop onto the
        // streaming path without anything noticing.
        unimplemented!("the tool loop uses complete(); see the guard there")
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n > 0 {
            // Second turn: stop asking for tools so the loop ends.
            return Ok(CompletionResponse {
                content: "done".into(),
                model: request.model.clone(),
                usage: TokenUsage::default(),
                tool_calls: Vec::new(),
                finish_reason: Some("stop".into()),
            });
        }
        Ok(CompletionResponse {
            content: String::new(),
            model: request.model.clone(),
            usage: TokenUsage::default(),
            tool_calls: vec![niki::llm::provider::ToolCall {
                id: "call_1".into(),
                // Truncated mid-JSON, as a stream cut at the token limit produces.
                name: "write".into(),
                arguments: serde_json::json!({ "path": self.target, "content": "trunca" }),
            }],
            finish_reason: Some(self.finish_reason.clone()),
        })
    }
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
        // Empty = block-all, the shipped default.
        network_allowlist: Vec::new(),
        task_store: None,
        human_input: None,
    }
}

/// Run the loop with a provider that reports `reason`, and report what
/// actually happened: how many model turns, and whether the file was written.
fn run_with(reason: &str) -> (usize, bool) {
    let calls = Arc::new(AtomicUsize::new(0));
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("victim.txt");
    let provider = TruncatingProvider {
        finish_reason: reason.to_string(),
        calls: calls.clone(),
        target: target.display().to_string(),
    };
    // The REAL baseline registry. The first version used ToolRegistry::new(),
    // which is empty, so the write tool did not exist and nothing was written
    // whether or not the guard was there — the test passed with the guard
    // deleted.
    let registry = niki::runtime::build_baseline_registry();
    let mut c = ctx();
    c.project_path = dir.path().to_path_buf();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let out = rt.block_on(async {
        run_tool_loop(
            &provider,
            "m",
            &registry,
            &c,
            vec![niki::runtime::LoopMessage::User("go".into())],
            None,
            4,
            None,
            None,
        )
        .await
    });
    assert!(out.is_ok(), "loop should not error: {out:?}");
    (calls.load(Ordering::SeqCst), target.exists())
}

#[test]
fn a_truncated_response_never_executes_its_tool_calls() {
    for reason in [
        "length",
        "max_tokens",
        "MAX_TOKENS",
        "eval_limit",
        "token_limit",
    ] {
        let (turns, wrote) = run_with(reason);
        assert!(
            !wrote,
            "a tool call carried by a response that stopped on {reason:?} must not execute — \
             its arguments are truncated JSON, and a half-written path or command is the worst \
             kind of wrong because it looks successful"
        );
        assert!(
            turns >= 2,
            "and the model must get a chance to re-issue it, not be left hanging"
        );
    }
}

#[test]
fn a_complete_response_still_executes() {
    // The other half, and the one that makes the test above mean something: a
    // guard that refuses everything is a harness that cannot do anything. With
    // the real baseline registry and a `stop` reason the write happens — which
    // is why the truncated cases are a real distinction and not a tool that was
    // never registered.
    let (turns, wrote) = run_with("stop");
    assert!(wrote, "a complete response must be allowed to write");
    assert_eq!(turns, 2, "the loop ran both turns");
}

#[test]
fn a_provider_that_reports_no_reason_is_not_assumed_to_be_truncated() {
    // A provider that does not report a reason must not have every one of its
    // tool calls refused — that would break it entirely rather than safely.
    for reason in [
        None,
        Some("stop"),
        Some("tool_calls"),
        Some("end_turn"),
        Some("done"),
    ] {
        assert!(!was_truncated(reason), "{reason:?} is not a truncation");
    }
    assert!(was_truncated(Some("length")));
    assert!(
        was_truncated(Some("  max_tokens  ")),
        "comparison must be forgiving"
    );
}
