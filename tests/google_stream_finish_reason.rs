//! Google's streaming path, which had no tests at all.
//!
//! `src/llm/google.rs` was the one provider file with **zero** tests
//! (`ROADMAP.md` §6), and the first thing the first test found is the defect
//! the roadmap predicted: the streaming path never emitted
//! `StreamChunk::Finish`, while the non-streaming path read the same field
//! three lines earlier.
//!
//! That is not cosmetic. `StreamChunk::Finish` exists for one reason, and the
//! enum's own doc says it:
//!
//! > This is the only way a streaming caller can tell a *complete* response
//! > from one cut off at the token limit, and without it the two are
//! > indistinguishable: a half-written artifact looks exactly like a malformed
//! > one, gets fed to the JSON repairer, and is reported as "did not satisfy
//! > the artifact requirements" — which sends a user to blame the model for
//! > something the token limit did.
//!
//! Google sends `finishReason: "MAX_TOKENS"` in exactly that situation, and
//! the one-shot agent path streams — so on Google, a response that ran out of
//! tokens was reported as a bad model.
//!
//! Driving the real parser needed it out of the `tokio::spawn` closure that
//! reads a live HTTP response. That extraction is the other half of the slice:
//! an SSE parser nobody can reach is a parser nobody is checking.

use niki::llm::google::handle_sse_line;
use niki::llm::provider::StreamChunk;

/// Feed one SSE line through the real parser and collect what it emitted.
fn chunks_from(line: &str) -> Vec<StreamChunk> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    assert!(
        handle_sse_line(line, &tx),
        "the parser reported a dead receiver on a live channel"
    );
    drop(tx);
    let mut out = Vec::new();
    while let Ok(Ok(c)) = rx.try_recv() {
        out.push(c);
    }
    out
}

fn data(payload: serde_json::Value) -> Vec<StreamChunk> {
    chunks_from(&format!("data: {payload}"))
}

/// **The defect.** A response cut off at the token limit must say so.
#[test]
fn a_truncated_response_keeps_its_stop_reason() {
    let out = data(serde_json::json!({
        "candidates": [{
            "content": {"parts": [{"text": "{\"summary\":"}]},
            "finishReason": "MAX_TOKENS",
        }],
    }));

    let finish = out
        .iter()
        .find_map(|c| match c {
            StreamChunk::Finish { reason } => Some(reason.clone()),
            _ => None,
        })
        .expect(
            "a MAX_TOKENS response must emit StreamChunk::Finish — without it a \
                 half-written artifact is reported as a malformed one, and the user \
                 is told to blame the model for the token limit",
        );

    assert_eq!(
        finish, "MAX_TOKENS",
        "the stop reason must be Google's own value, not a guess"
    );
}

/// And the reason is *distinguishable*, which is the point of the enum: a
/// complete response and a truncated one must not look alike.
#[test]
fn a_complete_response_and_a_truncated_one_differ() {
    let complete = data(serde_json::json!({
        "candidates": [{
            "content": {"parts": [{"text": "{\"summary\":\"done\"}"}]},
            "finishReason": "STOP",
        }],
    }));
    let truncated = data(serde_json::json!({
        "candidates": [{
            "content": {"parts": [{"text": "{\"summary\":"}]},
            "finishReason": "MAX_TOKENS",
        }],
    }));

    let reason = |v: &[StreamChunk]| {
        v.iter()
            .find_map(|c| match c {
                StreamChunk::Finish { reason } => Some(reason.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "<none>".to_string())
    };
    assert_ne!(
        reason(&complete),
        reason(&truncated),
        "a complete response and one cut off at the token limit must be \
         distinguishable to the caller"
    );
    assert_eq!(reason(&complete), "STOP");
    assert_eq!(reason(&truncated), "MAX_TOKENS");
}

/// Text still streams, one chunk at a time.
#[test]
fn text_is_still_streamed() {
    let out = data(serde_json::json!({
        "candidates": [{"content": {"parts": [{"text": "hello"}]}}],
    }));
    assert!(
        out.iter()
            .any(|c| matches!(c, StreamChunk::Text(t) if t == "hello")),
        "text must still reach the caller: {out:?}"
    );
}

/// Usage still streams, and the reasoning-token field is not dropped.
#[test]
fn usage_is_still_streamed_with_reasoning_tokens() {
    let out = data(serde_json::json!({
        "usageMetadata": {
            "promptTokenCount": 11,
            "candidatesTokenCount": 22,
            "cachedContentTokenCount": 3,
            "thoughtsTokenCount": 7,
        },
    }));
    let usage = out
        .iter()
        .find_map(|c| match c {
            StreamChunk::Usage(u) => Some(*u),
            _ => None,
        })
        .expect("usage must reach the caller");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 22);
    assert_eq!(usage.cached_input_tokens, 3);
    assert_eq!(usage.reasoning_tokens, 7);
}

/// `[DONE]` terminates cleanly, and a line with no `data: ` prefix is ignored
/// rather than treated as an error — SSE comment lines and blank keep-alives
/// are normal.
#[test]
fn sentinel_and_noise_lines_are_handled() {
    let out = chunks_from("data: [DONE]");
    assert!(out.is_empty(), "[DONE] must emit nothing: {out:?}");

    let out = chunks_from(": keep-alive");
    assert!(out.is_empty(), "an SSE comment must emit nothing: {out:?}");

    let out = chunks_from("event: message");
    assert!(out.is_empty(), "a non-data line must emit nothing: {out:?}");

    // Malformed JSON is a chunk NIKI cannot use, and must not kill the
    // reader — a provider that sends one bad line has not failed the call.
    let out = chunks_from("data: {not json");
    assert!(out.is_empty(), "malformed JSON must emit nothing: {out:?}");
}

/// A candidate with an empty `finishReason` must not emit an empty reason: a
/// caller checking `reason == "MAX_TOKENS"` would be fine, but one printing it
/// would show the user a blank stop reason.
#[test]
fn an_empty_stop_reason_is_not_emitted() {
    let out = data(serde_json::json!({
        "candidates": [{
            "content": {"parts": [{"text": "x"}]},
            "finishReason": "",
        }],
    }));
    assert!(
        !out.iter()
            .any(|c| matches!(c, StreamChunk::Finish { reason } if reason.is_empty())),
        "an empty finishReason must not become an empty Finish chunk: {out:?}"
    );
}

/// Every provider that can stream must be able to say why it stopped. This is
/// the general form of the defect: it is a property of the enum's contract, not
/// of one file.
#[test]
fn every_streaming_provider_emits_a_finish_chunk() {
    for provider in ["anthropic", "openai", "ollama", "google"] {
        let body = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("src/llm/{provider}.rs")),
        )
        .unwrap_or_else(|e| panic!("{provider}.rs must be readable: {e}"));
        assert!(
            body.contains("StreamChunk::Finish"),
            "{provider} streams but never emits `StreamChunk::Finish`, so a \
             response cut off at the token limit is indistinguishable from a \
             malformed one"
        );
    }
}
