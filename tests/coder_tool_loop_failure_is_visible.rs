//! A tool loop that *errored* must not look like a tool loop that declined.
//!
//! The last of the "every way the Coder's loop can come back empty has to say
//! so" cases, and the only one that means something is wrong.
//!
//! Measured against a live provider, the Coder's tool loop ended with:
//!
//!     error sending request for url (.../v1/chat/completions): client error
//!     (SendRequest): connection error: Connection timed out (os error 110)
//!
//! and `.await.ok()?` made that the *same value* as a loop that returned no
//! artifact. The caller could not tell the two apart, so it fell back to the
//! one-shot path and said nothing. From the outside the run read as "the Coder
//! did not use the tool loop this time" — which is a benign, expected thing —
//! when the truth was that the Coder lost the network and then lost several
//! more minutes inside the fallback.
//!
//! Two things have to hold, and each is checked separately because each fails
//! for a different reason:
//!
//!  1. the `Err` arm exists and reports, rather than being folded into
//!     `Ok`/`None` at the call site; and
//!  2. the message names the actual error and says the fallback is degraded —
//!     because a generic "fell back" is what the user already had, and it is
//!     what made the failure invisible in the first place.

use std::path::Path;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

fn tool_loop_body() -> String {
    let s = src("src/orchestrator/pipeline.rs");
    let start = s
        .find("async fn run_coder_tool_loop(")
        .expect("the tool loop exists");
    let end = s[start..]
        .find("\n/// What to tell the user when the Coder's tool loop")
        .map(|i| start + i)
        .expect("the loop ends before the notice helpers");
    s[start..end].to_string()
}

/// The `Err` arm survives at the call site.
#[test]
fn a_failed_tool_loop_is_not_silently_empty() {
    let body = tool_loop_body();

    // Only the loop's *own* result matters here. The function legitimately
    // uses `.ok()?` for the schema, the template and the render — an unwritten
    // asset is a different problem from a lost network. `out` is the loop's
    // result and nothing else in this function is called that, and matching on
    // whitespace-stripped source covers the two shapes rustfmt produces:
    // `).await\n.ok()?` and `out.ok()?`.
    let flat: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        !flat.contains(").await.ok()?") && !flat.contains("out.ok()?"),
        "`.ok()?` on the tool loop's result makes a network failure the same \
         value as an empty loop: the caller falls back either way and the user \
         is told nothing. A live run lost its connection here and the run \
         reported a declined tool loop instead."
    );
    assert!(
        body.contains("Err(e) =>") && body.contains("tool_loop_failure_notice(&e)"),
        "the tool loop's error must reach the user through its own notice; \
         `coder_loop_fallback_notice` is a different case and says the wrong thing"
    );
}

/// The message is actionable, not a restatement that a fallback happened.
#[test]
fn tool_loop_failure_names_the_cause_and_the_cost() {
    // Wrapped, because that is how the real error arrives: a transport failure
    // nested under the stage's own context. `{e:#}` walks that chain, so a
    // message built from `{}` would print only the outer sentence and drop the
    // "os error 110" that identifies the actual fault.
    let err = anyhow::anyhow!("Connection timed out (os error 110): client error (SendRequest)")
        .context(
            "error sending request for url (https://integrate.api.nvidia.com/v1/chat/completions)",
        );
    let msg = niki::orchestrator::pipeline::tool_loop_failure_notice(&err);

    assert!(
        msg.contains("os error 110"),
        "the notice must carry the innermost cause, not just the outer \
         sentence. This is the one that named the real fault: {msg}"
    );
    assert!(
        msg.contains("nvidia.com"),
        "and the outer sentence, which names the request that failed: {msg}"
    );
    assert!(
        msg.contains("less capable"),
        "the fallback is a downgrade, and the user is the one who has to decide \
         whether to trust the result: {msg}"
    );
    assert!(
        msg.contains("not this"),
        "the fallback will usually fail too, and its error is a different \
         problem. Without this the user reads the fallback's error as the \
         real one — which is exactly the misreading observed live: {msg}"
    );
}
