//! Two runs in one repository must not apply each other's patch.
//!
//! `apply_diff_to_working_tree` wrote the patch to a **fixed** `.niki-tmp.patch`
//! inside the repository, and `git apply` reads that file back *by path*.
//!
//! That is worse than the shared-temp-name case fixed in B4-10
//! (`write_restricted_atomic`): there, one writer lost a rename and the write
//! failed loudly. Here the contents cross over. Two concurrent runs against
//! one repository could each apply **the other's** diff — two tasks' work
//! silently mixed on the host tree — and whichever finished first removed the
//! file out from under the other's `git apply`.
//!
//! Real `git`, real repository, real concurrent applies. The assertion is that
//! each run's own file lands and neither sees the other's content.

use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Barrier};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} must run: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A repo with one committed file, ready to be patched.
fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-q", "-b", "main", "."]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("shared.txt"), "base\n").expect("write base");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "init"]);
    (tmp, dir)
}

/// A real diff, produced by `git` itself.
///
/// The first version hand-wrote one and every apply failed with "corrupt patch
/// at line 5" — a test fixture that does not parse is a fixture that asserts
/// nothing, and the same trap as the hand-written `task.json` in
/// `tests/history_enter_opens_the_run.rs`. Ask git for the format.
fn diff_for(dir: &Path, tag: &str) -> String {
    std::fs::write(dir.join("shared.txt"), format!("base\n{tag}\n")).expect("write");
    let diff = git(dir, &["diff", "--", "shared.txt"]);
    git(dir, &["checkout", "--", "shared.txt"]);
    assert!(
        diff.contains(tag),
        "git did not produce a diff containing the tag: {diff:?}"
    );
    diff
}

/// **The defect.** Two concurrent applies must never read each other's patch.
///
/// The first version of this asserted that *both applies succeed*, and it was
/// flaky: two `git apply` calls against the same file genuinely conflict —
/// whichever lands first changes the context, so the second fails with
/// `patch failed: shared.txt:1`. That is correct behaviour, with or without a
/// shared temp name, and a test must not fail on it.
///
/// What is actually claimed is narrower and checkable: each run writes a
/// **distinct** file holding **its own** content, and any failure is a git
/// conflict (exit 1) rather than a missing file (exit 128). Exit 128 with
/// "can't open patch ... No such file or directory" is the signature of the
/// shared-temp bug — one run removed the file the other was about to read —
/// and it is what the sabotage produces.
#[test]
fn concurrent_applies_never_read_each_others_patch() {
    let (_tmp, dir) = repo();
    let dir = Arc::new(dir);

    // Both diffs are produced **before** any thread starts. `git diff` and
    // `git checkout` take the index lock, so generating them inside the threads
    // raced the fixture rather than the code under test — the first version
    // died with "Unable to create '.git/index.lock'".
    let one = diff_for(&dir, "TASK-ONE");
    let two = diff_for(&dir, "TASK-TWO");

    let barrier = Arc::new(Barrier::new(2));
    let seen: Arc<std::sync::Mutex<Vec<std::path::PathBuf>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for patch in [one.clone(), two.clone()] {
        let dir = Arc::clone(&dir);
        let barrier = Arc::clone(&barrier);
        let seen = Arc::clone(&seen);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            // Watch the repo for the temporaries each run creates, and which
            // run's tag each one holds.
            let watcher = {
                let dir = Arc::clone(&dir);
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || {
                    for _ in 0..200 {
                        if let Ok(entries) = std::fs::read_dir(&*dir) {
                            for e in entries.flatten() {
                                let name = e.file_name().to_string_lossy().to_string();
                                if !name.contains(".niki-tmp") {
                                    continue;
                                }
                                let body = std::fs::read_to_string(e.path()).unwrap_or_default();
                                let tag = if body.contains("TASK-ONE") {
                                    "TASK-ONE"
                                } else if body.contains("TASK-TWO") {
                                    "TASK-TWO"
                                } else {
                                    "empty"
                                };
                                seen.lock()
                                    .expect("lock")
                                    .push(std::path::PathBuf::from(format!("{name}|{tag}")));
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                })
            };
            let result = niki::output::git::apply_diff_to_working_tree(&dir, &patch);
            // Let the watcher catch a file that is still on disk.
            std::thread::sleep(std::time::Duration::from_millis(20));
            drop(watcher);
            result
        }));
    }

    for h in handles {
        let r = h.join().expect("no thread may panic");
        if let Err(e) = &r {
            let msg = e.to_string();
            assert!(
                !msg.contains("No such file or directory") && !msg.contains("exit Some(128)"),
                "a run's patch file vanished before it could read it — the \
                 shared-temp signature. A git *conflict* (exit 1, \
                 \"patch failed\") is correct behaviour and is allowed. Got: {msg}"
            );
        }
    }

    // Group by name. The watcher polls, so it can catch a file **mid-write** —
    // before the bytes land — and record the same name twice, once with no
    // readable content and once with a real tag. That is the watcher seeing
    // the write in progress, not two runs sharing a name, and the first
    // version of this assertion reported it as exactly that:
    //
    //     [(".niki-tmp…6408.patch", "TASK-ONE"),
    //     (".niki-tmp…6408.patch", "empty"),
    //     (".niki-tmp…8687.patch", "TASK-TWO")]
    //
    // An empty observation is a partial read, so it carries no claim about
    // which run owns the name. A name with **two different real tags** is the
    // bug, and is what this now asserts.
    let observed = seen.lock().expect("lock").clone();
    let mut by_name: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    for p in &observed {
        let raw = p.to_string_lossy().to_string();
        let mut it = raw.rsplitn(2, '|');
        let tag = it.next().unwrap_or("").to_string();
        let name = it.next().unwrap_or("").to_string();
        by_name.entry(name).or_default().insert(tag);
    }

    for (name, tags) in &by_name {
        let real: Vec<&String> = tags.iter().filter(|t| *t != "empty").collect();
        assert!(
            real.len() <= 1,
            "one temporary name was written by two different runs, so one can \
             read the other's patch: {name} carried {real:?}"
        );
        for tag in real {
            assert!(
                *tag == "TASK-ONE" || *tag == "TASK-TWO",
                "a temporary held neither run's patch: {name}|{tag}"
            );
        }
    }
    assert!(
        by_name.len() >= 2,
        "expected both runs' temporaries to be observed; saw {by_name:?}. If \
         the watcher is too slow to catch them this test is asserting \
         nothing — raise its poll count rather than weaken the assertion."
    );
}

/// And the temp file must be gone afterwards, whatever happened.
#[test]
fn the_patch_temp_file_is_removed() {
    let (_tmp, dir) = repo();
    let patch = diff_for(&dir, "ONLY-ONE");
    niki::output::git::apply_diff_to_working_tree(&dir, &patch).expect("apply");

    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("read_dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".patch"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "the temporary patch must be removed on every path, including the \
         failure path: {leftovers:?}"
    );
    assert!(
        std::fs::read_to_string(dir.join("shared.txt"))
            .expect("read")
            .contains("ONLY-ONE"),
        "and the patch must have been applied"
    );
}

/// A crash leaves the temp file behind, so it must be ignored by git —
/// otherwise a failed run's patch is one `git add -A` from a commit.
#[test]
fn a_leftover_patch_file_is_ignored_by_git() {
    let (_tmp, dir) = repo();
    // A previous run wrote the exclude; then this one was killed and left its
    // patch behind. The exclude is the product's, written by its own code —
    // the first version of this test hand-wrote the file and never called
    // `ensure_patch_files_ignored`, so it asserted against a repository the
    // product had never touched.
    niki::output::git::ensure_patch_files_ignored(&dir);
    std::fs::write(dir.join(".niki-tmp.999.12345.patch"), "diff\n").expect("write");

    git(&dir, &["add", "-A"]);
    let staged = git(&dir, &["diff", "--cached", "--name-only"]);
    assert!(
        !staged.contains(".niki-tmp"),
        "a leftover patch from a killed run was staged by `git add -A` — the \
         same hazard as the sandbox directory, one file smaller. Staged: \
         {staged:?}"
    );
    // And the user's own file is still stageable.
    std::fs::write(dir.join("notes.md"), "work\n").expect("write");
    git(&dir, &["add", "-A"]);
    let staged = git(&dir, &["diff", "--cached", "--name-only"]);
    assert!(
        staged.contains("notes.md"),
        "the ignore must not swallow the user's own work: {staged:?}"
    );
}

/// And the name must be unique per caller, which is the property the whole
/// slice is about.
/// NIKI's own repository must cover the pattern. It runs `niki` inside itself,
/// which is the exact scenario, and its `.gitignore` had the *old* fixed name
/// — so a crashed run's uniquely-named patch would have been stageable here
/// while the pattern that would have covered it sat unused.
#[test]
fn this_repository_ignores_every_patch_temporary() {
    let ignore = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(".gitignore"))
        .expect(".gitignore must be readable");
    assert!(
        ignore
            .lines()
            .map(|l| l.trim())
            .any(|l| l == ".niki-tmp*.patch"),
        "this repository's .gitignore must cover `.niki-tmp*.patch`. It has \
         `.niki-tmp.patch` — the name the old fixed path used — which matches \
         none of the per-caller names now written. Current:\n{ignore}"
    );
}

#[test]
fn the_patch_path_is_unique_per_caller() {
    let src =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/output/git.rs"))
            .expect("git.rs must be readable");

    assert!(
        !src.contains("repo_path.join(\".niki-tmp.patch\")"),
        "the temp path is fixed again: two concurrent runs share one file and \
         can apply each other's patch"
    );
    let fn_body = src
        .split("fn patch_temp_path(")
        .nth(1)
        .and_then(|r| r.split("\n}").next())
        .expect("patch_temp_path must exist");
    assert!(
        fn_body.contains("std::process::id()") && fn_body.contains("as_nanos()"),
        "the name must be unique per writer and per call, or two runs collide"
    );
}
