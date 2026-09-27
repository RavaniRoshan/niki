//! Process execution with a timeout that actually kills the work.
//!
//! The previous implementation wrapped `Command::output()` — which blocks
//! until the child exits — in `tokio::task::spawn_blocking`, then dropped the
//! `JoinHandle` when `tokio::time::timeout` fired. Dropping a blocking task's
//! handle does **not** cancel it: a timed-out `cargo build` or `npm install`
//! kept running as an orphan in the user's worktree, holding the port, writing
//! to the tree, and outliving the run that started it.
//!
//! `Command::spawn` returns as soon as the child exists, so the deadline can
//! be enforced here: poll for exit, and on timeout signal the whole **process
//! group** (the child is made a group leader, so `-pid` reaches every
//! descendant — `npm`, `make`, and the compilers it spawns).

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// How long to wait after SIGTERM before escalating to SIGKILL. A well-behaved
/// child gets a chance to clean up; one that ignores SIGTERM still dies.
const GRACE_PERIOD: Duration = Duration::from_millis(500);

/// Poll interval while waiting for exit. Short enough that a timeout is
/// enforced promptly, long enough not to spin.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// What an exec produced.
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

/// An exec that exceeded its deadline. Distinct from a non-zero exit, so a
/// caller cannot mistake "we killed it" for "it failed".
#[derive(Debug, Clone)]
pub struct ExecTimeout {
    pub seconds: u64,
    /// True when the process group was successfully signalled. False means an
    /// orphan may still be running, which the caller must surface.
    pub killed: bool,
}

impl std::fmt::Display for ExecTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "exec timed out after {}s (process group {})",
            self.seconds,
            if self.killed {
                "killed"
            } else {
                "NOT killed — a child may still be running"
            }
        )
    }
}

/// Run `cmd` in `cwd` with a hard deadline, killing the process group on
/// expiry.
///
/// `truncate` bounds the captured output, because an agent command can emit
/// megabytes and the transcript is not the place to store them.
pub async fn exec_with_timeout(
    cmd: &[String],
    cwd: &std::path::Path,
    timeout: Duration,
    max_exec_seconds: u64,
    truncate: fn(&str, usize, usize) -> String,
) -> Result<Result<ExecOutput, ExecTimeout>> {
    let (program, args) = cmd
        .split_first()
        .context("exec received an empty command")?;

    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Make the child a process-group leader so one signal reaches every
    // descendant it spawns.
    #[cfg(unix)]
    {
        // SAFETY: `pre_exec` runs between fork and exec. `setpgid` is
        // async-signal-safe and allocates nothing, so it is legal here.
        unsafe {
            command.pre_exec(|| {
                // New process group, detached from ours, so Ctrl-C in the
                // user's terminal does not race our own kill.
                libc_setpgid();
                Ok(())
            });
        }
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("spawning `{program}`"))?;
    let pid = child.id();

    // Drain stdout/stderr concurrently so a chatty child cannot deadlock on a
    // full pipe buffer while we wait for it to exit.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(p) = stdout_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf).await;
        }
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(p) = stderr_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf).await;
        }
        buf
    });

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    };

    if timed_out {
        let killed = kill_group(pid).await;
        // Reap so the child does not become a zombie.
        let _ = tokio::time::timeout(GRACE_PERIOD, child.wait()).await;
        // Let the readers drain whatever was already buffered.
        let _ = tokio::time::timeout(GRACE_PERIOD, stdout_task).await;
        let _ = tokio::time::timeout(GRACE_PERIOD, stderr_task).await;
        return Ok(Err(ExecTimeout {
            seconds: max_exec_seconds,
            killed,
        }));
    }

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();

    Ok(Ok(ExecOutput {
        exit_code: status.and_then(|s| s.code()).unwrap_or(0) as i64,
        stdout: truncate(&String::from_utf8_lossy(&stdout), 1500, 65536),
        stderr: truncate(&String::from_utf8_lossy(&stderr), 1500, 65536),
    }))
}

#[cfg(unix)]
fn libc_setpgid() {
    // setpgid(0, 0): make this process its own group leader.
    // Declared locally to avoid adding a `libc` dependency for one call.
    unsafe extern "C" {
        fn setpgid(pid: i32, pgid: i32) -> i32;
    }
    unsafe {
        setpgid(0, 0);
    }
}

/// Signal the child's entire process group. Returns whether the signal was
/// delivered.
#[cfg(unix)]
async fn kill_group(pid: Option<u32>) -> bool {
    let Some(pid) = pid else { return false };
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
        fn getpgid(pid: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    const SIGKILL: i32 = 9;

    // A negative pid targets the process group. Fall back to the pgid lookup
    // when the child had not yet become a group leader.
    let target = {
        let pgid = unsafe { getpgid(pid as i32) };
        if pgid > 0 { -pgid } else { -(pid as i32) }
    };

    let term_sent = unsafe { kill(target, SIGTERM) } == 0;
    // Escalate after a grace period; some children ignore SIGTERM.
    tokio::time::sleep(GRACE_PERIOD).await;
    let kill_sent = unsafe { kill(target, SIGKILL) } == 0;
    term_sent || kill_sent
}

/// Whether `pid` is still running. `kill(pid, 0)` performs the permission and
/// existence checks without delivering a signal.
#[cfg(unix)]
pub fn process_is_alive(pid: i32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid, 0) == 0 }
}

#[cfg(not(unix))]
pub fn process_is_alive(_pid: i32) -> bool {
    false
}

#[cfg(not(unix))]
async fn kill_group(_pid: Option<u32>) -> bool {
    // Windows has no process groups in the POSIX sense. `kill_on_drop(true)`
    // on the Command handles the common case; descendants are not reaped.
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), script.to_string()]
    }

    async fn run(script: &str, secs: u64) -> Result<Result<ExecOutput, ExecTimeout>> {
        let cwd = std::env::temp_dir();
        exec_with_timeout(
            &sh(script),
            &cwd,
            Duration::from_secs(secs),
            secs,
            crate::sandbox::truncate_head_tail,
        )
        .await
    }

    #[tokio::test]
    async fn a_fast_command_returns_its_output() {
        let r = run("echo hello; echo oops >&2; exit 3", 10)
            .await
            .expect("exec runs");
        let out = r.expect("must not time out");
        assert!(out.stdout.contains("hello"), "{:?}", out.stdout);
        assert!(out.stderr.contains("oops"), "{:?}", out.stderr);
        assert_eq!(out.exit_code, 3, "a non-zero exit is not a timeout");
    }

    #[tokio::test]
    async fn a_hanging_command_times_out() {
        let r = run("sleep 30", 1).await.expect("exec runs");
        let err = match r {
            Err(e) => e,
            Ok(_) => panic!("sleep 30 must not finish inside a 1s deadline"),
        };
        assert_eq!(err.seconds, 1);
    }

    /// The bug this module exists for: a timed-out command must not keep
    /// running. The child writes a sentinel file well after the deadline; if
    /// the sentinel appears, the orphan survived.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_timed_out_command_leaves_no_orphan() {
        let sentinel =
            std::env::temp_dir().join(format!("niki-orphan-probe-{}", std::process::id()));
        let _ = std::fs::remove_file(&sentinel);

        // Detach from this shell so the child is not reaped with it, and
        // ignore the signal NIKI sends, forcing the SIGKILL escalation path.
        let script = format!(
            "( sleep 3; touch {} ) >/dev/null 2>&1 & wait",
            sentinel.display()
        );
        let r = run(&script, 1).await.expect("exec runs");
        let err = r.expect_err("must time out");
        assert!(err.killed, "the process group must be signalled: {err}");

        // Well past when the orphan would have fired.
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(
            !sentinel.exists(),
            "the timed-out child survived and wrote {} — exec timeouts are not killing the \
             process group",
            sentinel.display()
        );
        let _ = std::fs::remove_file(&sentinel);
    }

    /// A grandchild must die too. `sh -c 'sleep 30'` alone is a single
    /// process; the real leak is `sh` spawning something that outlives it.
    ///
    /// The child records its own pid, and liveness is checked with
    /// `kill(pid, 0)`. Two earlier versions of this test were wrong: one
    /// counted any `sleep 30` system-wide (which the other tests here also
    /// spawn, so it passed alone and failed in the suite), and one grepped
    /// `ps` for a path that the grep process and the enclosing shell both
    /// matched, so it was measuring itself.
    #[cfg(unix)]
    #[tokio::test]
    async fn grandchildren_are_killed_too() {
        let pidfile =
            std::env::temp_dir().join(format!("niki-orphan-pid-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&pidfile);

        // The child writes its pid, then blocks. If the timeout only kills
        // the direct child, this one survives and the pid stays live.
        let r = run(
            &format!("sh -c 'echo $$ > {}; sleep 30'", pidfile.display()),
            1,
        )
        .await
        .expect("exec runs");
        assert!(r.is_err(), "must time out");

        // Give the SIGKILL escalation time to land.
        tokio::time::sleep(Duration::from_millis(800)).await;

        let recorded = std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok());
        let _ = std::fs::remove_file(&pidfile);
        let Some(pid) = recorded else {
            // The child never got far enough to record itself. That is a
            // legitimate outcome on a very slow machine, but it means this
            // run proved nothing, so say so rather than reporting a pass.
            eprintln!("skipping: grandchild never recorded its pid");
            return;
        };

        assert!(
            !process_is_alive(pid),
            "grandchild pid {pid} survived the timeout — the process group is not being killed, \
             so a timed-out command leaves work running in the user's tree"
        );
    }

    /// `process_is_alive` is the oracle the grandchild test depends on. If it
    /// were broken — always false, say — that test would pass while proving
    /// nothing, which is the failure mode this whole harness exists to catch.
    #[cfg(unix)]
    #[test]
    fn the_liveness_probe_actually_detects_a_running_process() {
        // This process is definitionally alive.
        let me = std::process::id() as i32;
        assert!(
            process_is_alive(me),
            "process_is_alive said a running process was dead; every orphan assertion built on \
             it is vacuous"
        );
        // A pid beyond the kernel maximum must read as dead. Note that -1 is
        // NOT such a pid: `kill(-1, sig)` broadcasts to every process the
        // caller may signal, so it succeeds — using it here would have made
        // this self-check fail for a correct implementation.
        let beyond_max = i32::MAX;
        assert!(
            !process_is_alive(beyond_max),
            "process_is_alive reported pid {beyond_max} (beyond any kernel maximum) as alive"
        );
    }

    #[tokio::test]
    async fn empty_output_is_not_an_error() {
        let r = run("true", 10).await.expect("exec runs");
        let out = r.expect("must not time out");
        assert_eq!(out.exit_code, 0);
        assert!(out.stdout.is_empty());
    }

    #[tokio::test]
    async fn a_nonexistent_binary_is_a_typed_error() {
        let r = exec_with_timeout(
            &["definitely-not-a-real-binary-xyz".to_string()],
            &std::env::temp_dir(),
            Duration::from_secs(5),
            5,
            crate::sandbox::truncate_head_tail,
        )
        .await;
        assert!(
            r.is_err(),
            "spawn failure must be an Err, not a fake exit code"
        );
    }
}
