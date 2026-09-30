//! A run that was killed must not keep claiming to be running.
//
// Found by running a real four-agent pipeline against a live model rather
//! than by reading the code. The run was killed mid-Coder; `task.json` said
// `status: "Running"` with a full 2,791 tokens recorded, and nothing would
// ever change it — because a SIGKILL leaves no code to write a terminal state,
// and the SIGTERM handler that would have written one never ran.
//
// The user comes back to a run that has not existed for ten minutes and the
// product says `Running`. `created_at` cannot distinguish that: a slow model
// legitimately looks old while it is working perfectly. A heartbeat can.

use niki::orchestrator::state::{TaskRecord, TaskStatus};
use uuid::Uuid;

fn running() -> TaskRecord {
    let mut r = TaskRecord::new(Uuid::new_v4(), "paginate has an off-by-one");
    r.status = TaskStatus::Running;
    r
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// A live run is not stale, however long it has been going.
#[test]
fn a_live_run_is_never_reported_as_interrupted() {
    let mut r = running();
    // A heartbeat written a minute ago.
    r.last_update = Some(now() - chrono::Duration::minutes(1));
    assert!(
        !r.is_stale_running(now()),
        "a model that is slow is not a model that is dead"
    );

    // And one written nine minutes ago — the longest a real stage should take.
    r.last_update = Some(now() - chrono::Duration::minutes(9));
    assert!(
        !r.is_stale_running(now()),
        "the threshold must not misreport a slow but working run"
    );
}

/// A run whose process stopped writing is not running.
#[test]
fn a_killed_run_is_reported_as_interrupted() {
    let mut r = running();
    r.last_update = Some(now() - chrono::Duration::minutes(11));
    assert!(
        r.is_stale_running(now()),
        "a run that has not written for eleven minutes is not in progress"
    );
}

/// No heartbeat at all is stale. A record written by an older NIKI has none,
/// and treating it as live is how the original bug survives an upgrade.
#[test]
fn a_record_with_no_heartbeat_is_interrupted() {
    let r = running();
    assert!(
        r.last_update.is_none(),
        "precondition: a record built in memory carries no heartbeat yet"
    );
    assert!(r.is_stale_running(now()));
}

/// Only `Running` is ever stale. A finished run stays finished.
#[test]
fn a_finished_run_is_never_interrupted_however_old() {
    for status in [
        TaskStatus::Completed,
        TaskStatus::Cancelled,
        TaskStatus::Failed {
            error: "boom".into(),
        },
    ] {
        let mut r = running();
        r.status = status.clone();
        r.last_update = Some(now() - chrono::Duration::days(3));
        assert!(
            !r.is_stale_running(now()),
            "{status:?} is a settled outcome and must not be relabelled"
        );
    }
}

/// And the heartbeat is actually written, or the check above is theatre.
#[test]
fn saving_a_record_stamps_a_heartbeat() {
    let dir = tempfile::tempdir().unwrap();
    let r = running();
    assert!(r.last_update.is_none(), "precondition");
    r.save_to_disk(dir.path()).expect("the record saves");

    let bytes = std::fs::read(dir.path().join("task.json")).expect("task.json written");
    let back: TaskRecord = serde_json::from_slice(&bytes).expect("it round-trips");
    let stamped = back
        .last_update
        .expect("the heartbeat is persisted, not just in memory");
    assert!(
        (now() - stamped).num_seconds().abs() < 120,
        "the stamp must be the time of the write, not of the struct's construction"
    );
}
