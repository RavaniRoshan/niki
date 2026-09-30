//! `niki resume` must not claim it continued something.
//!
//! It printed:
//!
//! ```text
//! Session state restored successfully. Ready for continuation.
//! ```
//!
//! and exited 0. Nothing was restored into anything — the `AgentRuntime`
//! holding the session is dropped on the next line — and no code path anywhere
//! re-enters the pipeline from a checkpoint. A user with an interrupted run was
//! told they could carry on, and then nothing happened.
//!
//! Actually resuming is a **feature**, not a repair: it means starting
//! `execute_pipeline` partway through, deciding which stages a checkpoint's
//! `produced_artifacts` already satisfies. That is a design question with
//! product consequences, and it is the roadmap's to answer, not a fix to slip
//! in here. So this slice makes the command say what it actually did and name
//! the commands that do something.
//!
//! The fixture is a real `SessionCheckpoint`, serialized by the real
//! `save()`. A hand-written JSON literal is the wrong tool twice over: it
//! drifted while writing this file (`missing field startup_ms`, then
//! `context_tokens`) and, more importantly, a test that seeds its fixture by
//! hand is asserting against a schema the code no longer has.

use niki::artifacts::types::AgentRole;
use niki::runtime::checkpoint::SessionCheckpoint;

/// Build and save a real checkpoint, the way a run does.
fn seed(project: &std::path::Path) -> (String, String) {
    let task_id = uuid::Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").expect("uuid");
    let session_id = "s-resume-test".to_string();
    let cp = SessionCheckpoint {
        checkpoint_id: "chk-test01".to_string(),
        session_id: session_id.clone(),
        task_id,
        task_description: "add a health endpoint".to_string(),
        current_role: AgentRole::Coder,
        current_turn: 1,
        current_step: 3,
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4".to_string(),
        produced_artifacts: vec![
            (AgentRole::Planner, "{\"summary\":\"x\"}".to_string()),
            (
                AgentRole::Coder,
                "{\"files_changed\":[\"src/lib.rs\"]}".to_string(),
            ),
        ],
        active_branch: Some("niki/aaaaaaaa".to_string()),
        risk_level: Some("low".to_string()),
        metrics: Default::default(),
        fragments: vec![],
        turns: vec![],
        timestamp: chrono::Utc::now(),
    };
    cp.save(project)
        .expect("a real checkpoint saves with the real serializer");
    (session_id, task_id.to_string())
}

/// **The defect.** The command must not claim it continued anything.
#[test]
fn resume_does_not_claim_it_continued() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let (session_id, task_id) = seed(tmp.path());

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args(["resume", &session_id, "--project"])
        .arg(tmp.path())
        .output()
        .expect("niki must run");

    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    let all = format!("{text}{err}");

    assert!(
        !all.contains("Ready for continuation"),
        "the command said it was ready to continue and then exited without \
         continuing anything — no code path re-enters the pipeline from a \
         checkpoint. Output: {all}"
    );
    assert!(
        all.contains("Nothing was re-run"),
        "and it must say so plainly, because a user with an interrupted run \
         needs to know the difference between 'restored' and 'continued': {all}"
    );
    // Naming the commands that *do* something is the useful half.
    assert!(
        all.contains("niki run") && all.contains(&task_id),
        "the output must name what to do instead, including the task it can \
         be re-run with: {all}"
    );
}

/// And it must still report what the checkpoint says — that part was already
/// true and is the reason the command is worth running at all.
#[test]
fn resume_still_reports_the_checkpoint() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let (session_id, task_id) = seed(tmp.path());

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args(["resume", &session_id, "--project"])
        .arg(tmp.path())
        .output()
        .expect("niki must run");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    for expected in ["add a health endpoint", &task_id, "Coder", "niki/aaaaaaaa"] {
        assert!(
            all.contains(expected),
            "the checkpoint report must still include {expected:?}: {all}"
        );
    }
    // Locating a checkpoint that exists is a success, so exit 0 is right here.
    assert!(
        out.status.success(),
        "a checkpoint that exists must exit 0: {all}"
    );
}

/// The failure path must stay as it was: a checkpoint that does not exist is
/// an error with a non-zero exit, not a table of zeroes.
#[test]
fn a_missing_checkpoint_still_fails_loudly() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args(["resume", "no-such-session", "--project"])
        .arg(tmp.path())
        .output()
        .expect("niki must run");

    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "a session that does not exist must exit non-zero: {all}"
    );
    assert!(
        all.to_lowercase().contains("no checkpoint"),
        "and must say what it could not find: {all}"
    );
}
