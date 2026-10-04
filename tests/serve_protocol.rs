//! `niki serve` speaks the declared protocol, and says so when it cannot.
//!
//! These drive the **real binary** over real pipes rather than calling a handler, because the
//! properties under test are seam properties: one JSON object per line, flushed per line, an
//! explicit error for anything undeclared, and a server that survives a line it could not parse.
//! A handler that returns the right value but is never written to stdout has none of them.
//!
//! Every child process here is bounded by a timeout and killed by a guard on drop. A protocol
//! test that hangs is not a slow test, it is a leaked process holding a pipe open for the rest
//! of the run.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long any single read may block before the test gives up and kills the server.
const READ_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the server may take to exit after `shutdown` or EOF.
const EXIT_TIMEOUT: Duration = Duration::from_secs(20);

fn niki_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_niki"))
}

/// A running `niki serve`, plus the only handle allowed to outlive a failure.
///
/// `Drop` kills the child. Without it a test that panics mid-conversation leaves a server
/// holding an open stdin pipe, and the next test in the same binary waits on a process nobody
/// is going to answer.
struct ServerGuard {
    child: Child,
    lines: Receiver<String>,
    stdin: Option<std::process::ChildStdin>,
    /// The child's stderr, collected on its own thread so a chatty run cannot fill the pipe
    /// buffer and deadlock while we are waiting for a frame.
    stderr_lines: Receiver<String>,
}

impl ServerGuard {
    fn start(project: &Path, extra: &[&str]) -> Self {
        let mut cmd = Command::new(niki_bin());
        cmd.arg("serve")
            .arg("--project")
            .arg(project)
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("niki serve must start");

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        // Both pipes are drained on their own threads. A server that writes more than a pipe
        // buffer while nobody is reading would otherwise block on write forever.
        let (tx, lines_rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let (stderr_tx, stderr_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if stderr_tx.send(line).is_err() {
                    break;
                }
            }
        });

        Self {
            child,
            lines: lines_rx,
            stdin: Some(stdin),
            stderr_lines: stderr_rx,
        }
    }

    fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        writeln!(stdin, "{line}").expect("write a frame to the server");
        stdin.flush().expect("flush a frame to the server");
    }

    /// The next line the server wrote, or a failure naming how long it waited.
    fn next_line(&self) -> String {
        match self.lines.recv_timeout(READ_TIMEOUT) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                panic!(
                    "the server wrote no line within {READ_TIMEOUT:?}. stderr so far:\n{}",
                    self.stderr_so_far()
                )
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!(
                    "the server closed stdout without a line. stderr so far:\n{}",
                    self.stderr_so_far()
                )
            }
        }
    }

    fn next_json(&self) -> serde_json::Value {
        let line = self.next_line();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("line is not JSON: {e}\n{line}"))
    }

    fn stderr_so_far(&self) -> String {
        let mut out = String::new();
        while let Ok(line) = self.stderr_lines.try_recv() {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// Wait for the process to exit, bounded. Returns its status.
    fn wait(&mut self) -> Option<i32> {
        let deadline = Instant::now() + EXIT_TIMEOUT;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(e) => panic!("could not wait for the server: {e}"),
            }
        }
        panic!("the server did not exit within {EXIT_TIMEOUT:?}");
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        // Drop stdin first: EOF is the server's own signal to stop, and it gets a chance to
        // shut down cleanly before the kill.
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A directory that is not a git repository, for the cases that must not care.
fn plain_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn init_line(id: u64, trace: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"trace_id":"{trace}","method":"initialize","params":{{"protocol_version":1,"client":{{"name":"serve_protocol","version":"0","cols":80,"rows":24}}}}}}"#
    )
}

#[test]
fn initialize_replies_with_the_real_protocol_and_engine_versions() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send(&init_line(1, "t1"));

    let reply = server.next_json();
    assert_eq!(reply["jsonrpc"], "2.0");
    assert_eq!(reply["id"], 1);
    assert_eq!(
        reply["trace_id"], "t1",
        "the response echoes the request's trace"
    );
    assert_eq!(reply["result"]["protocol_version"], 1);
    assert_eq!(
        reply["result"]["engine_version"],
        env!("CARGO_PKG_VERSION"),
        "the engine version is this binary's, read from Cargo.toml at build time"
    );

    // Capabilities must be present, not absent. A shell that finds the key missing cannot tell
    // "off" from "old client", and defaults to drawing everything.
    let caps = &reply["result"]["capabilities"];
    for key in [
        "streaming",
        "approvals",
        "sessions",
        "diffs",
        "context_usage",
        "cost",
    ] {
        assert!(
            caps.get(key).is_some(),
            "capabilities is missing `{key}`: {caps}"
        );
    }
    assert_eq!(
        caps["context_usage"], false,
        "this build never emits `context.usage`, so it must not advertise it: {caps}"
    );
}

#[test]
fn a_version_mismatch_is_refused_rather_than_guessed() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send(
        r#"{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"initialize","params":{"protocol_version":99,"client":{"name":"old","version":"0","cols":80,"rows":24}}}"#,
    );
    let reply = server.next_json();
    assert!(
        reply.get("result").is_none(),
        "a shell speaking another protocol version must not get a result: {reply}"
    );
    assert_eq!(
        reply["error"]["code"],
        niki_protocol::error_code::INTERNAL_ERROR
    );
}

#[test]
fn shutdown_replies_and_the_process_exits() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send(&init_line(1, "t1"));
    let _ = server.next_json();

    server.send(r#"{"jsonrpc":"2.0","id":2,"trace_id":"t1","method":"shutdown","params":{"user_initiated":true}}"#);
    let reply = server.next_json();
    assert_eq!(reply["id"], 2);
    assert_eq!(reply["result"]["ok"], true);

    let code = server.wait();
    assert_eq!(
        code,
        Some(0),
        "a server the shell asked to quit exits zero, not by crashing on the way out"
    );
}

#[test]
fn an_unknown_method_is_an_explicit_error_and_never_a_silent_success() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send(r#"{"jsonrpc":"2.0","id":7,"trace_id":"t1","method":"turn.strat","params":{}}"#);

    let reply = server.next_json();
    assert!(
        reply.get("result").is_none(),
        "a method nobody declared must not come back as a success: {reply}"
    );
    assert_eq!(
        reply["error"]["code"],
        niki_protocol::error_code::METHOD_NOT_FOUND
    );
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("turn.strat")),
        "the error must name the method the caller got wrong: {reply}"
    );

    // And the server is still there afterwards.
    server.send(&init_line(8, "t1"));
    assert_eq!(server.next_json()["id"], 8);
}

#[test]
fn a_malformed_line_is_answered_and_the_server_stays_alive() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send("this is not json at all");

    let reply = server.next_json();
    assert_eq!(
        reply["error"]["code"],
        niki_protocol::error_code::PARSE_ERROR
    );
    assert!(
        reply.get("result").is_none(),
        "a parse error carries no result: {reply}"
    );

    // The next real request is answered normally: a bad line is an error the shell can show,
    // not a dead server.
    server.send(&init_line(9, "t2"));
    let next = server.next_json();
    assert_eq!(next["id"], 9);
    assert_eq!(next["result"]["protocol_version"], 1);
}

#[test]
fn a_declared_method_with_broken_params_is_not_reported_as_an_unknown_method() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    // `turn.start` exists; `prompt` is not a string. Answering "unknown method" here would send
    // the caller hunting for a typo in a method name it spelled correctly.
    server.send(
        r#"{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"turn.start","params":{"prompt":42}}"#,
    );
    let reply = server.next_json();
    assert_eq!(
        reply["error"]["code"],
        niki_protocol::error_code::PARSE_ERROR,
        "got {reply}"
    );
    assert!(
        reply["error"]["data"]
            .as_str()
            .is_some_and(|d| d.contains("turn.start")),
        "the error must name the method whose params were wrong: {reply}"
    );
}

#[test]
fn an_empty_turn_prompt_is_refused_before_a_single_token_is_spent() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send(
        r#"{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"turn.start","params":{"prompt":"   ","permission_mode":"manual"}}"#,
    );
    let reply = server.next_json();
    assert_eq!(
        reply["error"]["code"],
        niki_protocol::error_code::INVALID_PARAMS
    );
}

#[test]
fn session_load_reports_the_real_project_and_invents_no_branch() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    let canonical = dir.path().canonicalize().expect("tempdir resolves");
    server.send(&format!(
        r#"{{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"session.load","params":{{"session_id":null,"project_path":{:?}}}}}"#,
        canonical.to_string_lossy()
    ));

    let reply = server.next_json();
    assert_eq!(reply["id"], 1);
    assert_eq!(
        reply["result"]["project_path"],
        canonical.to_string_lossy().as_ref(),
        "session.load must report the path it resolved, not the string it was handed"
    );
    assert_eq!(
        reply["result"]["branch"],
        serde_json::Value::Null,
        "a directory that is not a git repository has no branch, and `null` is the only honest \
         answer: {reply}"
    );
}

#[test]
fn session_load_reports_the_branch_git_actually_has() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    for args in [
        vec!["init", "-q", "--initial-branch=main"],
        vec!["config", "user.email", "a@b.c"],
        vec!["config", "user.name", "serve"],
    ] {
        let ok = Command::new("git")
            .args(&args)
            .current_dir(repo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} — this test needs a real repository");
    }
    std::fs::write(repo.join("f.txt"), "hi\n").expect("write");
    for args in [vec!["add", "-A"], vec!["commit", "-q", "-m", "init"]] {
        let ok = Command::new("git")
            .args(&args)
            .current_dir(repo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?}");
    }

    let mut server = ServerGuard::start(repo, &[]);
    let canonical = repo.canonicalize().expect("resolves");
    server.send(&format!(
        r#"{{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"session.load","params":{{"session_id":null,"project_path":{:?}}}}}"#,
        canonical.to_string_lossy()
    ));
    let reply = server.next_json();
    assert_eq!(
        reply["result"]["branch"], "main",
        "git reports `main`, so that is what the shell is told: {reply}"
    );
    assert_eq!(
        reply["result"]["ahead"],
        serde_json::Value::Null,
        "a branch with no upstream has no ahead count, and inventing `0` would claim the shell \
         had something to compare against: {reply}"
    );
}

#[test]
fn approval_reply_for_an_unknown_id_is_an_error_not_a_silent_success() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &[]);
    server.send(
        r#"{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"approval.reply","params":{"id":"approval-999","decision":"allow","reason":null}}"#,
    );
    let reply = server.next_json();
    assert!(
        reply.get("result").is_none(),
        "answering an approval nobody asked for must not report success: {reply}"
    );
    assert_eq!(
        reply["error"]["code"],
        niki_protocol::error_code::INTERNAL_ERROR
    );
}

#[test]
fn stdout_carries_frames_only() {
    let dir = plain_dir();
    let mut server = ServerGuard::start(dir.path(), &["--backend", "worktree"]);
    server.send(&init_line(1, "t1"));
    let first = server.next_json();
    assert_eq!(first["result"]["protocol_version"], 1);
    server.send(r#"{"jsonrpc":"2.0","id":2,"trace_id":"t1","method":"shutdown","params":{"user_initiated":true}}"#);
    assert_eq!(server.next_json()["result"]["ok"], true);

    // Every line read so far parsed as JSON. Anything else — a banner, a warning, a log line —
    // would have failed `next_json` above with the offending text in the panic. This asserts
    // the invariant explicitly so the reason survives a future refactor.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match server.lines.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                serde_json::from_str::<serde_json::Value>(&line)
                    .unwrap_or_else(|e| panic!("a non-frame line reached stdout: {e}\n{line}"));
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[test]
fn help_is_a_real_help_page() {
    let out = Command::new(niki_bin())
        .args(["serve", "--help"])
        .output()
        .expect("niki serve --help runs");
    assert!(out.status.success(), "serve --help must exit zero");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("--project"),
        "help must document --project:\n{text}"
    );
    assert!(
        text.contains("--backend"),
        "help must document --backend:\n{text}"
    );
    for backend in ["docker", "worktree"] {
        assert!(
            text.contains(backend),
            "help must name the `{backend}` backend:\n{text}"
        );
    }
}

// ── the fixture runtime is a debug-only door ──────────────────────────────
//
// Everything above proves the real server. These two prove the escape hatch cannot be part of
// a shipped binary: a check that has only ever been green is not known to be a check, and the
// failure this guards against — a released `niki` that answers `turn.start` from a script — is
// silent in every other way.

/// The feature must not be on by default. If it were, `cargo test` and `cargo install` would
/// both compile the fixture in, and nothing downstream would notice.
#[test]
fn the_fixture_runtime_is_off_in_the_default_feature_set() {
    // `cfg!` in a const block rather than a plain `assert!`: the flag is decided at compile
    // time, so a plain assertion is a constant, and clippy is right that a constant assertion
    // cannot be checked at run time — the point is that this binary is only *built* with the
    // feature off, and the assertion is how that fact is pinned to the build rather than to a
    // comment.
    const { assert!(!cfg!(feature = "fixture-runtime")) };

    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml is readable");
    let default_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("default = ["))
        .expect("Cargo.toml declares a [features] default");
    assert!(
        !default_line.contains("fixture-runtime"),
        "the default feature list pulls in the fixture runtime: {default_line}"
    );

    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("Cargo.toml is readable");
    let default_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("default = ["))
        .expect("Cargo.toml declares a [features] default");
    assert!(
        !default_line.contains("fixture-runtime"),
        "the default feature list pulls in the fixture runtime: {default_line}"
    );
    assert!(
        manifest.contains("fixture-runtime = []"),
        "the feature should be declared and empty — it adds code, never a dependency"
    );
}

/// `--release` must be a compile error with the feature on, not a slower build.
#[test]
fn a_release_build_cannot_reach_the_fixture_code() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/cli/serve/fixture.rs"
    ))
    .expect("src/cli/serve/fixture.rs is readable");
    assert!(
        source.contains("#[cfg(not(debug_assertions))]") && source.contains("compile_error!"),
        "the fixture module must refuse to compile in a non-debug build. Without the guard, \
         `cargo build --release --features fixture-runtime` succeeds and ships a scripted engine."
    );
    let server = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cli/serve.rs"))
        .expect("src/cli/serve.rs is readable");
    assert!(
        server.contains(r#"#[cfg(feature = "fixture-runtime")]"#),
        "the fixture module and its `--fixture` flag must be behind the feature, so a default \
         build cannot name either"
    );
}
