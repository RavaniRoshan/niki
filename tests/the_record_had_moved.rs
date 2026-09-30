//! Four claims `ROADMAP.md` listed as open, and the code had already fixed.
//!
//! §4.5, §5 and §9.3/§9.4 all said something the tree no longer did. Nothing
//! was wrong with the *code* in any of the four — the record had moved on and
//! the record had not. That is the failure mode this programme keeps meeting:
//! a commit that closes a defect does not always close the bullet that named
//! it, and a stale bullet reads as work still owed.
//!
//! A record correction with nothing behind it rots the same way. So each
//! corrected claim gets a test here. They are not tests of the prose — they
//! test the *fact* the prose now states, so the defect cannot come back and
//! leave the record right again by accident.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// §5 / §4.5: a 500 and a 502 must be retried by the layer above the
/// transport.
///
/// The agent loop's matcher used to be the keyword list `429`, `503`,
/// `overloaded` — while `llm::failover`, a few lines away, listed `500`,
/// `502`, `504` and `408`. A 502 therefore got *less* resilience for having a
/// second retry layer above it: the transport retried, then the agent loop
/// looked at the same error, did not recognise it, and gave up.
///
/// This is a test of the real predicate, not of a list in the source, so the
/// first draft of the bug cannot pass it: a keyword scan of the message text
/// has no way to answer for a code it was never told about.
#[test]
fn a_500_or_502_from_a_provider_is_transient() {
    for code in [429u16, 500, 502, 503, 504] {
        // The shape every provider writes: `HTTP {code}: {body}`.
        let message = format!("HTTP {code}: upstream said no");
        let parsed = niki::llm::provider::http_status_in(&message)
            .unwrap_or_else(|| panic!("`{message}` must yield its status code"));
        assert_eq!(parsed, code, "the status must be read out of the message");
        assert!(
            niki::llm::provider::is_retryable_code(parsed),
            "{code} is a server-side or rate-limit failure and must be retried \
             by the agent loop as well as by the transport"
        );
    }
}

/// And a 4xx that is *not* a rate limit must not be retried, or the matcher
/// has simply become "retry everything" — which turns a bad request key into
/// four slow identical failures.
#[test]
fn a_400_is_not_transient() {
    for code in [400u16, 401, 403, 404, 422] {
        assert!(
            !niki::llm::provider::is_retryable_code(code),
            "{code} will fail identically however many times it is sent"
        );
    }
}

/// §9.3: the agent loop must *use* that predicate.
///
/// A shared helper nothing calls is the exact defect this programme has found
/// three times now (`ensure_git_excluded`, `McpManager::shutdown`,
/// `tools_summary`). So the call site is named, not just the helper's
/// existence.
#[test]
fn the_agent_loop_uses_the_shared_predicate() {
    let agents = read("src/agents/mod.rs");
    assert!(
        agents.contains("crate::llm::provider::http_status_in(&err_str)")
            && agents.contains("crate::llm::provider::is_retryable_code"),
        "the agent loop must judge a provider failure by the same predicate \
         `send_request` and `failover` use, or the layer above the transport \
         is the weaker copy — which is the bug"
    );
}

/// §5: the Coder's tool loop must not swallow its errors.
///
/// `.await.ok()?` turned "something went wrong" into "the loop produced
/// nothing", the caller fell back to the one-shot path, and the user saw a
/// stage that had apparently declined to use the tool loop. `None` for "no
/// artifact" and `None` for "the network died" were the same value.
#[test]
fn the_coder_loop_reports_its_failure() {
    let pipeline = read("src/orchestrator/pipeline.rs");
    let start = pipeline
        .find("async fn run_coder_tool_loop(")
        .expect("run_coder_tool_loop must exist");
    let body: String = pipeline[start..].chars().take(12_000).collect();
    // The error arm, not the function. The first version searched the whole
    // function for `eprintln!` and stayed green when the arm's own was
    // deleted — an unrelated `eprintln!` a few lines away satisfied it. A
    // window is a region; this one has to be the region the claim is about.
    let arm_start = body
        .find("let out = match out {")
        .expect("the result of the loop must be matched, not discarded");
    let arm: String = body[arm_start..].chars().take(400).collect();

    assert!(
        !body.contains(".await.ok()?"),
        "the loop's result must not be discarded with `.ok()?` — that is how a \
         network failure became a silent fallback to the one-shot path"
    );
    assert!(
        arm.contains("tool_loop_failure_notice"),
        "and an error must be reported, not merely returned. The arm reads: {arm}"
    );
    // On **stderr**, not only through `tracing`. `tracing` is off unless
    // `RUST_LOG` is set, and this is the default headless path — so an error
    // reported only through `tracing` is reported to nobody.
    assert!(
        arm.contains("eprintln!"),
        "the failure must reach stderr from the arm that handles it. The arm \
         reads: {arm}"
    );
}

/// §9.4: no un-qualified temporary path in the git output module.
///
/// The record said `git.rs:158` was a fixed temp path that collides across
/// concurrent runs. Whatever is there now, the invariant is that no temporary
/// file is named by anything but the process's own id — which is what makes
/// two concurrent runs not collide. Checked as a **count over every use**, so
/// a new one cannot appear unexamined.
#[test]
fn no_temporary_path_in_git_output_is_shared_between_runs() {
    let git = read("src/output/git.rs");
    let uses: Vec<&str> = git
        .lines()
        .filter(|l| l.contains("temp_dir()"))
        .map(|l| l.trim())
        .collect();
    for line in uses {
        assert!(
            line.contains("process::id()"),
            "a temporary path shared by every run on the machine is a \
             collision waiting for two concurrent runs: {line:?}"
        );
    }
}
