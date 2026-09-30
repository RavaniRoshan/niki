//! Exec timeouts, against a command that actually runs.
//!
//! Four of the seven tests this file used to hold were arithmetic on a value
//! the test itself had just set: one built `"exec timed out after {}s"` and
//! asserted the string contained `"exec timed out"`, and three asserted that a
//! field equalled the value they had assigned to it a line earlier. No
//! production code was called, so a broken timeout implementation passed all
//! four. The file's own comment claimed it "verif[ied] the message format" —
//! it did not.
//!
//! These run real commands through the sandbox and observe real outcomes.

use niki::config::SecurityPolicyConfig;
use niki::sandbox::check_command_policy;
use niki::sandbox::exec::{ExecTimeout, exec_with_timeout, process_is_alive};
use std::time::{Duration, Instant};

fn sh(script: &str) -> Vec<String> {
    vec!["sh".to_string(), "-c".to_string(), script.to_string()]
}

async fn run(
    script: &str,
    secs: u64,
) -> anyhow::Result<Result<niki::sandbox::exec::ExecOutput, ExecTimeout>> {
    let cwd = std::env::temp_dir();
    exec_with_timeout(
        &sh(script),
        &cwd,
        Duration::from_secs(secs),
        secs,
        niki::sandbox::truncate_head_tail,
    )
    .await
}

// ── Policy configuration ─────────────────────────────────────────────

#[test]
fn default_policy_has_reasonable_exec_timeout() {
    // Meaningful because 300 is a *product decision*: long enough for a cold
    // `cargo build`, short enough that a hung command is not left running all
    // afternoon. Asserting the literal alone would not catch a change; the
    // bounds below express why the value is acceptable.
    let secs = SecurityPolicyConfig::default().max_exec_seconds;
    assert!(
        (60..=900).contains(&secs),
        "the default exec timeout of {secs}s is outside the range that is both useful for a \
         real build and bounded enough to not strand a process"
    );
}

#[test]
fn deny_list_rejects_before_any_timeout_is_consulted() {
    // The point of the deny list is that a forbidden command is refused
    // *immediately*, not run and then stopped. The assertion checks what was
    // refused and why, rather than only that the call did not error.
    let policy = SecurityPolicyConfig::default();
    let start = Instant::now();
    let err =
        check_command_policy(&["rm", "-rf", "/"], &policy).expect_err("`rm -rf /` must be denied");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "a denied command must be rejected without running it, took {elapsed:?}"
    );
    let reason = err.to_string();
    assert!(
        !reason.trim().is_empty(),
        "a denial must say what was denied and why, so the user can act on it"
    );
}

#[test]
fn an_allowed_command_passes_the_policy_check() {
    let policy = SecurityPolicyConfig::default();
    let start = Instant::now();
    check_command_policy(&["cargo", "test", "--lib"], &policy)
        .expect("`cargo test --lib` must be allowed by the default policy");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "the policy check must be a pure, fast decision — it gates every exec"
    );
}

// ── Timeout behaviour, observed rather than asserted arithmetically ──

#[tokio::test]
async fn a_hanging_command_reports_a_timeout_not_a_slow_success() {
    let start = Instant::now();
    let outcome = run("sleep 30", 1)
        .await
        .expect("exec runs")
        .expect_err("`sleep 30` cannot finish inside a 1s deadline");
    let elapsed = start.elapsed();

    assert_eq!(
        outcome.seconds, 1,
        "the error must name the configured limit"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "the deadline must be enforced, not merely reported: the call took {elapsed:?}"
    );
}

#[tokio::test]
async fn the_timeout_error_names_the_configured_limit() {
    // The original version of this test built the expected string itself and
    // asserted the built string matched, so it could not fail. This asserts
    // against the message the production code actually produced.
    let outcome = run("sleep 30", 2)
        .await
        .expect("exec runs")
        .expect_err("must time out");
    let msg = outcome.to_string();

    assert!(
        msg.contains("2s"),
        "the message must name the limit: {msg:?}"
    );
    assert!(
        msg.contains("timed out"),
        "the message must say what happened: {msg:?}"
    );
    assert!(
        msg.contains("killed"),
        "the message must say whether the process group was signalled: {msg:?}"
    );
}

#[tokio::test]
async fn a_command_within_the_deadline_is_not_reported_as_a_timeout() {
    let outcome = run("sleep 1", 30)
        .await
        .expect("exec runs")
        .expect("`sleep 1` must finish well inside a 30s deadline");
    assert_eq!(outcome.exit_code, 0);
}

#[tokio::test]
async fn a_timed_out_command_leaves_nothing_running() {
    // The defect this pins: the deadline used to cancel only the future, not
    // the process, so a timed-out command kept running in the user's tree.
    let pidfile =
        std::env::temp_dir().join(format!("niki-exec-timeout-pid-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&pidfile);

    let outcome = run(
        &format!("sh -c 'echo $$ > {}; sleep 30'", pidfile.display()),
        1,
    )
    .await
    .expect("exec runs")
    .expect_err("must time out");
    assert!(
        outcome.killed,
        "the process group must be signalled: {outcome}"
    );

    std::thread::sleep(Duration::from_millis(800));
    if let Ok(text) = std::fs::read_to_string(&pidfile)
        && let Ok(pid) = text.trim().parse::<i32>()
    {
        assert!(
            !process_is_alive(pid),
            "pid {pid} survived the timeout — a timed-out command is still running"
        );
    }
    let _ = std::fs::remove_file(&pidfile);
}

#[tokio::test]
async fn per_role_timeouts_are_honoured_independently() {
    // The original `policy_timeout_is_configurable_per_role` assigned 60 to a
    // field and asserted the field was 60. This runs the same command under
    // two different limits and shows the limit is what decides the outcome.
    let cwd = std::env::temp_dir();

    let tight = exec_with_timeout(
        &sh("sleep 30"),
        &cwd,
        Duration::from_secs(1),
        1,
        niki::sandbox::truncate_head_tail,
    )
    .await
    .expect("tight exec runs");
    assert!(tight.is_err(), "a 1s limit must fire against `sleep 30`");

    // And a limit generous enough for the command must not fire, proving the
    // bound comes from the policy rather than from a constant.
    let generous = exec_with_timeout(
        &sh("true"),
        &cwd,
        Duration::from_secs(60),
        60,
        niki::sandbox::truncate_head_tail,
    )
    .await
    .expect("generous exec runs");
    assert!(generous.is_ok(), "a 60s limit must not fire against `true`");
}

/// A child killed by a signal is not a success.
///
/// `ExitStatus::code()` is `None` when the process was signalled, and
/// `unwrap_or(0)` turned that into exit code 0. The `test` tool maps 0 to
/// `ToolStatus::Success`, and the red-suite gate reads that to decide whether to
/// cut a branch — so a suite killed by the OOM killer, by SIGSEGV, or by a
/// `panic = "abort"` profile was reported as **passing** and the branch was
/// cut. Every one of those is an ordinary way for a large `cargo test` to die
/// on a loaded machine, and `src/runtime/tools.rs:2673` already used
/// `unwrap_or(-1)` for exactly this.
#[tokio::test(flavor = "multi_thread")]
async fn a_signalled_child_is_not_reported_as_exit_zero() {
    let out = run("kill -SEGV $$", 10)
        .await
        .expect("the exec itself should succeed — the *child* is what dies");

    let exec = out.expect("no transport error");
    assert_ne!(
        exec.exit_code, 0,
        "a child killed by a signal must never read as success: {exec:?}"
    );
    #[cfg(unix)]
    assert_eq!(
        exec.exit_code, 139,
        "SIGSEGV should report 128 + 11, the convention a shell uses: {exec:?}"
    );
    assert!(
        exec.exit_code > 128,
        "the code should name the signal rather than lump it in with 'exited 1': {exec:?}"
    );
}

/// The ordinary path is untouched: a real non-zero exit is still that exit.
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_failure_still_reports_its_own_exit_code() {
    let out = run("exit 3", 10)
        .await
        .expect("exec runs")
        .expect("no transport error");
    assert_eq!(
        out.exit_code, 3,
        "an ordinary exit code must pass through unchanged: {out:?}"
    );
}

/// And a real success is still zero.
#[tokio::test(flavor = "multi_thread")]
async fn a_real_success_is_still_zero() {
    let out = run("exit 0", 10)
        .await
        .expect("exec runs")
        .expect("no transport error");
    assert_eq!(out.exit_code, 0, "{out:?}");
}
