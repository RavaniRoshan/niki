//! One retry rule, three call sites, no disagreements.
//!
//! The rule "is this failure worth retrying?" existed in three places with
//! three different answers:
//!
//! | Site | What it listed |
//! |---|---|
//! | `send_request` | `429` or any 5xx, by `StatusCode` |
//! | `llm::failover` | `http 500`, `502`, `503`, `504`, `408` as substrings |
//! | `agents/mod.rs` | `429`, `503`, `overloaded` as substrings |
//!
//! The agent loop sits **above** the transport, so it was the weakest of the
//! three: a 502 that `send_request` had already retried four times fell out of
//! `agents/mod.rs` unretried, while the failover chain beside it treated 502 as
//! transient. A stage got *less* resilience for having a retry layer above it.
//!
//! The status is now parsed once, from the `HTTP {code}: {body}` every provider
//! writes, and judged by the same predicate — `is_retryable_code` — everywhere.

use niki::llm::provider::{http_status_in, is_retryable_code};

/// The status is recoverable from what the providers actually emit.
#[test]
fn a_provider_status_is_recovered_from_its_message() {
    for code in [400u16, 401, 403, 404, 422, 429, 500, 502, 503, 504] {
        let msg = format!("HTTP {code}: some response body");
        assert_eq!(
            http_status_in(&msg),
            Some(code),
            "`{msg}` must yield {code} — every provider formats it this way"
        );
    }
}

/// And it is **parsed**, not matched as a bare number: a body that happens to
/// contain "500" must not make a permanent 400 look transient.
#[test]
fn a_body_mentioning_a_code_is_not_a_status() {
    for msg in [
        "quota exceeded: 429 requests per minute",
        "server error 503 while fetching",
        "",
        "not an http error at all",
    ] {
        assert_eq!(
            http_status_in(msg),
            None,
            "`{msg}` has no `HTTP <code>:` prefix and must not be read as \\
             carrying a status"
        );
    }
    // The real thing still parses.
    assert_eq!(http_status_in("HTTP 502: upstream"), Some(502));

    // **The case that matters.** A prefixed message whose *body* mentions
    // another code must report the prefix's code, not the body's — otherwise a
    // permanent 400 mentioning "500" would be retried forever. The first
    // version of this test put exactly that case in the list above and
    // expected `None`, which was wrong: it plainly carries a status, 400.
    for (msg, expected) in [
        ("HTTP 400: the model claude-500-2024 does not exist", 400u16),
        ("HTTP 404: tried 3 endpoints, got 502 from the proxy", 404),
        ("HTTP 401: invalid key, hint: rotate at 429", 401),
    ] {
        assert_eq!(
            http_status_in(msg),
            Some(expected),
            "the prefix wins over whatever the body mentions: {msg}"
        );
        assert!(
            !is_retryable_code(expected),
            "and a {expected} must not be retried however suggestive its body \
             is: {msg}"
        );
    }
}

/// The classification itself.
#[test]
fn the_retryable_set_is_one_rule() {
    for code in [429u16, 500, 502, 503, 504] {
        assert!(
            is_retryable_code(code),
            "HTTP {code} is transient — a gateway timeout or a server error \\
             says nothing about the request"
        );
    }
    for code in [200u16, 400, 401, 403, 404, 409, 413, 422] {
        assert!(
            !is_retryable_code(code),
            "HTTP {code} is not transient; retrying spends the user's money to \\
             produce the same error again"
        );
    }
}

/// **The regression.** Every site that decides "transient" must use the shared
/// rule, and none may keep its own list.
#[test]
fn no_call_site_keeps_its_own_transient_list() {
    use std::path::Path;
    let read = |rel: &str| {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
    };

    // The agent loop: the site that was wrong, and the one that matters most
    // because it sits above the transport.
    //
    // Asserted over the **whole file**, not a window. The first version cut
    // the source at the first `Err(e) => {`, which is a different arm several
    // hundred lines earlier, and so examined a region that had never contained
    // the classification. A window here was guessing where the code lives; the
    // claim — "this file no longer hard-codes status numbers" — is about the
    // file.
    let agents = read("src/agents/mod.rs");
    // `is_retryable_code` without a trailing `(`: it is passed as a function
    // *reference* to `is_some_and`, so there is no call parenthesis. Asserting
    // `is_retryable_code(` matched nothing, and the first version reported a
    // correct file as broken for that reason.
    assert!(
        agents.contains("http_status_in(") && agents.contains("is_retryable_code"),
        "the agent loop must judge the status by the shared rule. It listed \
         only `429` and `503`, so a 502 the transport had already retried \
         four times fell out here unretried — less resilience for having a \
         retry layer above it."
    );
    assert!(
        !agents.contains("err_lower.contains(\"429\")")
            && !agents.contains("err_lower.contains(\"503\")"),
        "the agent loop still hard-codes status numbers in its transient \
         list, so the rule can drift again"
    );

    // The failover chain: it had the *better* list, and keeping it would let
    // the two drift apart again.
    let failover = read("src/llm/failover.rs");
    let classify = failover
        .split("fn classify_error(")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("classify_error must exist");
    assert!(
        classify.contains("http_status_in(") && classify.contains("is_retryable_code"),
        "the failover chain must use the same rule, not its own list of \\
         `\"http 5xx\"` substrings: {classify}"
    );
    assert!(
        !classify.contains("\"http 500\""),
        "the failover chain still hard-codes status strings: {classify}"
    );

    // And the transport itself, so the three are provably one rule.
    let provider = read("src/llm/provider.rs");
    assert!(
        provider.contains("pub fn is_retryable_code("),
        "the shared rule must be public, or the other two cannot use it"
    );
}

/// The two must agree on a real message, which is the property the defect was.
#[test]
fn every_site_agrees_on_a_real_provider_message() {
    // What a 502 actually looks like coming out of a provider.
    let real = "HTTP 502: upstream connect error or disconnect/reset before headers";
    let shared = http_status_in(real).is_some_and(is_retryable_code);

    // The agent loop's old list.
    let lower = real.to_lowercase();
    let old_agent = lower.contains("429")
        || lower.contains("503")
        || lower.contains("overloaded")
        || lower.contains("timeout")
        || lower.contains("rate")
        || lower.contains("connection")
        || lower.contains("network");
    // The failover chain's old list.
    let old_failover = ["http 500", "http 502", "http 503", "http 504", "http 408"]
        .iter()
        .any(|s| lower.contains(s));

    assert!(shared, "a 502 must be transient: {real}");
    assert!(
        !old_agent,
        "this message used to be judged NON-transient by the agent loop, \\
         which is the defect"
    );
    assert!(
        old_failover,
        "and transient by the failover chain, which is why the two disagreed"
    );
}
