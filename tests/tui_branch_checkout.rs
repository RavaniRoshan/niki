//! The branch a run hands back must be reachable from the TUI.
//!
//! A run's deliverable is a `niki/<id>` branch. Before this, `/branch <name>`
//! in the TUI printed "Switching branches from the TUI is on the roadmap, not
//! wired yet ... no git command was run" — so the product's central handoff was
//! the one thing it would not do, and the only route to it was leaving the
//! interface for a shell.
//!
//! These run against a real temporary repository and real `git`. A fake would
//! not catch the two things that actually make this safe, both of which are
//! properties of git's argument parsing:
//!
//! - a name starting with `-` is read by git as one of its own options, and
//!   `git checkout -f --` discards uncommitted changes to tracked files while
//!   exiting 0; and
//! - `git checkout <file>` exits 0 having switched nothing, so a name that
//!   resolves to a file reads as a successful switch.

use std::path::{Path, PathBuf};
use std::process::Command;

use niki::session::branch::{checkout, current_branch, validate_branch_name};

fn git(dir: &Path, args: &[&str]) {
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
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .unwrap_or_else(|e| panic!("{name} must be readable: {e}"))
}

/// A repo on `main` with a `niki/<id>` branch, as a completed run leaves it.
fn repo_with_a_run_branch() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-q", "-b", "main", "."]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "t"]);
    std::fs::write(dir.join("a.txt"), "committed\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "init"]);
    git(&dir, &["branch", "niki/abc123"]);
    (tmp, dir)
}

#[test]
fn a_run_branch_is_reachable_from_the_tui() {
    let (_tmp, dir) = repo_with_a_run_branch();
    assert_eq!(current_branch(&dir).as_deref(), Some("main"));

    let after = checkout(&dir, "niki/abc123").expect("the run's branch must check out");

    assert_eq!(after, "niki/abc123");
    // Verified against the repository, not against the return value: a
    // checkout that reported success without moving HEAD is exactly the shape
    // of defect this slice removes.
    assert_eq!(current_branch(&dir).as_deref(), Some("niki/abc123"));
}

#[test]
fn a_leading_dash_never_reaches_git() {
    let (_tmp, dir) = repo_with_a_run_branch();
    // Uncommitted work that `-f` would throw away.
    std::fs::write(dir.join("a.txt"), "UNCOMMITTED WORK\n").unwrap();

    let result = checkout(&dir, "-f");

    // Asserted before the return value, because the return value is the lesser
    // claim. With the guard removed, git runs `git checkout -f --`, discards
    // the edit and exits 0 — and `checkout` then reports the branch it was
    // already on, so a user reading the transcript sees a successful switch.
    assert_eq!(
        read(&dir, "a.txt"),
        "UNCOMMITTED WORK\n",
        "git read the name as `-f` and discarded the uncommitted edit, exiting 0"
    );
    let err = result.expect_err("a leading dash must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("'-'") && msg.contains("options"),
        "the refusal has to explain why, or the user reads it as a bug: {msg}"
    );
    assert_eq!(current_branch(&dir).as_deref(), Some("main"));

    // Measured, not assumed — this is what makes the guard necessary:
    // uncommenting the next line loses the edit and still exits 0.
    // std::process::Command::new("git").args(["checkout", "-f", "--"]).current_dir(&dir).status();
}

#[test]
fn a_name_that_is_a_file_is_not_a_successful_switch() {
    let (_tmp, dir) = repo_with_a_run_branch();
    // `git checkout a.txt` exits 0 and switches nothing; without the trailing
    // `--` this call would return Ok and report a branch the user is not on.
    let err = checkout(&dir, "a.txt").expect_err("a file is not a branch");
    assert!(
        !err.to_string().is_empty(),
        "git's own refusal must reach the user verbatim"
    );
    assert_eq!(current_branch(&dir).as_deref(), Some("main"));
}

#[test]
fn a_nonexistent_branch_reports_git_not_success() {
    let (_tmp, dir) = repo_with_a_run_branch();
    let err = checkout(&dir, "niki/does-not-exist").expect_err("no such branch");
    assert!(
        !err.to_string().is_empty(),
        "a failed checkout must carry a message: {err}"
    );
    assert_eq!(current_branch(&dir).as_deref(), Some("main"));
}

#[test]
fn a_detached_head_is_reported_rather_than_called_a_branch() {
    let (_tmp, dir) = repo_with_a_run_branch();
    let head = {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&dir, &["checkout", "-q", &head]);
    // A hash is a legitimate thing to ask for, and it leaves HEAD detached.
    // Reporting "now on <hash>" without saying so is the small lie.
    assert!(current_branch(&dir).is_none());
    let after = checkout(&dir, "main").expect("switching back works");
    assert_eq!(after, "main");
}

/// The validation itself, so the rule is pinned even where git is not.
#[test]
fn the_name_is_validated_before_it_is_an_argument() {
    assert!(validate_branch_name("-f").is_err());
    assert!(validate_branch_name("--detach").is_err());
    assert!(validate_branch_name("a b").is_err());
    assert!(validate_branch_name("a\nb").is_err());
    assert!(validate_branch_name("").is_err());
    assert_eq!(validate_branch_name("  niki/abc  ").unwrap(), "niki/abc");
}

/// The TUI command is a real command, not another stub.
#[test]
fn the_tui_branch_command_is_wired_and_listed() {
    let s = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/pages/chat.rs"),
    )
    .expect("chat.rs must be readable");

    assert!(
        !s.contains("Switching branches from the TUI {NOT_WIRED}"),
        "`/branch` was advertising a stub while the run's whole deliverable sat \
         behind it"
    );
    assert!(
        s.contains("crate::session::branch::checkout("),
        "`/branch <name>` must actually call git"
    );
    assert!(
        s.contains("crate::session::branch::current_branch("),
        "`/branch` with no argument must read HEAD, not the run's remembered \
         branch — the two differ after a checkout"
    );
    assert!(
        niki::display::pages::chat::HELP_TEXT.contains("/branch <name>"),
        "a working command a user cannot find in /help is not reachable"
    );
}
