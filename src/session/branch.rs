//! Checking out the branch a run handed back, from the TUI or the CLI.
//!
//! A run's whole deliverable is a `niki/<id>` branch. Until this existed, the
//! only way to get it was to leave the TUI, open a shell, and type `git
//! checkout` — so the product's central handoff was one of the few things it
//! would not do for you.
//!
//! ## Why the argument is validated rather than passed through
//!
//! `git checkout` has no idea which of its arguments is a branch and which is a
//! flag, and it takes them positionally. That makes a user-typed name a way to
//! run a *different git command*:
//!
//! ```text
//! $ git checkout -f --          # in a repo with uncommitted work
//! $ echo $?
//! 0
//! $ cat a.txt                   # the uncommitted edit is gone
//! committed
//! ```
//!
//! `-f` throws away local changes to tracked files and still exits 0. So does
//! `--orphan` (create an orphan branch) and `--detach`. The trailing `--` that
//! makes a branch/path ambiguity safe does **not** help here: in
//! `git checkout -f --` the `-f` is parsed long before the `--` is reached.
//! The only defence is to refuse a name that starts with `-`.
//!
//! Two more shapes are handled, both measured rather than assumed:
//!
//! - `git checkout <name>` where `<name>` is a *file* exits 0 having switched
//!   nothing ("Updated 0 paths from the index"), which reads as success. The
//!   trailing `--` turns it into `fatal: invalid reference: a.txt`.
//! - `git checkout -- <name>` is *not* the fix — it makes git read the name as
//!   a pathspec, so a real branch fails. The separator goes after the name.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Bound on a `git checkout`.
///
/// A local checkout is milliseconds. The bound exists because this runs on the
/// TUI's event thread, where a blocked call is a frozen interface with no
/// timeout and no way out — and because `Command::output()` has no deadline of
/// its own, which is the same defect B2-04 fixed in the tool loop.
const GIT_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a branch could not be checked out, in terms the user can act on.
#[derive(Debug)]
pub enum BranchError {
    /// The name is not something `git checkout` can be handed safely.
    Rejected(String),
    /// git ran and refused. Carries git's own message.
    GitFailed(String),
    /// git did not finish. Carries the bound it exceeded.
    TimedOut(Duration),
    /// The repository could not be reached at all.
    NotARepository(String),
}

impl std::fmt::Display for BranchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(why) => write!(f, "{why}"),
            Self::GitFailed(m) => write!(f, "git refused: {}", m.trim()),
            Self::TimedOut(d) => write!(
                f,
                "git did not finish within {}s and was killed. Another git \
                 process may hold the index lock; `rm .git/index.lock` if you \
                 are sure nothing else is running.",
                d.as_secs()
            ),
            Self::NotARepository(m) => write!(f, "not a git repository: {}", m.trim()),
        }
    }
}

/// Check a user-typed branch name before it reaches git's argv.
///
/// The leading-dash case is the one that destroys work; the rest keep a
/// newline or a control byte out of the transcript and out of a ref that git
/// would have to be told to quote.
pub fn validate_branch_name(name: &str) -> Result<&str, BranchError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(BranchError::Rejected("Usage: /branch <name>".into()));
    }
    if trimmed.starts_with('-') {
        // Checked, not assumed: `git checkout -f --` overwrites uncommitted
        // changes to tracked files and exits 0. `Command::args` does not go
        // through a shell, so this is not shell injection — it is git reading
        // our argument as one of its own options.
        return Err(BranchError::Rejected(format!(
            "'{trimmed}' starts with '-', which git would read as one of its own \
             options rather than a branch — `/branch -f` would run `git checkout -f`, \
             which discards uncommitted changes to tracked files. Branch names cannot \
             start with a dash."
        )));
    }
    if let Some(c) = trimmed.chars().find(|c| c.is_control()) {
        return Err(BranchError::Rejected(format!(
            "a branch name cannot contain the control character {:?}.",
            c
        )));
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(BranchError::Rejected(format!(
            "'{trimmed}' contains whitespace, which a git ref cannot."
        )));
    }
    Ok(trimmed)
}

/// Run a git command with a deadline, returning stdout on success.
fn git_bounded(
    project_dir: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<String, BranchError> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| BranchError::NotARepository(e.to_string()))?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            // Exited between the spawn and the first poll, or is still running.
            Ok(None) => {}
            Err(e) => return Err(BranchError::GitFailed(e.to_string())),
        }
        if Instant::now() >= deadline {
            // Kill, and reap: a killed-but-unreaped child is a zombie that
            // also holds whatever git had open.
            let _ = child.kill();
            let _ = child.wait();
            return Err(BranchError::TimedOut(timeout));
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let out = child
        .wait_with_output()
        .map_err(|e| BranchError::GitFailed(e.to_string()))?;
    if !out.status.success() {
        return Err(BranchError::GitFailed(
            String::from_utf8_lossy(&out.stderr).to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The branch HEAD is on right now, or `None` when detached.
pub fn current_branch(project_dir: &Path) -> Option<String> {
    git_bounded(
        project_dir,
        &["rev-parse", "--abbrev-ref", "HEAD"],
        GIT_TIMEOUT,
    )
    .ok()
    .filter(|s| s != "HEAD")
}

/// Check out `name`, and report the branch that HEAD ended up on.
///
/// Returns the new branch rather than assuming `name`: asking for a tag or a
/// commit hash leaves HEAD detached, and reporting "now on niki/abc" when the
/// repo is actually at a hash is the kind of small lie this pass has been
/// removing elsewhere.
pub fn checkout(project_dir: &Path, name: &str) -> Result<String, BranchError> {
    let name = validate_branch_name(name)?;

    // Trailing `--`, for the branch/path ambiguity and nothing else. Checked
    // against real git: `git checkout <file>` exits 0 having switched nothing,
    // while `git checkout <file> --` is `fatal: invalid reference`.
    git_bounded(project_dir, &["checkout", name, "--"], GIT_TIMEOUT)?;

    match current_branch(project_dir) {
        Some(after) => Ok(after),
        // A detached HEAD is a legitimate outcome, not a failure.
        None => Ok(format!(
            "{name} (HEAD is detached — `git switch` back with a branch name)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_dash_is_refused_before_git_sees_it() {
        // The reason this module exists. `-f` discards uncommitted changes to
        // tracked files and exits 0.
        assert!(validate_branch_name("-f").is_err());
        assert!(validate_branch_name("--orphan").is_err());
        assert!(validate_branch_name("-f --").is_err());
        // ...while the same names as a *branch* are not the problem.
        assert_eq!(validate_branch_name("niki/abc123").unwrap(), "niki/abc123");
        assert_eq!(validate_branch_name("feat/a-b-c").unwrap(), "feat/a-b-c");
    }

    #[test]
    fn an_empty_or_inert_name_is_refused() {
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("   ").is_err());
        assert!(validate_branch_name("a\nb").is_err());
        assert!(validate_branch_name("a\tb").is_err());
        assert!(validate_branch_name("a b").is_err());
    }
}
