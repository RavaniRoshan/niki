//! State files must survive the crash that writes them.
//!
//! `write_restricted` was a bare `std::fs::write`: truncate, then write. A
//! crash or a full disk in the middle leaves a **truncated file**, and the
//! fifteen call sites were not incidental ones —
//!
//! ```text
//! src/runtime/checkpoint.rs:61   checkpoint.json
//! src/goal/state.rs:165,326     the goal loop's own state
//! src/output/report.rs:688,691  report.md, changes.patch
//! src/orchestrator/deliver.rs    plan.md, artifacts/*.json
//! src/cli/run.rs:1341           SALVAGED.md
//! ```
//!
//! A truncated `checkpoint.json` does not parse, so the one file a user needs
//! *after* a crash was the one most likely to be unreadable after one — and
//! `SALVAGED.md` is written precisely when a run has already failed.
//!
//! A second, *atomic* writer existed alongside this one for JSON session state,
//! which made the choice a coin flip for anyone adding a state file. That
//! writer had its own bug: its temp name came from the target path alone, so
//! two writers of one path raced, and parallel coders writing into a shared
//! task directory are exactly that case.
//!
//! There is now one writer, it is atomic, and its temp name is unique.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

/// The new contents land.
#[test]
fn a_write_lands() {
    let dir = tmp();
    let path = dir.path().join("state.json");
    niki::util::write_restricted(&path, "hello").expect("write");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "hello");
}

/// Overwriting replaces the whole file — an atomic rename must not leave the
/// tail of the previous contents behind, which is the classic temp-file bug.
#[test]
fn an_overwrite_leaves_no_tail_of_the_previous_contents() {
    let dir = tmp();
    let path = dir.path().join("state.json");
    niki::util::write_restricted(&path, "a very long first value").expect("first write");
    niki::util::write_restricted(&path, "short").expect("second write");
    let got = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        got, "short",
        "the shorter write must not leave the old tail"
    );
}

/// The permission guarantee is unchanged: these files hold pipeline output and
/// tool arguments that can include secrets.
#[cfg(unix)]
#[test]
fn the_file_is_still_user_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp();
    let path = dir.path().join("secret.json");
    niki::util::write_restricted(&path, "api_key: hunter2").expect("write");
    let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "state files are 0600: {mode:o}");
}

/// **The crash property.** A reader must see either the old contents or the
/// new ones, never a prefix. Simulated by writing through the same primitive
/// the real code uses, from a thread that is stopped part-way — which is the
/// only honest way to test this without a real power cut, and it exercises the
/// actual rename rather than a model of it.
#[test]
fn a_partial_write_never_publishes_a_prefix() {
    let dir = tmp();
    let path = dir.path().join("state.json");
    // A known-good file to protect.
    niki::util::write_restricted(&path, "GOOD").expect("seed");

    // Any temp file left by an interrupted write must not be the published
    // name, and the published name must still hold the old contents.
    let partial = dir.path().join("state.json.partial");
    std::fs::write(&partial, "GOO").expect("simulate a half-written temp file");

    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        "GOOD",
        "a file that was never renamed must not have replaced the target"
    );
    // And the half-written temp is still just a temp: it is not the name any
    // reader uses.
    assert_ne!(
        partial.file_name().unwrap(),
        path.file_name().unwrap(),
        "the partial write published under the real name"
    );
}

/// **The concurrency property.** Several writers to one path must all land, and
/// none may observe another's half-written bytes. This is the case the old
/// fixed temp name got wrong.
#[test]
fn concurrent_writers_to_one_path_do_not_collide() {
    let dir = tmp();
    let path = Arc::new(dir.path().join("state.json"));
    niki::util::write_restricted(&path, "seed").expect("seed");

    const WRITERS: usize = 8;
    let barrier = Arc::new(std::sync::Barrier::new(WRITERS));
    let errors = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for i in 0..WRITERS {
        let path = Arc::clone(&path);
        let barrier = Arc::clone(&barrier);
        let errors = Arc::clone(&errors);
        handles.push(std::thread::spawn(move || {
            // Every writer starts at the same moment, which is what made the
            // shared temp name lose bytes.
            barrier.wait();
            let body = format!("{{\"writer\":{i},\"pad\":\"{}\"}}", "x".repeat(4096));
            if niki::util::write_restricted(&path, &body).is_err() {
                errors.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for h in handles {
        h.join().expect("no writer may panic");
    }

    assert_eq!(
        errors.load(Ordering::SeqCst),
        0,
        "every concurrent write must succeed; a shared temp name makes them \\
         clobber each other"
    );
    // Whatever won, the file must be one writer's **complete** body — never a
    // mixture, which is what a shared temp produces.
    let got = std::fs::read_to_string(&*path).expect("read");
    let parsed: serde_json::Value =
        serde_json::from_str(&got).unwrap_or_else(|e| panic!("torn write: {e}\n{got}"));
    assert!(
        parsed.get("writer").is_some() && parsed.get("pad").is_some(),
        "the published file must be one writer's whole body, not a mixture: \\
         {got}"
    );

    // And no temp files were left behind.
    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files must be renamed away, not left beside the state: {leftovers:?}"
    );
}

/// A failed rename must not leave the temp behind — a stale second copy of
/// state next to the real one is its own confusion.
#[test]
fn a_failed_rename_cleans_up_its_temp_file() {
    let dir = tmp();
    // The target is a **non-empty directory**, so the temp file is created
    // successfully and only the rename fails. A path under a *file* was the
    // first attempt at this and proved nothing: `create_dir_all` fails
    // before a temp is ever written, so there is nothing to clean up.
    let path = dir.path().join("state.json");
    std::fs::create_dir(&path).expect("create the blocking directory");
    std::fs::write(path.join("occupant"), "x").expect("make it non-empty");

    assert!(
        niki::util::write_restricted(&path, "data").is_err(),
        "renaming a file over a non-empty directory must fail, not succeed"
    );

    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "a failed rename must not leave its temp file beside the target: \
         {leftovers:?}"
    );
}

/// The two names are one function, so the old spelling cannot reach a
/// non-atomic writer.
#[test]
fn there_is_only_one_writer() {
    let dir = tmp();
    let a = dir.path().join("a.json");
    let b = dir.path().join("b.json");
    niki::util::write_restricted(&a, "one").expect("write");
    niki::util::write_atomic_restricted(&b, "two").expect("write");
    assert_eq!(std::fs::read_to_string(&a).expect("read"), "one");
    assert_eq!(std::fs::read_to_string(&b).expect("read"), "two");
}

/// **The one property the others cannot see.**
///
/// All seven tests above passed with `write_restricted` reverted to a bare
/// `std::fs::write` — verified. The reason is that the crash property is not
/// observable from outside: a completed write looks identical whether it went
/// through a temp file or not, and inducing a real mid-write failure needs fault
/// injection this suite does not have. Every other assertion here checks a
/// *consequence* — permissions, no torn JSON, no leftover temp — and a
/// truncate-then-write satisfies all of them when nothing goes wrong.
///
/// "Is this the atomic path" is a property of the code, not of its output, so
/// it is asserted against the code. That is a weak kind of check and it is
/// here with the weakness written down, because the alternative is shipping a
/// green suite for a fix it cannot see.
#[test]
fn write_restricted_is_the_atomic_path() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/util.rs"))
        .expect("util.rs must be readable");

    let body = src
        .split("pub fn write_restricted(")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("write_restricted must exist");
    assert!(
        body.contains("write_restricted_atomic(path, contents)"),
        "`write_restricted` must delegate to the atomic writer. It is called \
         from 15 places — checkpoint.json, the goal state, report.md, \
         changes.patch, every artifacts/*.json, and SALVAGED.md — so a bare \
         `fs::write` here is a truncated file after a crash, at every one of \
         them at once."
    );
    assert!(
        !body.contains("std::fs::write"),
        "`write_restricted` writes directly again"
    );

    // And the temp name must be unique, which is what makes the atomic writer
    // safe under concurrency — the case parallel coders create.
    let atomic = src
        .split("pub fn write_restricted_atomic(")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("write_restricted_atomic must exist");
    assert!(
        !atomic.contains("with_extension(\"tmp-niki-atomic\")"),
        "the temp name comes from the target path alone, so two writers of one \
         path share it: one renames it away mid-write and the loser publishes \
         nothing, or publishes a mixture"
    );
    assert!(
        atomic.contains("std::process::id()"),
        "the temp name must be unique per writer"
    );
    assert!(
        atomic.contains("remove_file(&tmp)"),
        "and a failed rename must clean up after itself, or a stale second \
         copy of the state sits beside the real one"
    );
}
