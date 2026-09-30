//! NIKI's own directories must not end up in the user's commits.
//!
//! The worktree backend keeps its sandboxes in `.niki-worktrees/` **inside the
//! user's project**, and a SIGKILL leaves one behind holding a complete copy of
//! their repository. The user's next `git add -A` then commits all of it. This
//! is the shape of the bug: a crash in a tool turns into a large, silent,
//! permanent diff in someone else's history.
//!
//! Two independent defences, because they cover different things:
//!
//! - `is_publishable_path` refuses the path, so an agent that *reports* a
//!   sandbox path as a change it made cannot get it into the published diff.
//!   This is the same rule that already refuses `.niki/` and `.git/`.
//! - `.git/info/exclude` gains the entry when the directory is created, so
//!   git itself skips it. That file is **per-clone and never committed** —
//!   NIKI writes nothing the user reviews or shares. The alternative, appending
//!   to their tracked `.gitignore`, is a change to their repository made by a
//!   tool they ran once.
//!
//! Real git, real repository: a `git add -A` in a project with a leftover
//! sandbox must stage the user's file and not the sandbox's.

use std::path::Path;
use std::process::Command;

use niki::output::git::is_publishable_path;
use niki::sandbox::worktree::{WORKTREE_DIR, ensure_git_excluded};

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

/// A repository with a committed file and a leftover sandbox.
fn repo_with_sandbox() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-q", "-b", "main", "."]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("src.rs"), "fn main() {}\n").expect("write source");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "init"]);
    // Written *after* the commit, so it is untracked and `git add -A` has
    // something to stage. The first version committed it, staged nothing, and
    // the "the user's own file must still be stageable" assertion failed —
    // for the right reason, but not the one it was written for.
    std::fs::write(dir.join("notes.md"), "work in progress\n").expect("write new file");

    // What a SIGKILL leaves behind: a complete copy of the project.
    let sandbox = dir.join(WORKTREE_DIR).join("abc123");
    std::fs::create_dir_all(&sandbox).expect("create sandbox");
    std::fs::write(sandbox.join("src.rs"), "// a copy nobody asked for\n").expect("write copy");
    (tmp, dir)
}

/// **The defect, measured.** A leftover sandbox must not be staged by
/// `git add -A`.
#[test]
fn a_leftover_sandbox_is_not_staged_by_git_add() {
    let (_tmp, dir) = repo_with_sandbox();

    // Before the fix, this is the state a user is left in.
    ensure_git_excluded(&dir, WORKTREE_DIR).expect("exclude write succeeds in a real repo");

    git(&dir, &["add", "-A"]);
    let staged = git(&dir, &["diff", "--cached", "--name-only"]);

    assert!(
        staged.lines().any(|l| l.trim() == "notes.md"),
        "the user's own file must still be stageable — the exclude is for the \
         sandbox, not for their work: {staged:?}"
    );
    assert!(
        !staged.contains(WORKTREE_DIR),
        "a leftover sandbox was staged by `git add -A` — after a crash in a tool, \
         the user's next commit would contain a whole copy of their repository. \
         Staged: {staged:?}"
    );
}

/// And the fix is in git's own exclude file, not the user's tracked one.
#[test]
fn the_exclude_is_written_without_touching_the_users_gitignore() {
    let (_tmp, dir) = repo_with_sandbox();
    std::fs::write(dir.join(".gitignore"), "/target\n").expect("user gitignore");

    ensure_git_excluded(&dir, WORKTREE_DIR).expect("exclude write succeeds");

    let exclude = std::fs::read_to_string(dir.join(".git").join("info").join("exclude"))
        .expect("git's exclude file must exist");
    assert!(
        exclude.contains(WORKTREE_DIR),
        "the sandbox dir must be excluded: {exclude}"
    );
    assert!(
        exclude.contains("added by niki"),
        "and the line must say who wrote it, because a user reading this file \
         will wonder: {exclude}"
    );

    let user = std::fs::read_to_string(dir.join(".gitignore")).expect("user gitignore");
    assert_eq!(
        user, "/target\n",
        "NIKI wrote to the user's tracked .gitignore. That is a change to their \
         repository made by a tool they ran once, and it is not needed: \
         .git/info/exclude is per-clone and never committed."
    );
}

/// Idempotent, and it says whether it wrote.
#[test]
fn the_exclude_is_written_once() {
    let (_tmp, dir) = repo_with_sandbox();

    assert!(
        ensure_git_excluded(&dir, WORKTREE_DIR).expect("first write"),
        "the first call must write"
    );
    assert!(
        !ensure_git_excluded(&dir, WORKTREE_DIR).expect("second write"),
        "the second call must be a no-op, or every run appends a line"
    );
    assert!(
        !ensure_git_excluded(&dir, WORKTREE_DIR).expect("third write"),
        "and the third"
    );

    let exclude = std::fs::read_to_string(dir.join(".git").join("info").join("exclude"))
        .expect("exclude file");
    let count = exclude
        .lines()
        .map(|l| l.trim().trim_start_matches('/').trim_end_matches('/'))
        .filter(|l| *l == WORKTREE_DIR)
        .count();
    assert_eq!(count, 1, "one line, not several: {exclude}");
}

/// A user's own spelling of the rule is left alone, not duplicated.
#[test]
fn an_existing_exclude_line_is_not_duplicated() {
    let (_tmp, dir) = repo_with_sandbox();
    let exclude = dir.join(".git").join("info").join("exclude");
    std::fs::create_dir_all(exclude.parent().unwrap()).expect("create .git/info");
    std::fs::write(&exclude, format!("/{WORKTREE_DIR}/\n")).expect("write");

    assert!(
        !ensure_git_excluded(&dir, WORKTREE_DIR).expect("write"),
        "the user already excludes it; writing again is noise in a file they \
         can read"
    );
    // Unchanged, byte for byte. The point of the case is that NIKI adds
    // *nothing* — the earlier version asserted the file contained NIKI's
    // "added by niki" marker, which is the opposite of what it should be when
    // the user has already written the line themselves.
    let text = std::fs::read_to_string(&exclude).expect("read");
    assert_eq!(
        text,
        format!("/{WORKTREE_DIR}/\n"),
        "the file must be untouched: the user already excludes the sandbox, so \
         NIKI has nothing to add. Got: {text}"
    );
}

/// A pre-existing `.gitignore` keeps its contents, and one that did not exist
/// is not created — the exclude file is the only place NIKI writes.
#[test]
fn nothing_else_in_the_project_is_touched() {
    let (_tmp, dir) = repo_with_sandbox();

    // What must not change: any **tracked** file. `git status` does change —
    // the sandbox stops being listed as untracked, which is the fix working —
    // so the first version of this test asserted the whole status was
    // identical and failed on its own success.
    let tracked_before = git(&dir, &["diff", "--stat", "HEAD"]);
    ensure_git_excluded(&dir, WORKTREE_DIR).expect("write");
    assert_eq!(
        git(&dir, &["diff", "--stat", "HEAD"]),
        tracked_before,
        "writing the exclude modified a tracked file in the user's project"
    );
    assert!(
        !dir.join(".gitignore").exists(),
        "NIKI created a .gitignore in the user's project"
    );
    // And nothing but the sandbox stopped being visible.
    let status = git(&dir, &["status", "--porcelain"]);
    assert!(
        !status.contains(WORKTREE_DIR),
        "the sandbox is still visible to git: {status:?}"
    );
    assert!(
        status.contains("notes.md"),
        "the user's own untracked work must still be visible: {status:?}"
    );
}

/// And the publish filter refuses the path, which covers the *other* half: an
/// agent that reports a sandbox path as a change it made.
#[test]
fn a_sandbox_path_is_not_publishable() {
    for path in [
        ".niki-worktrees/abc123/src/lib.rs",
        ".niki-worktrees\\abc123\\src\\lib.rs",
        "./.niki-worktrees/abc123",
    ] {
        assert!(
            !is_publishable_path(path),
            "{path} is inside NIKI's sandbox directory and must not appear in a \
             published diff — a whole copy of the user's repository is not a \
             change they asked for"
        );
    }
    // And a dotfile the user legitimately asked for is still publishable. The
    // earlier rule refused every path starting with a dot, which silently
    // dropped `.github/workflows/*.yml` and `.gitignore` — the same way an
    // over-broad fix here would drop real work.
    for path in [".github/workflows/ci.yml", ".gitignore", ".env.example"] {
        assert!(
            is_publishable_path(path),
            "{path} is ordinary work and must stay publishable"
        );
    }
}

/// This repository must not commit its own sandboxes either. It ran `niki`
/// inside itself, which is the exact scenario.
#[test]
fn this_repository_ignores_its_own_sandboxes() {
    let ignore = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(".gitignore"))
        .expect(".gitignore must be readable");
    assert!(
        ignore
            .lines()
            .map(|l| l.trim().trim_start_matches('/').trim_end_matches('/'))
            .any(|l| l == WORKTREE_DIR),
        "NIKI's own .gitignore must exclude {WORKTREE_DIR}/ — this repository \
         runs niki inside itself, and a crash in a test would otherwise leave a \
         copy of the tree in the next commit. Current:\n{ignore}"
    );
}

/// **The call site.** The other seven tests call `ensure_git_excluded`
/// themselves, so they all passed with the call removed from sandbox creation
/// — which is the defect, because nothing in a real run writes the exclude
/// then. This is the assertion that ties the helper to the product.
#[test]
fn creating_a_sandbox_writes_the_exclude() {
    let body = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sandbox/worktree.rs"),
    )
    .expect("worktree.rs must be readable");

    let create = body
        .split("pub async fn create(")
        .nth(1)
        .or_else(|| body.split(".niki-worktrees\"").next())
        .expect("the sandbox constructor must exist");
    let window: String = create.chars().take(1200).collect();
    assert!(
        window.contains("ensure_git_excluded("),
        "creating a worktree sandbox must write the exclude. Every other test \
         here calls the helper directly, so they stay green when the call \
         site is deleted — and a real run then leaves a copy of the user's \
         repository for their next commit."
    );
    // And it must be best-effort: a repository with no writable `.git` is not
    // a reason to fail a run.
    assert!(
        window.contains("let _ = ensure_git_excluded("),
        "the call must be best-effort, or a read-only or absent .git fails          every run on the recommended backend"
    );
}
