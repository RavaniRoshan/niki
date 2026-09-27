//! Typed SSE constructors.
//!
//! Modelled on `codex-rs/core/tests/common/responses.rs`, where the SSE payload
//! is built with typed helpers so a test reads declaratively instead of
//! hand-assembling JSON strings.
//!
//! The existing in-process `MockProvider` (`src/llm/mock.rs`) cannot exercise
//! any of this: it returns `tool_calls: Vec::new()`, hardcodes
//! `supports_structured_output() = true`, ignores the JSON schema entirely, and
//! models no HTTP semantics at all. These constructors are what make the
//! transport layer testable.

/// OpenAI chat-completions streaming events.
pub mod openai {
    use serde_json::{Value, json};

    /// `data: {...}` frame, exactly as it goes on the wire (no trailing newline;
    /// [`frames`] adds the separators).
    pub fn ev_created(id: &str, model: &str) -> String {
        json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 0,
            "model": model,
            "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]
        })
        .to_string()
    }

    pub fn ev_delta(id: &str, model: &str, text: &str) -> String {
        json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 0,
            "model": model,
            "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": null}]
        })
        .to_string()
    }

    /// A tool call, split the way the real API splits it: the first delta
    /// carries the name, subsequent deltas carry argument fragments. Getting
    /// this wrong is exactly the class of bug the fragment-splitting fault
    /// below is designed to catch.
    pub fn ev_tool_call_start(id: &str, model: &str, call_id: &str, name: &str) -> String {
        json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 0,
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": call_id,
                        "type": "function",
                        "function": {"name": name, "arguments": ""}
                    }]
                },
                "finish_reason": null
            }]
        })
        .to_string()
    }

    pub fn ev_tool_call_args(id: &str, model: &str, call_id: &str, args_fragment: &str) -> String {
        json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 0,
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": call_id,
                        "type": "function",
                        "function": {"arguments": args_fragment}
                    }]
                },
                "finish_reason": null
            }]
        })
        .to_string()
    }

    /// The terminal frame. `finish_reason` names the completion reason; the
    /// call id is not repeated here because the API omits it on the final chunk.
    pub fn ev_tool_call_done(id: &str, model: &str, _call_id: &str) -> String {
        json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 0,
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "tool_calls"
            }]
        })
        .to_string()
    }

    pub fn ev_completed(id: &str, model: &str, usage: Option<(u64, u64)>) -> String {
        let mut v = json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 0,
            "model": model,
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        });
        if let Some((prompt, completion)) = usage {
            v["usage"] = json!({
                "prompt_tokens": prompt,
                "completion_tokens": completion,
                "total_tokens": prompt + completion
            });
        }
        v.to_string()
    }

    /// A non-streaming JSON body, for the `stream: false` path.
    pub fn completion_body(model: &str, content: &str, usage: (u64, u64)) -> Value {
        json!({
            "id": "cmpl-fault",
            "object": "chat.completion",
            "created": 0,
            "model": model,
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": usage.0,
                "completion_tokens": usage.1,
                "total_tokens": usage.0 + usage.1
            }
        })
    }
}

/// Anthropic messages-streaming events.
pub mod anthropic {
    use serde_json::json;

    pub fn ev_message_start(model: &str) -> String {
        format!(
            "event: message_start\ndata: {}",
            json!({
                "type": "message_start",
                "message": {
                    "id": "msg_fault",
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            })
        )
    }

    pub fn ev_text_delta(text: &str) -> String {
        format!(
            "event: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}",
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}})
        )
    }

    pub fn ev_tool_use(name: &str, input_json: &str) -> String {
        format!(
            "event: content_block_start\ndata: {}\n\nevent: content_block_delta\ndata: {}",
            json!({
                "type": "content_block_start",
                "index": 1,
                "content_block": {"type": "tool_use", "id": "toolu_fault", "name": name, "input": {}}
            }),
            json!({
                "type": "content_block_delta",
                "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": input_json}
            })
        )
    }

    pub fn ev_message_stop() -> String {
        format!(
            "event: message_stop\ndata: {}",
            json!({"type": "message_stop"})
        )
    }
}

/// Join raw event bodies into a well-formed SSE body.
///
/// `terminator` controls the tail: `"done"` appends `data: [DONE]`, `None`
/// leaves the stream open (used by the `stream_never_ends` fault), and
/// `Some("")` emits a bare `data:` with an empty payload.
pub fn frames(events: &[String], terminator: Option<&str>) -> String {
    let mut out = String::new();
    for e in events {
        out.push_str("data: ");
        out.push_str(e);
        out.push_str("\n\n");
    }
    if let Some(t) = terminator {
        out.push_str("data: ");
        out.push_str(t);
        out.push_str("\n\n");
    }
    out
}

/// Truncate a valid SSE body at an arbitrary byte offset, simulating a
/// connection that dies mid-stream. A correct parser must either reassemble
/// the original or error — never produce a *complete but wrong* message.
pub fn truncate_at(body: &str, byte: usize) -> String {
    let b = body.as_bytes();
    let cut = byte.min(b.len());
    // Respect UTF-8 boundaries so the fixture itself stays valid UTF-8.
    let mut cut = cut;
    while cut > 0 && !body.is_char_boundary(cut) {
        cut -= 1;
    }
    String::from_utf8_lossy(&b[..cut]).to_string()
}

/// Split a body at an arbitrary byte offset, yielding two responses that a
/// load balancer or a flaky proxy could hand to a client. Exercises streaming
/// resumption across a chunk boundary.
pub fn split_at(body: &str, byte: usize) -> (String, String) {
    let b = body.as_bytes();
    let mut cut = byte.min(b.len());
    while cut > 0 && !body.is_char_boundary(cut) {
        cut -= 1;
    }
    (
        String::from_utf8_lossy(&b[..cut]).to_string(),
        String::from_utf8_lossy(&b[cut..]).to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_well_formed_sse() {
        let body = frames(
            &[
                openai::ev_created("r1", "m"),
                openai::ev_delta("r1", "m", "hello"),
                openai::ev_completed("r1", "m", Some((10, 3))),
            ],
            Some("[DONE]"),
        );
        assert!(body.starts_with("data: "));
        assert!(body.ends_with("data: [DONE]\n\n"));
        // Every non-terminator line pair must parse as JSON.
        for line in body.lines().filter(|l| l.starts_with("data: ")) {
            let payload = line.trim_start_matches("data: ").trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            serde_json::from_str::<serde_json::Value>(payload)
                .unwrap_or_else(|e| panic!("frame is not valid JSON: {payload} — {e}"));
        }
    }

    #[test]
    fn truncate_never_panics_and_respects_utf8() {
        let body = frames(
            &[openai::ev_delta("r1", "m", "héllo ✅ 世界")],
            Some("[DONE]"),
        );
        for i in 0..=body.len() {
            let t = truncate_at(&body, i);
            assert!(t.len() <= body.len());
            // Must remain valid UTF-8 — lossy conversion is applied.
            assert!(std::str::from_utf8(t.as_bytes()).is_ok());
        }
    }

    #[test]
    fn truncate_beyond_length_is_identity() {
        let body = "data: x\n\n";
        assert_eq!(truncate_at(body, 10_000), body);
    }

    #[test]
    fn split_is_lossless() {
        let body = frames(&[openai::ev_delta("r1", "m", "abcdef")], Some("[DONE]"));
        for i in 0..=body.len() {
            let (a, b) = split_at(&body, i);
            assert_eq!(format!("{a}{b}"), body, "split at {i} lost bytes");
        }
    }

    #[test]
    fn anthropic_frames_carry_event_names() {
        let body = format!(
            "{}\n\n{}\n\n{}\n\n",
            anthropic::ev_message_start("m"),
            anthropic::ev_text_delta("hi"),
            anthropic::ev_message_stop()
        );
        assert!(body.contains("event: message_start"));
        assert!(body.contains("event: content_block_delta"));
        assert!(body.contains("event: message_stop"));
    }
}
