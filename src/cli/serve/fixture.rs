//! A scripted, deterministic runtime for `niki serve`, behind the `fixture-runtime` feature.
//!
//! Purpose: let a PTY or snapshot test drive the **real binary** end to end with no network,
//! no model and no sandbox. It walks the reference loop a shell has to render — user prompt,
//! thinking, parallel tool rows, results, a failed tool, streaming text, an approval, a diff,
//! an interrupt, and a completion — by pushing real `DisplayEvent`s through the real
//! `AgenticDisplay` sink, so the production adapter in `super` is what carries them to the wire.
//!
//! It is a surface walk, not a plausible run: no model produced the text, and the interrupt and
//! the completion that follows it are two rows of a UI checklist. Nothing here fabricates an
//! engine result — every event is labelled as fixture data.

// The fixture engine must be unreachable in anything a user installs. A release build with this
// feature enabled is a mistake, and it should be a build failure rather than a shipped binary
// that answers `turn.start` with a script.
#[cfg(not(debug_assertions))]
compile_error!(
    "the fixture runtime is a debug-only feature; `cargo build --release` must never enable \
     `fixture-runtime`. A shipped binary that replays a script instead of running the engine is \
     the worst possible version of this bug."
);

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use crate::artifacts::types::AgentRole;
use crate::display::tui::DisplayEvent;
use crate::permissions::PermissionAction;

use super::{AdapterSink, TurnOutcome};

/// How long the script waits for a shell to answer its scripted approval before answering for
/// it.
///
/// Two seconds was tuned for a render-only test and is far too short for a real shell in a real
/// pseudo-terminal: the interface was still booting when the approval expired, so the prompt
/// flashed past and the run finished before anyone could see it. Thirty seconds is long enough
/// for a human and for a PTY test, and short enough that a forgotten run still terminates.
/// A PTY test boots the real shell through a TypeScript loader, which on a busy machine can take
/// the better part of a minute before the turn is even submitted.
/// `NIKI_FIXTURE_APPROVAL_WAIT_MS` overrides it for a test that must not wait.
fn approval_answer_wait() -> Duration {
    let default = Duration::from_secs(120);
    match std::env::var("NIKI_FIXTURE_APPROVAL_WAIT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        Some(ms) => Duration::from_millis(ms),
        None => default,
    }
}

/// The scripted reference loop.
///
/// Kept as data so a test can assert the shape of the walk without running it, and so the
/// order is one list rather than a sequence buried in timing-dependent calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    Event(&'static str),
    Approval,
}

/// What `niki serve --fixture` replays, in order.
pub(crate) const SCRIPT: &[Step] = &[
    Step::Event("stage.start:planner"),
    Step::Event("stage.done:planner"),
    Step::Event("stage.start:coder"),
    Step::Event("tool.call:read"),
    Step::Event("tool.call:git_status"),
    Step::Event("tool.call:bash"),
    Step::Event("tool.result:read"),
    Step::Event("tool.result:git_status"),
    Step::Event("tool.result:bash_failed"),
    Step::Event("stage.token:coder"),
    Step::Approval,
    Step::Event("diff.ready"),
    Step::Event("notice:interrupted"),
    Step::Event("stage.done:coder"),
    Step::Event("final"),
];

/// Drive `SCRIPT` into `tx` and report what a shell would have seen.
pub(crate) async fn replay(
    tx: &Sender<DisplayEvent>,
    sink: &Arc<AdapterSink>,
    cancel: &Arc<AtomicBool>,
) -> TurnOutcome {
    let mut tool_calls = 0u32;

    for step in SCRIPT {
        tokio::task::yield_now().await;

        if *step == Step::Approval {
            request_approval(tx, sink).await;
            continue;
        }

        let Step::Event(name) = step else {
            continue;
        };

        if name.starts_with("tool.call:") {
            tool_calls += 1;
        }
        emit(tx, name);
    }

    cancel.store(true, Ordering::SeqCst);

    TurnOutcome {
        files_changed: 1,
        tool_calls,
        summary: "Approved (fixture replay)".to_string(),
    }
}

/// Push one scripted event onto the real sink.
///
/// Named events rather than a `Vec<DisplayEvent>` because several of these events carry an
/// `mpsc::Sender` that has to be constructed here, at replay time, not baked into static data.
fn emit(tx: &Sender<DisplayEvent>, name: &str) {
    let ev = match name {
        "stage.start:planner" => DisplayEvent::StageStart {
            role: AgentRole::Planner,
        },
        "stage.done:planner" => DisplayEvent::StageDone {
            role: AgentRole::Planner,
            summary: vec!["Read src/list.rs and src/main.rs.".to_string()],
            input_tokens: 812,
            output_tokens: 96,
            cost_usd: 0.0012,
            latency_ms: 940,
            retry_count: 0,
        },
        "stage.start:coder" => DisplayEvent::StageStart {
            role: AgentRole::Coder,
        },
        "tool.call:read" => DisplayEvent::ToolCall {
            role: AgentRole::Coder,
            tool_name: "read".to_string(),
            summary: "src/list.rs".to_string(),
        },
        "tool.call:git_status" => DisplayEvent::ToolCall {
            role: AgentRole::Coder,
            tool_name: "git_status".to_string(),
            summary: "--porcelain".to_string(),
        },
        "tool.call:bash" => DisplayEvent::ToolCall {
            role: AgentRole::Coder,
            tool_name: "bash".to_string(),
            summary: "cargo test".to_string(),
        },
        "tool.result:read" => DisplayEvent::ToolResult {
            role: AgentRole::Coder,
            tool_name: "read".to_string(),
            success: true,
            error: None,
            output: Some("pub fn paginate(items: &[u32], start: usize, size: usize) -> &[u32] {…}".to_string()),
            duration_ms: 12,
        },
        "tool.result:git_status" => DisplayEvent::ToolResult {
            role: AgentRole::Coder,
            tool_name: "git_status".to_string(),
            success: true,
            error: None,
            output: Some(" M src/list.rs".to_string()),
            duration_ms: 21,
        },
        // The failure the reference loop contains: one tool that did not work, and the run
        // carries on to say so rather than pretending every row succeeded.
        "tool.result:bash_failed" => DisplayEvent::ToolResult {
            role: AgentRole::Coder,
            tool_name: "bash".to_string(),
            success: false,
            error: Some("cargo: command not found (exit 127)".to_string()),
            output: None,
            duration_ms: 8,
        },
        "stage.token:coder" => DisplayEvent::StageToken {
            role: AgentRole::Coder,
            token: "The slice upper bound is `start + size - 1`, which drops the last item.".to_string(),
        },
        "diff.ready" => DisplayEvent::DiffContent(
            "diff --git a/src/list.rs b/src/list.rs\n--- a/src/list.rs\n+++ b/src/list.rs\n@@ -1,4 +1,4 @@\n-    let end = start + size - 1;\n+    let end = start + size;\n"
                .to_string(),
        ),
        "notice:interrupted" => DisplayEvent::Notice {
            text: "run interrupted by the user; the tool loop stopped mid-turn".to_string(),
            warning: true,
        },
        "stage.done:coder" => DisplayEvent::StageDone {
            role: AgentRole::Coder,
            summary: vec!["Fixed the slice upper bound in src/list.rs.".to_string()],
            input_tokens: 1403,
            output_tokens: 218,
            cost_usd: 0.0041,
            latency_ms: 2610,
            retry_count: 1,
        },
        "final" => DisplayEvent::Final {
            verdict: Some("Approved".to_string()),
            error: None,
        },
        other => {
            eprintln!("niki serve: no fixture event named `{other}`");
            return;
        }
    };
    if tx.send(ev).is_err() {
        eprintln!("niki serve: the adapter closed before the fixture finished");
    }
}

/// Ask for an approval and answer it, the way a shell pressing "Allow" would.
///
/// The sender and receiver both have to outlive the call. If the receiver were dropped, the
/// adapter's `recv_timeout` would return immediately and the prompt would resolve to Deny
/// before any shell could see it.
async fn request_approval(tx: &Sender<DisplayEvent>, sink: &Arc<AdapterSink>) {
    let (response_tx, _receiver) = mpsc::channel::<PermissionAction>();
    if tx
        .send(DisplayEvent::PermissionRequest {
            command: "cargo test --all".to_string(),
            response_tx,
        })
        .is_err()
    {
        return;
    }

    let wait = approval_answer_wait();
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if sink.resolve_oldest_approval(PermissionAction::Allow) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    eprintln!(
        "niki serve: fixture: nobody answered the scripted approval within {:?}; it resolves on \
         its own timeout",
        wait
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference loop must contain every row the checklist names, in order. A fixture
    /// that quietly dropped the failed tool or the approval would still render a screenshot.
    #[test]
    fn the_script_walks_the_whole_reference_loop() {
        let names: Vec<&str> = SCRIPT
            .iter()
            .map(|s| match s {
                Step::Event(n) => *n,
                Step::Approval => "<approval>",
            })
            .collect();
        assert_eq!(names.first(), Some(&"stage.start:planner"));
        assert_eq!(names.last(), Some(&"final"));
        assert!(
            names.contains(&"tool.result:bash_failed"),
            "the loop's failed tool is missing: {names:?}"
        );
        assert!(names.contains(&"<approval>"), "no approval in: {names:?}");
        assert!(names.contains(&"diff.ready"), "no diff in: {names:?}");
        assert!(
            names.contains(&"notice:interrupted"),
            "no interrupt in: {names:?}"
        );
        assert!(
            names.contains(&"stage.token:coder"),
            "no streaming text in: {names:?}"
        );
        // The interrupt is followed by a completion, which is the last pair the checklist asks
        // for and the one a fixture is most likely to truncate.
        let interrupt = names
            .iter()
            .position(|n| *n == "notice:interrupted")
            .expect("interrupt");
        assert!(
            names.iter().skip(interrupt).any(|n| *n == "final"),
            "the script must reach a completion after the interrupt: {names:?}"
        );
    }
}
