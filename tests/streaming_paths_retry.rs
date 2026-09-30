//! Every provider path must retry a transient failure, or say that it does not.
//!
//! `AnthropicProvider::complete()` builds the request, hands it to
//! `send_request`, and gets four attempts with exponential back-off. Its
//! streaming twin — the same provider, the same endpoint, the same transient
//! failures — called `.send()` directly and got **none**:
//!
//! ```rust
//! let resp = self.client.post(url)....json(&payload).send().await?;
//! ```
//!
//! So one 429 or one 503 killed a stream on its first attempt while the
//! non-streaming call retried. The comment above `complete()` said "Retries on
//! 429/5xx" and was true of `complete()` only — `ROADMAP.md` §5.
//!
//! The agent-level matcher catches 429 and 503 but not 500 or 502, so those
//! were terminal on the streaming path even above the transport.
//!
//! **Why retrying a stream is safe here, and where the boundary is.**
//! `send_request` returns once the response *headers* are in; the body is read
//! afterwards. Nothing has been yielded to the caller at the moment a retry can
//! still happen, so a retry cannot duplicate output. A retry around the body
//! read would. That is why the fix is at this line and not further down.

use std::path::Path;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// **The defect.** `stream()` must go through the same retry wrapper as
/// `complete()`.
#[test]
fn anthropic_stream_retries_like_complete() {
    let body = src("src/llm/anthropic.rs");
    let stream = body
        .split("async fn stream(")
        .nth(1)
        .expect("AnthropicProvider::stream must exist");
    let complete = body
        .split("async fn complete(")
        .nth(1)
        .expect("AnthropicProvider::complete must exist");

    for (name, body) in [("complete", complete), ("stream", stream)] {
        assert!(
            body.contains("send_request("),
            "anthropic::{name} must send through `send_request`. Calling \\
             `.send()` directly gets no HTTP retry, so one 429 or 503 from the \\
             provider is terminal — while the other method on the same endpoint \\
             retries four times."
        );
        assert!(
            !body.contains(".json(&payload)\n            .send()")
                && !body.contains(".json(&payload).send()"),
            "anthropic::{name} calls `.send()` directly again"
        );
    }
}

/// And every provider's streaming path, not just Anthropic's. This is the
/// general form: the defect is a path that quietly opts out of the resilience
/// every other path has.
#[test]
fn no_provider_streams_without_the_retry_wrapper() {
    for provider in ["anthropic", "openai", "google", "ollama"] {
        let body = src(&format!("src/llm/{provider}.rs"));
        let stream = body
            .split("fn stream(")
            .nth(1)
            .unwrap_or_else(|| panic!("{provider} must implement `stream`"));
        // Up to the end of the function, or the next `fn `, whichever is first.
        let end = stream
            .find("\n    fn ")
            .map(|i| i + 1)
            .unwrap_or(stream.len());
        let window: String = stream[..end.min(9000)].to_string();
        assert!(
            window.contains("send_request("),
            "{provider}::stream does not go through `send_request`, so a \\
             transient status is terminal on the streaming path only"
        );
    }
}

/// The retry classification itself, since the claim above is only as good as it.
#[test]
fn the_retryable_set_is_what_the_comment_says() {
    use niki::llm::provider::status_is_retryable_for_test;
    for code in [429u16, 500, 502, 503, 504] {
        assert!(
            status_is_retryable_for_test(code),
            "HTTP {code} is transient and must be retried; the comment on \\
             `send_request` says 429 and 5xx"
        );
    }
    for code in [200u16, 201, 400, 401, 403, 404, 422] {
        assert!(
            !status_is_retryable_for_test(code),
            "HTTP {code} is not transient; retrying it wastes the budget and \\
             hides a real error"
        );
    }
}

/// The budget bounds the whole call, retries included — so the fix cannot make
/// a stalled stream take 4 × 120s.
#[test]
fn a_retried_stream_is_still_bounded() {
    let body = src("src/llm/provider.rs");
    assert!(
        body.contains("TOTAL_REQUEST_BUDGET"),
        "`send_request` must keep its total budget, or adding retries to the \\
         streaming path multiplies the worst case"
    );
    // 300s < 4 × 120s read timeout, so the budget is the binding constraint.
    assert!(
        body.contains("from_secs(300)"),
        "the budget must stay below RETRY_MAX_ATTEMPTS x the read timeout, so \\
         a stalled stream is abandoned in a time a person will wait for"
    );
}
