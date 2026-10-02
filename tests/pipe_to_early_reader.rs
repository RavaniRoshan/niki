//! `niki … | head` must behave like every other Unix tool, not panic.
//!
//! `niki providers models --plain` is the documented scripting entry point —
//! it exists so output can be piped. But Rust starts with SIGPIPE ignored and
//! turns a failed write into a panic, because `println!` unwraps. So the
//! first reader to leave early (`head`, `grep -q`, a pager the user quits
//! from) produced
//!
//! ```text
//! thread 'main' panicked at library/std/src/io/stdio.rs:1165:9:
//! failed printing to stdout: Broken pipe (os error 32)
//! ```
//!
//! and exit 101, on a command whose entire purpose is to be piped. This is
//! not hypothetical: `scripts/mega-e2e.sh` had `providers models | grep -q .`
//! under `set -o pipefail`, and CI reported a model catalogue that had just
//! printed five ids as "returned nothing niki could parse".
//!
//! The exit status for a tool killed by SIGPIPE is 141. `SIG_IGN` does not
//! help — it converts the kill into an `EPIPE` write error and `println!`
//! panics on that too, so the panic is identical. Only `SIG_DFL` gives the
//! conventional silent death.

use std::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Enough models that the reader can leave with output still to come.
fn catalogue_of(n: usize) -> Vec<serde_json::Value> {
    (0..n)
        .map(|i| serde_json::json!({"id": format!("model-number-{i}")}))
        .collect()
}

async fn provider_serving(n: usize) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"object": "list", "data": catalogue_of(n)})),
        )
        .mount(&server)
        .await;
    server
}

struct Piped {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

/// Run `sh -c 'niki providers models --plain | <reader>'` against `server`.
async fn pipe_through(server: &MockServer, reader: &str) -> Piped {
    let cwd = tempfile::TempDir::new().expect("temp cwd");
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{} providers models --provider openai --plain | {reader}",
            env!("CARGO_BIN_EXE_niki")
        ))
        .current_dir(cwd.path())
        .env("OPENAI_API_KEY", "sk-canary0123456789abcdefghijklmnop")
        .env("OPENAI_BASE_URL", format!("{}/v1", server.uri()))
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENROUTER_API_KEY")
        .output()
        .expect("sh runs");

    Piped {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        // 128 + SIGPIPE. `sh` reports a signalled child as 128+n, so this is
        // the status a caller sees even though the binary never ran `echo $?`.
        code: out.status.code(),
    }
}

/// The defect: a reader that leaves early must not produce a Rust panic.
///
/// This is the assertion that fails when `SIG_IGN` is in place, and when the
/// call is removed entirely — both print the same panic.
#[tokio::test]
async fn head_does_not_make_niki_panic() {
    let server = provider_serving(40).await;
    let r = pipe_through(&server, "head -1").await;

    assert!(
        !r.stderr.contains("panicked"),
        "niki panicked when its reader left early:\n{}",
        r.stderr
    );
    assert!(
        !r.stderr.contains("Broken pipe"),
        "niki reported a broken pipe to the user:\n{}",
        r.stderr
    );
    assert_ne!(
        r.code,
        Some(101),
        "exit 101 is a Rust panic, not the 141 a SIGPIPE death produces"
    );
}

/// The reader still gets the line it asked for. A fix that silenced the panic
/// by printing nothing would pass the test above.
#[tokio::test]
async fn the_early_reader_still_receives_its_line() {
    let server = provider_serving(40).await;
    let r = pipe_through(&server, "head -1").await;

    assert!(
        r.stdout.contains("model-number-0"),
        "the one line requested did not arrive:\nstdout: {}\nstderr: {}",
        r.stdout,
        r.stderr
    );
}

/// `grep -q` is the reader that actually broke CI, and the one a user is most
/// likely to write. Same assertions, because it is the same failure.
#[tokio::test]
async fn grep_q_does_not_make_niki_panic() {
    let server = provider_serving(40).await;
    let r = pipe_through(&server, "grep -q model-number-0").await;

    assert!(
        !r.stderr.contains("panicked"),
        "niki panicked when `grep -q` closed the pipe:\n{}",
        r.stderr
    );
    assert_ne!(r.code, Some(101), "exit 101 is a Rust panic");
}

/// The case that must not regress: a reader that consumes everything gets
/// every line and a clean exit. If a fix broke this it would be fixing the
/// panic by suppressing output.
#[tokio::test]
async fn a_reader_that_consumes_everything_gets_everything() {
    let server = provider_serving(5).await;
    let r = pipe_through(&server, "cat").await;

    assert_eq!(r.code, Some(0), "a full read must exit 0:\n{}", r.stderr);
    for i in 0..5 {
        assert!(
            r.stdout.contains(&format!("model-number-{i}")),
            "model {i} was dropped:\n{}",
            r.stdout
        );
    }
    assert!(!r.stderr.contains("panicked"), "{}", r.stderr);
}
