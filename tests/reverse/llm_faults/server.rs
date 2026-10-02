//! A wiremock-backed LLM endpoint that can be told to misbehave, and that
//! records what the model actually received.
//!
//! Two capabilities the in-process `MockProvider` cannot provide:
//!
//! 1. **Real HTTP semantics** — status codes, headers, truncated bodies,
//!    connection-level truncation. Every provider path in niki goes through
//!    this transport, so a fault injected here covers all of them at once,
//!    with no production code change.
//! 2. **Request capture** — a test can assert on the exact bytes the model saw.
//!    This is what turns "I think we sent the schema" into a fact, and it is
//!    the precondition for the context-snapshot harness.

use serde_json::Value;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::sse;

/// A fault the server can be configured with. Every variant names the pipeline
/// behaviour it is designed to catch, and must be paired with a test that
/// either proves recovery or proves the pipeline fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// 429 with a `Retry-After` header, `times` times, then healthy.
    /// Catches: backoff that ignores the header.
    RateLimitRetryAfter { times: usize, retry_after_secs: u32 },
    /// 5xx, `times` times, then healthy. Catches: retry budget not consumed.
    ServerError { times: usize, code: u16 },
    /// 401. Catches: fatal-vs-transient misclassification (must not retry).
    AuthFailure,
    /// A valid SSE stream cut off mid-JSON.
    /// Catches: partial JSON being accepted as a complete message.
    TruncatedStream { cut_at: usize },
    /// A stream that never terminates — no `finish_reason`, no `[DONE]`.
    /// Catches: missing turn/stream timeout.
    StreamNeverEnds,
    /// Well-formed frames carrying zero content.
    /// Catches: an empty artifact being treated as success.
    EmptyContent,
    /// The model answers in prose instead of JSON.
    /// Catches: a schema gate bypassed by a conversational wrapper.
    ProseNotJson,
    /// Syntactically valid against the artifact schema, semantically empty.
    /// Catches: a no-op recorded as a successful run. This is the highest-value
    /// fault in the set — the shipped schemas have no `minItems`, so today it
    /// passes validation cleanly.
    ValidSchemaEmptySemantics,
    /// A tool call whose `arguments` are not valid JSON.
    /// Catches: a parse panic or a silently dropped call.
    MalformedToolCall,
    /// A tool call naming a tool that is not in the registry.
    /// Catches: unknown tools being dispatched anyway.
    UnknownToolName,
    /// The model repeats the same tool call forever.
    /// Catches: an unenforced step budget.
    InfiniteToolLoop,
    /// A review verdict of `approved` alongside blocking issues.
    /// Catches: a verdict not derived from its own issue list.
    VerdictContradictsIssues,
    /// A response far larger than any sane context budget.
    /// Catches: unbounded context growth.
    OversizedResponse { bytes: usize },
    /// The planner's artifact returned to the coder.
    /// Catches: a missing role/type check on the stage handoff.
    WrongRoleArtifact,
    /// Red challenges emitted with an empty `red_reconciliation`.
    /// Catches: the Red/Blue "reconcile every challenge" rule being prompt-only.
    ChallengesNeverReconciled,
}

impl Fault {
    /// Every fault, for catalogue completeness tests.
    pub fn all() -> Vec<Fault> {
        vec![
            Fault::RateLimitRetryAfter {
                times: 2,
                retry_after_secs: 1,
            },
            Fault::ServerError {
                times: 2,
                code: 500,
            },
            Fault::AuthFailure,
            Fault::TruncatedStream { cut_at: 40 },
            Fault::StreamNeverEnds,
            Fault::EmptyContent,
            Fault::ProseNotJson,
            Fault::ValidSchemaEmptySemantics,
            Fault::MalformedToolCall,
            Fault::UnknownToolName,
            Fault::InfiniteToolLoop,
            Fault::VerdictContradictsIssues,
            Fault::OversizedResponse { bytes: 2_000_000 },
            Fault::WrongRoleArtifact,
            Fault::ChallengesNeverReconciled,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Fault::RateLimitRetryAfter { .. } => "rate_limit_429_retry_after",
            Fault::ServerError { .. } => "server_5xx_transient",
            Fault::AuthFailure => "auth_401_fatal",
            Fault::TruncatedStream { .. } => "truncated_sse_mid_json",
            Fault::StreamNeverEnds => "stream_never_ends",
            Fault::EmptyContent => "empty_content",
            Fault::ProseNotJson => "prose_not_json",
            Fault::ValidSchemaEmptySemantics => "valid_schema_empty_semantics",
            Fault::MalformedToolCall => "tool_call_malformed_json",
            Fault::UnknownToolName => "tool_call_unknown_name",
            Fault::InfiniteToolLoop => "infinite_tool_loop",
            Fault::VerdictContradictsIssues => "verdict_contradicts_issues",
            Fault::OversizedResponse { .. } => "oversized_response",
            Fault::WrongRoleArtifact => "wrong_role_artifact",
            Fault::ChallengesNeverReconciled => "challenge_never_reconciled",
        }
    }
}

/// A mounted LLM endpoint that records every request body it served.
pub struct FaultServer {
    pub server: MockServer,
    /// Set to true by the matcher when a fault was actually served. A fault
    /// that never fired must be discarded, not counted as a pass — otherwise
    /// impact is systematically underestimated.
    fired: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl FaultServer {
    pub async fn start(faults: &[Fault]) -> Self {
        let server = MockServer::start().await;
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        for fault in faults {
            mount_fault(&server, fault, &fired).await;
        }
        FaultServer { server, fired }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// Every request body the server received, in order.
    pub async fn requests(&self) -> Vec<Value> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter_map(|r| serde_json::from_slice(&r.body).ok())
            .collect()
    }

    /// The single request the server received. Panics if there was not exactly
    /// one — a test that meant to inspect the prompt must not silently inspect
    /// whichever of several happened to be last.
    pub async fn single_request(&self) -> Value {
        let all = self.requests().await;
        assert_eq!(
            all.len(),
            1,
            "expected exactly one request, got {} — use requests() for multi-turn tests",
            all.len()
        );
        all.into_iter().next().unwrap()
    }

    /// The JSON body of the outbound request, for asserting on what the model
    /// was actually shown.
    pub async fn request_body(&self) -> Value {
        self.single_request().await
    }

    /// The system prompt string the model was sent, if the provider used one.
    pub async fn system_prompt(&self) -> Option<String> {
        fn walk(v: &Value) -> Option<String> {
            match v {
                Value::String(s) => Some(s.clone()),
                Value::Array(a) => a.iter().find_map(walk),
                Value::Object(o) => {
                    for key in ["system", "instructions"] {
                        if let Some(found) = o.get(key).and_then(walk) {
                            return Some(found);
                        }
                    }
                    // A chat message with role=system carries the prompt.
                    if o.get("role").and_then(|r| r.as_str()) == Some("system")
                        && let Some(c) = o.get("content")
                    {
                        return walk(c);
                    }
                    o.values().find_map(walk)
                }
                _ => None,
            }
        }
        walk(&self.request_body().await)
    }

    /// The JSON schema the model was constrained to, when the provider sent one.
    pub async fn sent_schema(&self) -> Option<Value> {
        fn walk(v: &Value) -> Option<Value> {
            match v {
                Value::Array(a) => a.iter().find_map(walk),
                Value::Object(o) => {
                    if let Some(r) = o
                        .get("response_format")
                        .and_then(|rf| rf.get("json_schema"))
                    {
                        return Some(r.clone());
                    }
                    if o.contains_key("type")
                        && (o["type"] == "json_schema" || o["type"] == "object")
                    {
                        return Some(v.clone());
                    }
                    o.values().find_map(walk)
                }
                _ => None,
            }
        }
        walk(&self.request_body().await)
    }

    /// Whether the mounted fault was actually served. Always assert this: an
    /// untriggered fault is not a pass.
    pub fn fault_fired(&self) -> bool {
        self.fired.load(std::sync::atomic::Ordering::SeqCst)
    }
}

fn mark(fired: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    fired.store(true, std::sync::atomic::Ordering::SeqCst);
}

const MODEL: &str = "fault-model";

async fn mount_fault(
    server: &MockServer,
    fault: &Fault,
    fired: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::Arc;
    use std::sync::atomic::Ordering::SeqCst;

    let template: ResponseTemplate = match fault {
        Fault::RateLimitRetryAfter {
            times,
            retry_after_secs,
        } => {
            // One mount with a decrementing counter. Registering N separate
            // `up_to_n_times(1)` mocks made the result depend on wiremock's
            // match ordering; this is deterministic.
            let fired = Arc::clone(fired);
            let retry_after_secs = *retry_after_secs;
            let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(*times));
            Mock::given(method("POST"))
                .respond_with(move |_req: &wiremock::Request| {
                    // `fetch_update` is deprecated on current Rust (renamed
                    // `try_update`), and CI's clippy runs the latest stable and
                    // fails on it. `try_update` is not in `rust-version = 1.88`,
                    // which this crate declares and CI enforces with its own MSRV
                    // job — so the rename is not available to us yet.
                    #[allow(deprecated, reason = "try_update is post-1.88")]
                    if remaining
                        .fetch_update(SeqCst, SeqCst, |n| n.checked_sub(1))
                        .is_ok()
                    {
                        mark(&fired);
                        ResponseTemplate::new(429)
                            .insert_header("Retry-After", retry_after_secs.to_string())
                            .set_body_json(serde_json::json!({
                                "error": {"message": "rate limited", "type": "rate_limit_error"}
                            }))
                    } else {
                        healthy_sse()
                    }
                })
                .mount(server)
                .await;
            return;
        }
        Fault::ServerError { times, code } => {
            let fired = Arc::clone(fired);
            let code = *code;
            let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(*times));
            Mock::given(method("POST"))
                .respond_with(move |_req: &wiremock::Request| {
                    // `fetch_update` is deprecated on current Rust (renamed
                    // `try_update`), and CI's clippy runs the latest stable and
                    // fails on it. `try_update` is not in `rust-version = 1.88`,
                    // which this crate declares and CI enforces with its own MSRV
                    // job — so the rename is not available to us yet.
                    #[allow(deprecated, reason = "try_update is post-1.88")]
                    if remaining
                        .fetch_update(SeqCst, SeqCst, |n| n.checked_sub(1))
                        .is_ok()
                    {
                        mark(&fired);
                        ResponseTemplate::new(code).set_body_json(serde_json::json!({
                            "error": {"message": "upstream unavailable"}
                        }))
                    } else {
                        healthy_sse()
                    }
                })
                .mount(server)
                .await;
            return;
        }
        Fault::AuthFailure => {
            mark(fired);
            ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {"message": "invalid api key", "type": "authentication_error"}
            }))
        }
        Fault::TruncatedStream { cut_at } => {
            mark(fired);
            let body = sse::truncate_at(&healthy_sse_body(), *cut_at);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        }
        Fault::StreamNeverEnds => {
            mark(fired);
            // Valid frames, but no terminating event and no [DONE].
            let body = sse::frames(
                &[
                    sse::openai::ev_created("r-open", MODEL),
                    sse::openai::ev_delta("r-open", MODEL, "thinking"),
                ],
                None,
            );
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        }
        Fault::EmptyContent => {
            mark(fired);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-empty", MODEL),
                        sse::openai::ev_completed("r-empty", MODEL, Some((12, 0))),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::ProseNotJson => {
            mark(fired);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-prose", MODEL),
                        sse::openai::ev_delta(
                            "r-prose",
                            MODEL,
                            "Sure! Here's the diff you asked for:\n\n```json\n{}\n```\nLet me know if you need changes.",
                        ),
                        sse::openai::ev_completed("r-prose", MODEL, Some((20, 40))),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::ValidSchemaEmptySemantics => {
            mark(fired);
            // Structurally valid against every shipped schema, and a no-op.
            let body = serde_json::json!({
                "edits": [],
                "files_changed": [],
                "implementation_notes": "",
                "spec_adherence": ""
            })
            .to_string();
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-noop", MODEL),
                        sse::openai::ev_delta("r-noop", MODEL, &body),
                        sse::openai::ev_completed("r-noop", MODEL, Some((12, 12))),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::MalformedToolCall => {
            mark(fired);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-bad", MODEL),
                        sse::openai::ev_tool_call_start("r-bad", MODEL, "call_1", "read_file"),
                        sse::openai::ev_tool_call_args("r-bad", MODEL, "call_1", "{\"path\": "),
                        sse::openai::ev_tool_call_done("r-bad", MODEL, "call_1"),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::UnknownToolName => {
            mark(fired);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-unk", MODEL),
                        sse::openai::ev_tool_call_start(
                            "r-unk",
                            MODEL,
                            "call_x",
                            "definitely_not_a_registered_tool",
                        ),
                        sse::openai::ev_tool_call_args("r-unk", MODEL, "call_x", "{}"),
                        sse::openai::ev_tool_call_done("r-unk", MODEL, "call_x"),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::InfiniteToolLoop => {
            // Not marked fired: the loop must be stopped by niki's step budget,
            // not by the stream ending. Marking here would credit a fault that
            // the test only observed because the client gave up.
            let body = sse::frames(
                &[
                    sse::openai::ev_created("r-loop", MODEL),
                    sse::openai::ev_tool_call_start("r-loop", MODEL, "call_loop", "read_file"),
                    sse::openai::ev_tool_call_args("r-loop", MODEL, "call_loop", "{}"),
                    sse::openai::ev_tool_call_done("r-loop", MODEL, "call_loop"),
                ],
                Some("[DONE]"),
            );
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        }
        Fault::VerdictContradictsIssues => {
            mark(fired);
            let body = serde_json::json!({
                "verdict": "approved",
                "summary": "Looks good.",
                "issues": [
                    {"severity": "blocking", "category": "security",
                     "description": "Command injection in the exec path", "file": "src/x.rs", "line": 10},
                    {"severity": "blocking", "category": "correctness",
                     "description": "Off-by-one in pagination", "file": "src/y.rs", "line": 4}
                ],
                "strengths": []
            })
            .to_string();
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-vc", MODEL),
                        sse::openai::ev_delta("r-vc", MODEL, &body),
                        sse::openai::ev_completed("r-vc", MODEL, Some((30, 90))),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::OversizedResponse { bytes } => {
            mark(fired);
            let filler = "A".repeat(*bytes);
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-big", MODEL),
                        sse::openai::ev_delta("r-big", MODEL, &filler),
                        sse::openai::ev_completed("r-big", MODEL, Some((10, *bytes as u64))),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::WrongRoleArtifact => {
            mark(fired);
            // A task_spec where a code_diff is expected.
            let body = serde_json::json!({
                "summary": "Fix pagination",
                "files_to_modify": [{"path": "src/list.rs", "reason": "off-by-one"}],
                "acceptance_criteria": ["tests pass"]
            })
            .to_string();
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-wr", MODEL),
                        sse::openai::ev_delta("r-wr", MODEL, &body),
                        sse::openai::ev_completed("r-wr", MODEL, Some((15, 25))),
                    ],
                    Some("[DONE]"),
                ))
        }
        Fault::ChallengesNeverReconciled => {
            mark(fired);
            let body = serde_json::json!({
                "challenges": [
                    {"category": "security", "challenge": "SQL injection in user lookup"},
                    {"category": "logic", "challenge": "Off-by-one on empty input"}
                ],
                "red_reconciliation": []
            })
            .to_string();
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse::frames(
                    &[
                        sse::openai::ev_created("r-nr", MODEL),
                        sse::openai::ev_delta("r-nr", MODEL, &body),
                        sse::openai::ev_completed("r-nr", MODEL, Some((20, 40))),
                    ],
                    Some("[DONE]"),
                ))
        }
    };

    Mock::given(method("POST"))
        .respond_with(template)
        .mount(server)
        .await;
}

/// A well-formed, complete, schema-valid SSE body — the "healthy" case every
/// fault is defined relative to.
fn healthy_sse_body() -> String {
    sse::frames(
        &[
            sse::openai::ev_created("r-ok", MODEL),
            sse::openai::ev_delta(
                "r-ok",
                MODEL,
                "{\"verdict\":\"approved\",\"summary\":\"ok\",\"issues\":[],\"strengths\":[]}",
            ),
            sse::openai::ev_completed("r-ok", MODEL, Some((10, 12))),
        ],
        Some("[DONE]"),
    )
}

fn healthy_sse() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(healthy_sse_body())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fault_has_a_unique_name() {
        let mut names: Vec<&str> = Fault::all().iter().map(|f| f.name()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "fault names must be unique");
    }

    #[tokio::test]
    async fn server_records_the_request_body() {
        let server = FaultServer::start(&[Fault::ProseNotJson]).await;
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/chat/completions", server.uri()))
            .json(&serde_json::json!({
                "model": "m",
                "messages": [{"role": "system", "content": "you are a planner"}],
                "stream": true
            }))
            .send()
            .await
            .expect("mock serves");
        assert!(resp.status().is_success());

        let body = server.request_body().await;
        assert_eq!(body["model"], "m");
        assert_eq!(
            server.system_prompt().await.as_deref(),
            Some("you are a planner"),
            "the system prompt must be recoverable from the captured request"
        );
    }

    #[tokio::test]
    async fn a_fault_that_fired_is_reported() {
        let server = FaultServer::start(&[Fault::AuthFailure]).await;
        let _ = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", server.uri()))
            .json(&serde_json::json!({"model": "m"}))
            .send()
            .await;
        assert!(
            server.fault_fired(),
            "a mounted fault that was served must be reported as fired"
        );
    }

    #[tokio::test]
    async fn rate_limit_recovers_after_the_configured_count() {
        let server = FaultServer::start(&[Fault::RateLimitRetryAfter {
            times: 2,
            retry_after_secs: 0,
        }])
        .await;
        let client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", server.uri());

        let r1 = client
            .post(&url)
            .json(&serde_json::json!({"model": "m"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r1.status().as_u16(), 429, "first call must be rate limited");
        assert_eq!(
            r1.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
            Some("0"),
            "Retry-After must reach the client so backoff can honour it"
        );

        let r2 = client
            .post(&url)
            .json(&serde_json::json!({"model": "m"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r2.status().as_u16(), 429);
        let r3 = client
            .post(&url)
            .json(&serde_json::json!({"model": "m"}))
            .send()
            .await
            .unwrap();
        assert!(r3.status().is_success(), "third call must recover");
        assert!(server.fault_fired());
    }

    #[tokio::test]
    async fn single_request_panics_on_multi_turn() {
        let server = FaultServer::start(&[]).await;
        let client = reqwest::Client::new();
        let url = format!("{}/v1/chat/completions", server.uri());
        for _ in 0..2 {
            let _ = client
                .post(&url)
                .json(&serde_json::json!({"model": "m"}))
                .send()
                .await;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async { server.single_request().await })
        }));
        assert!(
            result.is_err(),
            "single_request must reject more than one request"
        );
    }
}
