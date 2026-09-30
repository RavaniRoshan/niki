//! A deadline expiry must be recognisable as one.
//
// `ClientBuilder::timeout` is a **total** deadline: it applies from connect
//! until the response body has finished, so it capped whole generation rather
//! than the wait between bytes. A 3B model on CPU — the README's zero-setup
//! path — spends minutes producing an answer, and a stage that ran long was
//! killed mid-stream. It is now `connect_timeout` + `read_timeout`, so a slow
//! model is fine as long as it is still saying something, and what the deadline
//! catches is a genuinely hung upstream, which is what it was for.
//
// The classifier existed because of what happened next. A total-deadline expiry
//! produced an error whose `Display` never says "timeout", so every retry
//! classifier in this codebase — all of which match on substrings — treated it
//! as permanent. `is_timeout_error` asks the type and walks the source chain.
//
// A note on what a *stalled body* actually reports, measured rather than
// assumed: a server that accepts the connection and then never answers does not
// produce `is_timeout() == true` on the `reqwest::Error`. Depending on where
// it stalls it surfaces as a body-decode failure with the cause only in the
// chain. That is why the walk exists, and why these tests assert on the pieces
// that can be produced deterministically rather than on a 120-second stall.

use niki::llm::provider::{CompletionRequest, LlmProvider, is_timeout_error};

fn request() -> CompletionRequest {
    CompletionRequest {
        model: "m".into(),
        system_prompt: "s".into(),
        user_message: "hello".into(),
        max_tokens: 16,
        temperature: 0.0,
        json_schema: None,
        tools: None,
        reasoning_effort: None,
        history: Vec::new(),
    }
}

/// A real `reqwest` error, from a real connection failure, classified by type.
///
/// A closed port on localhost refuses immediately, so this is fast and
/// deterministic — and it is the same code path a connect timeout takes, since
/// `is_timeout_error` treats them together.
#[tokio::test(flavor = "multi_thread")]
async fn a_real_transport_failure_is_classified_by_type() {
    // Port 9 (discard) is reserved and nothing listens on it in a test sandbox.
    let provider = niki::llm::openai::OpenAiProvider::new_named(
        &niki::config::ProviderConfig {
            api_key: Some("k".into()),
            base_url: Some("http://127.0.0.1:9".into()),
            default_model: "m".into(),
        },
        "t",
    )
    .unwrap();

    let err = provider
        .complete(request())
        .await
        .expect_err("nothing is listening on that port");

    assert!(
        is_timeout_error(&err),
        "a refused connection is a transport failure and must be classified as one: {err}"
    );
}

/// The source chain is walked, because the `Display` does not say so.
///
/// This is the case the function exists for: reqwest's own `is_timeout()` is
/// false for a stalled body, and the string a caller sees is a body-decode
/// message. The `io::ErrorKind::TimedOut` is only reachable through `source()`.
#[test]
fn the_source_chain_is_walked_for_a_hidden_timeout() {
    #[derive(Debug)]
    struct Wrapper(std::io::Error);
    impl std::fmt::Display for Wrapper {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "error decoding response body")
        }
    }
    impl std::error::Error for Wrapper {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    let inner = std::io::Error::new(std::io::ErrorKind::TimedOut, "deadline has expired");
    let e = anyhow::Error::new(Wrapper(inner));

    // The premise: the visible text says nothing useful.
    assert!(
        !e.to_string().to_lowercase().contains("timeout"),
        "precondition: the Display must not name the timeout, or this proves nothing: {e}"
    );
    assert!(
        is_timeout_error(&e),
        "the chain must be walked: the cause is a TimedOut even though the message \\
         does not say so"
    );
}

/// And a genuinely permanent error is not swept up with it.
#[test]
fn a_permanent_error_is_not_swept_up_with_the_timeouts() {
    for msg in [
        "HTTP 404: model not found",
        "HTTP 401: the API key was rejected",
        "the model returned malformed JSON",
    ] {
        let e = anyhow::anyhow!(msg);
        assert!(
            !is_timeout_error(&e),
            "{msg:?} will fail identically on a retry and must not be retried as one"
        );
    }
}

/// An error whose type has been lost still answers the old way.
#[test]
fn a_stringified_timeout_is_still_recognised() {
    let e = anyhow::anyhow!("the request timed out after 30s");
    assert!(
        is_timeout_error(&e),
        "an error carried through a `map_err` that dropped the type must still be \
         recognised, or the string fallback has to stay"
    );
}

/// A timeout is retryable where it can be retried.
#[test]
fn a_timeout_is_retryable_mid_stream() {
    let inner = std::io::Error::new(std::io::ErrorKind::TimedOut, "deadline has expired");
    let e: anyhow::Error = inner.into();
    assert!(
        niki::agents::is_mid_stream_retryable(&e),
        "a stall part-way through an answer is the same event as a stall before one"
    );
}
