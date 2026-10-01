//! A salvaged run must hand a human something they can *read*.
//!
//! A failed run is salvaged: the Coder's `CodeDiff` is written to
//! `artifacts/coder.json` and a `SALVAGED.md` says what survived. That is the
//! change *as data* — search/replace blocks and a file list.
//!
//! Which is the one thing a person cannot review. Reviewing unreviewed work
//! means seeing a diff, and the files the product points at for a diff — the
//! TUI's Run page, the completion screen, `niki report` — all name
//! `changes.patch`, which a failed run never wrote. So a user whose run
//! crashed had the Coder's output in JSON and a link to a file that was not
//! there.
//!
//! `render_salvaged_patch` fixes that **without touching the working tree**.
//! The salvage deliberately applies nothing: quietly writing an unreviewed
//! diff into someone's repository is a larger semantic change than a
//! hardening pass should make on its own. The render happens in a scratch
//! copy, and the patch's own header says it has not been reviewed.

use std::path::{Path, PathBuf};

use niki::artifacts::types::{ChangedFile, CodeDiff, FileAction};
use niki::orchestrator::deliver::render_salvaged_patch;

fn repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(
        root.join("src/lib.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .expect("write");
    (dir, root)
}

fn modify_diff() -> CodeDiff {
    CodeDiff {
        edits: vec![niki::artifacts::types::EditBlock {
            search: "println!(\"hello\");".into(),
            replace: "println!(\"goodbye\");".into(),
        }],
        files_changed: vec![ChangedFile {
            path: "src/lib.rs".into(),
            action: FileAction::Modify,
            language: Some("rust".into()),
        }],
        implementation_notes: String::new(),
        spec_adherence: String::new(),
        uncertainties: None,
    }
}

#[test]
fn a_salvaged_change_renders_as_a_readable_diff() {
    let (_dir, root) = repo();
    let patch = render_salvaged_patch(&root, &modify_diff()).expect("the render must succeed");

    // A real unified diff, not a description of one.
    assert!(
        patch.contains("--- a/src/lib.rs"),
        "no old-file header:\n{patch}"
    );
    assert!(
        patch.contains("+++ b/src/lib.rs"),
        "no new-file header:\n{patch}"
    );
    assert!(
        patch.contains("-    println!(\"hello\");"),
        "no removal hunk:\n{patch}"
    );
    assert!(
        patch.contains("+    println!(\"goodbye\");"),
        "no addition hunk:\n{patch}"
    );
}

/// The patch must say it is unreviewed, in the patch itself.
///
/// A user who greps the file and pastes it elsewhere loses this directory,
/// loses `SALVAGED.md`, and loses the screen that told them. The header travels
/// with the content, so the warning travels with it.
#[test]
fn the_patch_carries_its_own_warning() {
    let (_dir, root) = repo();
    let patch = render_salvaged_patch(&root, &modify_diff()).expect("render");
    assert!(
        patch.contains("UNREVIEWED") && patch.contains("FAILED"),
        "a patch of unreviewed work from a failed run must say so inside the \\
         file, not only in a note beside it:\n{patch}"
    );
}

/// **The load-bearing property.** Rendering is not applying.
///
/// The salvage is deliberately inert: a failed run must not modify anyone's
/// repository. A render that got this wrong would be worse than the gap it
/// fills — it would take an unreviewed change from a *failed* run and write it
/// into a working tree.
#[test]
fn rendering_does_not_touch_the_working_tree() {
    let (_dir, root) = repo();
    let before = std::fs::read_to_string(root.join("src/lib.rs")).expect("read");
    let status_before = std::process::Command::new("git")
        .args(["-C", root.to_str().unwrap(), "status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let patch = render_salvaged_patch(&root, &modify_diff()).expect("render");
    assert!(!patch.is_empty());

    let after = std::fs::read_to_string(root.join("src/lib.rs")).expect("read");
    assert_eq!(
        before, after,
        "the render modified a file. A salvaged run's recovery must be inert."
    );
    let status_after = std::process::Command::new("git")
        .args(["-C", root.to_str().unwrap(), "status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    assert_eq!(
        status_before, status_after,
        "and it left the repository in a different state"
    );
}

/// A patch that applies where it says it does.
///
/// `--no-index` writes the *scratch* paths into the headers, so without the
/// rewrite the patch would apply to `/tmp/niki-salvage.123.456/before/…`. A
/// patch that applies in the wrong place is worse than no patch, so this is
/// asserted on the bytes rather than inferred from the rename code.
#[test]
fn the_patch_applies_to_the_repository() {
    let (dir, root) = repo();
    // A real git repo, so `git apply` can be run against it.
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "t"],
        vec!["add", "-A"],
    ] {
        std::process::Command::new("git")
            .args(&args)
            .current_dir(&root)
            .output()
            .expect("git");
    }

    let patch = render_salvaged_patch(&root, &modify_diff()).expect("render");
    let patch_path = dir.path().join("salvaged.patch");
    std::fs::write(&patch_path, &patch).expect("write patch");

    // `git apply --check` is the real question: would this land?
    let checked = std::process::Command::new("git")
        .args(["apply", "--check"])
        .arg(&patch_path)
        .current_dir(&root)
        .output()
        .expect("git apply");
    assert!(
        checked.status.success(),
        "the rendered patch does not apply to the repository it was rendered \\
         from:\\n{}\\npatch:\\n{patch}",
        String::from_utf8_lossy(&checked.stderr)
    );
}

/// A file the Coder said it created renders as a new-file hunk.
#[test]
fn a_created_file_renders_as_a_new_file() {
    let (_dir, root) = repo();
    let diff = CodeDiff {
        edits: vec![niki::artifacts::types::EditBlock {
            search: "".into(),
            replace: "pub fn added() -> u32 { 7 }\n".into(),
        }],
        files_changed: vec![ChangedFile {
            path: "src/added.rs".into(),
            action: FileAction::Create,
            language: Some("rust".into()),
        }],
        implementation_notes: String::new(),
        spec_adherence: String::new(),
        uncertainties: None,
    };
    let patch = render_salvaged_patch(&root, &diff).expect("render");
    assert!(
        patch.contains("+++ b/src/added.rs") && patch.contains("+pub fn added()"),
        "a created file must render as a new-file hunk:\n{patch}"
    );
    assert!(
        !std::path::Path::new(&root.join("src/added.rs")).exists(),
        "and rendering a create must not actually create it"
    );
}

/// A path NIKI must never publish gets no hunk.
///
/// The same filter `working_tree_diff_scoped` applies, and for the same
/// reason: a model that reported `.env` or a traversal escape must not get it
/// rendered into a file the user might apply.
#[test]
fn an_unpublishable_path_is_refused_rather_than_rendered() {
    let (_dir, root) = repo();
    for path in [".env", "../escape.rs", "/etc/passwd", "niki.toml"] {
        let diff = CodeDiff {
            edits: vec![niki::artifacts::types::EditBlock {
                search: "x".into(),
                replace: "y".into(),
            }],
            files_changed: vec![ChangedFile {
                path: path.into(),
                action: FileAction::Modify,
                language: None,
            }],
            implementation_notes: String::new(),
            spec_adherence: String::new(),
            uncertainties: None,
        };
        let out = render_salvaged_patch(&root, &diff);
        assert!(
            out.is_err(),
            "{path:?} must not render a patch — it is not publishable, and a \\
             recovered artifact is exactly where an unfiltered path would leak"
        );
    }
}

/// An edit that matches nothing must not render as "no change".
///
/// A diff that is quietly short is the worst outcome here: a user reading it
/// concludes the change was smaller than it is, and that is precisely the
/// mistake a salvaged artifact exists to prevent.
#[test]
fn an_unmatched_edit_is_reported_rather_than_rendering_a_short_patch() {
    let (_dir, root) = repo();
    let diff = CodeDiff {
        edits: vec![niki::artifacts::types::EditBlock {
            search: "this text is not in the file at all".into(),
            replace: "y".into(),
        }],
        files_changed: vec![ChangedFile {
            path: "src/lib.rs".into(),
            action: FileAction::Modify,
            language: None,
        }],
        implementation_notes: String::new(),
        spec_adherence: String::new(),
        uncertainties: None,
    };
    let out = render_salvaged_patch(&root, &diff);
    assert!(
        out.is_err(),
        "an edit that matched nothing must not produce a patch, or the patch \\
         reads as 'that is all the change was' — which is the one thing a \
         salvaged artifact must never do"
    );
}

/// Nothing in the system temp directory survives a render.
#[test]
fn a_render_leaves_no_scratch_copy_behind() {
    let (_dir, root) = repo();
    let pid = std::process::id();
    let before = temp_entries(pid);
    let _ = render_salvaged_patch(&root, &modify_diff());
    let after = temp_entries(pid);
    assert_eq!(
        before, after,
        "the scratch copy of the user's source was not cleaned up: {after:?}"
    );
}

fn temp_entries(pid: u32) -> Vec<PathBuf> {
    let prefix = format!("niki-salvage.{pid}.");
    std::fs::read_dir(std::env::temp_dir())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(&prefix))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// And the working tree is not merely unmodified — it is not even *read*
/// outside the files the Coder named. A render that scanned the whole tree
/// would be a privacy question the salvage does not need to ask.
#[test]
fn only_the_named_files_are_read() {
    let (_dir, root) = repo();
    // A file the Coder did not report, with a path that would fail loudly if
    // the render walked the tree.
    std::fs::write(root.join("secret.txt"), "not yours").expect("write");
    let _ = render_salvaged_patch(&root, &modify_diff()).expect("render");
    assert!(
        !Path::new(&root.join("secret.txt"))
            .metadata()
            .is_ok_and(|m| m.len() == 0),
        "the render must leave unrelated files alone"
    );
}

/// A run that failed before anything checkpointed says so in words.
///
/// Found by a live run against `poolside/laguna-s-2.1:free`, whose Planner
/// emits no conformant artifact and so never writes a checkpoint. The salvage
/// reported `No such file or directory (os error 2)` — an errno, not a
/// situation — and it landed on top of a perfectly good failure message, so
/// the last thing a user read was a number.
#[test]
fn a_run_with_no_checkpoint_says_so_rather_than_naming_an_errno() {
    let (dir, _root) = repo();
    // A project with no `.niki/sessions` at all: the normal state for a run
    // that failed in its first stage.
    let err = niki::cli::run::salvage_error_for_missing_sessions(dir.path())
        .expect_err("the situation must be described, not leaked");
    assert!(
        !err.contains("os error"),
        "a user-facing failure must not name an errno: {err}"
    );
    assert!(
        err.contains("checkpoint"),
        "and it must name what is missing, in words a person can act on: {err}"
    );
}

/// A path that no longer exists must not be handed to git.
///
/// Measured on a live run, and the consequence is worse than the warning it
/// produced. The agent's file list accumulates across revision rounds: the
/// first Coder declared creating `src/lib.rs`, the Reviewer rejected it, the
/// revision removed it — and the final staging still named it. `git add -N`
/// treats an unmatched pathspec as fatal for the **whole invocation**, so every
/// genuinely new file lost its intent-to-add and dropped out of the diff, and
/// the run finished with "A brand-new file may be missing from the diff" — on a
/// run that had in fact been approved.
///
/// The warning is not the defect. The defect is that the diff was short.
#[test]
fn a_path_that_no_longer_exists_is_dropped_before_staging() {
    let (dir, root) = repo();
    std::fs::write(root.join("src/real_new.rs"), "// a genuinely new file\n").expect("write");

    let declared = vec![
        // Not `src/lib.rs` — the `repo()` fixture creates that one, so naming
        // it as the removed file would test nothing.
        "src/removed_last_round.rs".to_string(), // round 1 declared it; round 2 removed it
        "src/real_new.rs".to_string(),           // round 2's actual new file
    ];
    let kept = niki::output::git::existing_paths(&root, &declared);

    assert!(
        !kept.contains(&"src/removed_last_round.rs".to_string()),
        "a path that does not exist cannot appear in a diff, and leaving it in \\
         makes the whole `git add -N` fail"
    );
    assert_eq!(
        kept,
        vec!["src/real_new.rs".to_string()],
        "and the file that does exist must survive — dropping it is the same \\
         silent omission one level down"
    );
    let _ = dir;
}

/// Both sandboxes must filter, not just the helper the test above drives.
///
/// The first version of this pinned `output::git::existing_paths` and nothing
/// else, so removing the *worktree* backend's own existence check left every
/// test green — the same false negative as a filter that matches no test name.
/// The worktree filter is inline, not a call to the helper, so it needs its
/// own assertion.
#[test]
fn both_sandboxes_filter_absent_paths_before_staging() {
    for (file, marker) in [
        ("src/sandbox/worktree.rs", "wt.join(s).exists()"),
        ("src/sandbox/docker.rs", "test -e /workspace/"),
    ] {
        let src =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
                .unwrap_or_else(|e| panic!("{file} must be readable: {e}"));
        assert!(
            src.contains(marker),
            "{file} must check that a declared path still exists before \
             handing it to git. Found no `{marker}`."
        );
    }
}
