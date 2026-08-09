//! Hermetic safety proof (BUILD_PLAN Phase 1, slice 1.1).
//!
//! NIKI's core promise is that your *committed* repository state is never
//! mutated. The Coder's work lands on a brand-new `niki/<id>` branch; everything
//! else — your HEAD commit and your existing branches — is left byte-for-byte
//! intact. Competitor agents have deleted entire databases and permanently
//! removed files, so this guarantee is the product's single strongest, most
//! defensible differentiator. Rather than assert it, we *prove* it on every run:
//! fingerprint the repo before the pipeline touches it, fingerprint it again
//! after the branch is committed, and report which invariants held.

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// A point-in-time fingerprint of the committed repository state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoSnapshot {
    /// `git rev-parse HEAD` at snapshot time — the base commit anchor (informational).
    pub head_commit: String,
    /// Local branch short-names (`git for-each-ref refs/heads`).
    pub branches: Vec<String>,
    /// Each branch's tip commit, so we can prove no existing ref was re-pointed.
    pub branch_tips: HashMap<String, String>,
    /// `git status --porcelain` output, kept for transparency in the report.
    pub porcelain: String,
    /// Whether the working tree was clean (no staged/unstaged changes).
    pub working_tree_clean: bool,
    /// Last N reflog entries per branch (branch name -> list of reflog lines).
    pub reflog_entries: HashMap<String, Vec<String>>,
}

/// The verifiable result of a hermetic run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SafetyProof {
    /// True only when every hermetic invariant holds.
    pub hermetic: bool,
    /// The new branch is exactly the one NIKI created (`niki/<id>` by default).
    pub branch_added: bool,
    /// Every pre-existing branch still exists and points at the same commit.
    pub existing_branches_preserved: bool,
    /// The new branch's first parent is your pre-run base commit (no rewrite).
    pub new_branch_parent_is_base: bool,
    /// The new branch name that was created.
    pub new_branch: String,
    /// Pre-run working-tree cleanliness (informational).
    pub pre_working_tree_clean: bool,
    /// Post-run working-tree cleanliness (informational).
    pub post_working_tree_clean: bool,
    /// No rebases or force-pushes detected in reflog entries.
    pub no_rebase_or_force_push: bool,
    /// One-line human summary of the blast radius.
    pub blast_radius: String,
    /// Bullet breakdown of each invariant, for the report.
    pub details: Vec<String>,
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").args(args).current_dir(repo).output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Snapshot the committed state of the repository at `repo`.
pub fn snapshot(repo: &Path) -> Result<RepoSnapshot> {
    let head_commit = git(repo, &["rev-parse", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "(unborn)".to_string());
    let branch_lines = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname)",
            "refs/heads",
        ],
    )?;
    let mut branches = Vec::new();
    let mut branch_tips = HashMap::new();
    for line in branch_lines.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let name = parts.next().unwrap_or("").to_string();
        let tip = parts.next().unwrap_or("").to_string();
        if !name.is_empty() {
            branches.push(name.clone());
            branch_tips.insert(name, tip);
        }
    }
    let porcelain = git(repo, &["status", "--porcelain=v1"])?;
    let working_tree_clean = porcelain.trim().is_empty();

    // Capture last N reflog entries per branch for rebase/force-push detection.
    let mut reflog_entries: HashMap<String, Vec<String>> = HashMap::new();
    for branch in &branches {
        let reflog = git(repo, &["reflog", "--no-abbrev", "-n", "10", branch]);
        if let Ok(output) = reflog {
            let lines: Vec<String> = output
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            reflog_entries.insert(branch.clone(), lines);
        }
    }

    Ok(RepoSnapshot {
        head_commit,
        branches,
        branch_tips,
        porcelain,
        working_tree_clean,
        reflog_entries,
    })
}

fn short(sha: &str) -> String {
    sha.get(..7).unwrap_or(sha).to_string()
}

/// Verify hermeticity: compare the pre-run `snapshot` against the live repo
/// after the branch has been created. `branch_name` is the branch NIKI was
/// expected to add, and `task_id` is used only for the human-readable summary.
/// When `strict` is true, any invariant failure causes an error return.
pub fn prove(
    pre: &RepoSnapshot,
    repo: &Path,
    branch_name: &str,
    task_id: &str,
    strict: bool,
) -> Result<SafetyProof> {
    let post = snapshot(repo)?;

    let added: Vec<&String> = post
        .branches
        .iter()
        .filter(|b| !pre.branches.contains(b))
        .collect();
    let removed: Vec<&String> = pre
        .branches
        .iter()
        .filter(|b| !post.branches.contains(b))
        .collect();

    let branch_added = added.len() == 1 && added[0] == branch_name;

    // Every pre-existing branch must still exist and point at the same commit.
    // This is the real hermetic guarantee: your existing refs are never touched.
    let existing_branches_preserved = removed.is_empty()
        && pre.branches.iter().all(|b| {
            post.branches.contains(b)
                && pre
                    .branch_tips
                    .get(b)
                    .map(|t| post.branch_tips.get(b) == Some(t))
                    .unwrap_or(false)
        });

    // The new branch's first parent must be the pre-run base commit, proving no
    // history was rewritten. In an unborn repo there is no parent to check.
    let new_branch_parent_is_base = if pre.head_commit == "(unborn)" {
        post.branches.contains(&branch_name.to_string())
    } else {
        match git(repo, &["rev-parse", &format!("{}^", branch_name)]) {
            Ok(parent) => parent.trim() == pre.head_commit,
            Err(_) => false,
        }
    };

    // Check reflog entries for rebase or force-push activity.
    let mut no_rebase_or_force_push = true;
    let mut rebase_or_force_details = Vec::new();
    for (branch, entries) in &post.reflog_entries {
        if branch == branch_name {
            continue;
        }
        if !pre.branches.contains(branch) {
            continue;
        }
        for entry in entries {
            let lower = entry.to_lowercase();
            if lower.contains("rebase") {
                no_rebase_or_force_push = false;
                rebase_or_force_details.push(format!("rebase detected on `{}`: {}", branch, entry));
            }
            if lower.contains("force push") || lower.contains("forced update") {
                no_rebase_or_force_push = false;
                rebase_or_force_details
                    .push(format!("force push detected on `{}`: {}", branch, entry));
            }
        }
    }

    let hermetic = branch_added
        && existing_branches_preserved
        && new_branch_parent_is_base
        && no_rebase_or_force_push;

    let mut details = Vec::new();
    details.push(format!(
        "{} Existing branch(es) preserved at the same commit ({} before / {} after).",
        if existing_branches_preserved {
            "PASS"
        } else {
            "FAIL"
        },
        pre.branches.len(),
        pre.branches.len() - removed.len()
    ));
    details.push(format!(
        "{} Exactly one new branch added: `{}`.",
        if branch_added { "PASS" } else { "FAIL" },
        branch_name
    ));
    details.push(format!(
        "{} New branch parents your base commit `{}` (no history rewrite).",
        if new_branch_parent_is_base {
            "PASS"
        } else {
            "FAIL"
        },
        short(&pre.head_commit)
    ));
    details.push(format!(
        "{} No rebase or force-push detected in reflog.",
        if no_rebase_or_force_push {
            "PASS"
        } else {
            "FAIL"
        }
    ));
    for d in &rebase_or_force_details {
        details.push(format!("  -> {}", d));
    }

    let blast_radius = if hermetic {
        // Honest wording (research report S9): NIKI intentionally mutates the host
        // working tree (to apply the diff so the user can review it). The guarantee
        // is about *committed* state — existing branches and history.
        format!(
            "Hermetic: existing branches and history untouched. Your {} existing branch(es) are \
             intact at the same commits; only `{}` was added (parented on base commit `{}`). Task {}.",
            pre.branches.len(),
            branch_name,
            short(&pre.head_commit),
            task_id
        )
    } else {
        format!(
            "NON-HERMETIC: committed state changed during the run. Review the details below. Task {}.",
            task_id
        )
    };

    let proof = SafetyProof {
        hermetic,
        branch_added,
        existing_branches_preserved,
        new_branch_parent_is_base,
        new_branch: branch_name.to_string(),
        pre_working_tree_clean: pre.working_tree_clean,
        post_working_tree_clean: post.working_tree_clean,
        no_rebase_or_force_push,
        blast_radius,
        details,
    };

    if strict && !proof.hermetic {
        return Err(anyhow!(
            "Strict safety mode: hermetic invariants broken. {}",
            proof.blast_radius
        ));
    }

    Ok(proof)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn short_truncates_to_seven() {
        assert_eq!(short("0123456789abcdef"), "0123456");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn serde_round_trips() {
        let proof = SafetyProof {
            hermetic: true,
            branch_added: true,
            existing_branches_preserved: true,
            new_branch_parent_is_base: true,
            new_branch: "niki/abc12345".to_string(),
            pre_working_tree_clean: true,
            post_working_tree_clean: true,
            no_rebase_or_force_push: true,
            blast_radius: "Hermetic".to_string(),
            details: vec!["PASS x".to_string()],
        };
        let json = serde_json::to_string(&proof).unwrap();
        let back: SafetyProof = serde_json::from_str(&json).unwrap();
        assert!(back.hermetic);
        assert_eq!(back.new_branch, "niki/abc12345");
    }

    /// End-to-end: init a repo, snapshot the base state, then add exactly one
    /// branch parented on the base commit, and assert the proof reports a
    /// hermetic run.
    #[test]
    fn prove_detects_hermetic_run() {
        let dir = std::env::temp_dir().join(format!("niki-safety-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let guard = TestDir(&dir);

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            );
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@niki.local"]);
        run(&["config", "user.name", "niki-test"]);
        fs::write(dir.join("a.txt"), "hello").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "base"]);

        // Snapshot the base state: only `master` exists, working tree clean.
        let pre = snapshot(&dir).unwrap();
        assert_eq!(pre.branches, vec!["master".to_string()]);

        // Simulate NIKI adding exactly one branch parented on the base commit,
        // then committing the Coder's work on top of it.
        run(&["checkout", "-q", "-b", "niki/branch1"]);
        fs::write(dir.join("b.txt"), "implemented").unwrap();
        run(&["add", "b.txt"]);
        run(&["commit", "-q", "-m", "niki implementation"]);
        let proof = prove(&pre, &dir, "niki/branch1", "test", false).unwrap();
        assert!(proof.hermetic, "expected hermetic: {:?}", proof.details);
        assert!(proof.existing_branches_preserved);
        assert!(proof.new_branch_parent_is_base);

        drop(guard);
    }

    /// The detector must flag a run that re-points an existing branch.
    #[test]
    fn prove_detects_non_hermetic_run() {
        let dir = std::env::temp_dir().join(format!("niki-safety-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let guard = TestDir(&dir);

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            );
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@niki.local"]);
        run(&["config", "user.name", "niki-test"]);
        fs::write(dir.join("a.txt"), "hello").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "base"]);

        let pre = snapshot(&dir).unwrap();

        // A bad run mutates `master` AND adds a new branch — exactly what the
        // hermetic guarantee forbids.
        fs::write(dir.join("a.txt"), "mutated").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "mutated master"]);
        run(&["checkout", "-q", "-b", "niki/branch2"]);

        let proof = prove(&pre, &dir, "niki/branch2", "test", false).unwrap();
        assert!(
            !proof.hermetic,
            "expected NON-hermetic: {:?}",
            proof.details
        );
        assert!(!proof.existing_branches_preserved);

        drop(guard);
    }

    /// Snapshot captures reflog entries for each branch.
    #[test]
    fn snapshot_captures_reflog_entries() {
        let dir = std::env::temp_dir().join(format!("niki-safety-reflog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let guard = TestDir(&dir);

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(out.status.success());
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@niki.local"]);
        run(&["config", "user.name", "niki-test"]);
        fs::write(dir.join("a.txt"), "hello").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "initial"]);

        // Make a second commit so there's reflog history.
        fs::write(dir.join("a.txt"), "world").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "second"]);

        let pre = snapshot(&dir).unwrap();
        assert!(
            pre.reflog_entries.contains_key("master"),
            "reflog should contain master branch"
        );
        let entries = &pre.reflog_entries["master"];
        assert!(
            entries.len() >= 2,
            "expected at least 2 reflog entries, got {}",
            entries.len()
        );
        assert!(
            entries[0].contains("commit:"),
            "reflog entry should mention commit"
        );

        drop(guard);
    }

    /// Prove detects rebase in reflog and marks no_rebase_or_force_push false.
    #[test]
    fn prove_detects_rebase_in_reflog() {
        let dir = std::env::temp_dir().join(format!("niki-safety-rebase-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let guard = TestDir(&dir);

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(out.status.success());
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@niki.local"]);
        run(&["config", "user.name", "niki-test"]);
        fs::write(dir.join("a.txt"), "hello").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "initial"]);

        // Create a second branch to rebase.
        run(&["checkout", "-q", "-b", "feature"]);
        fs::write(dir.join("b.txt"), "feature").unwrap();
        run(&["add", "b.txt"]);
        run(&["commit", "-q", "-m", "feature commit"]);
        run(&["checkout", "-q", "master"]);

        // Simulate a rebase by rewriting history.
        run(&["checkout", "-q", "-b", "feature-rebased"]);
        run(&["rebase", "master"]);

        let pre = snapshot(&dir).unwrap();

        // Simulate NIKI adding its branch.
        run(&["checkout", "-q", "-b", "niki/test-rebase"]);
        fs::write(dir.join("c.txt"), "niki work").unwrap();
        run(&["add", "c.txt"]);
        run(&["commit", "-q", "-m", "niki work"]);

        // The feature branch's reflog should contain a rebase entry.
        let proof = prove(&pre, &dir, "niki/test-rebase", "test", false).unwrap();
        assert!(
            !proof.no_rebase_or_force_push,
            "expected rebase detected in reflog"
        );
        assert!(!proof.hermetic, "expected NON-hermetic due to rebase");

        drop(guard);
    }

    /// Strict mode returns an error when hermetic invariants are broken.
    #[test]
    fn strict_mode_fails_on_non_hermetic() {
        let dir = std::env::temp_dir().join(format!("niki-safety-strict-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let guard = TestDir(&dir);

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(out.status.success());
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@niki.local"]);
        run(&["config", "user.name", "niki-test"]);
        fs::write(dir.join("a.txt"), "hello").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "base"]);

        let pre = snapshot(&dir).unwrap();

        // Mutate master to break hermetic invariant.
        fs::write(dir.join("a.txt"), "mutated").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "mutated master"]);
        run(&["checkout", "-q", "-b", "niki/strict-test"]);

        let result = prove(&pre, &dir, "niki/strict-test", "test", true);
        assert!(
            result.is_err(),
            "strict mode should return error on non-hermetic run"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Strict safety mode"),
            "error message should mention strict safety: {}",
            err_msg
        );

        drop(guard);
    }

    /// Strict mode succeeds when hermetic invariants hold.
    #[test]
    fn strict_mode_passes_on_hermetic() {
        let dir =
            std::env::temp_dir().join(format!("niki-safety-strict-ok-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let guard = TestDir(&dir);

        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(out.status.success());
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@niki.local"]);
        run(&["config", "user.name", "niki-test"]);
        fs::write(dir.join("a.txt"), "hello").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "base"]);

        let pre = snapshot(&dir).unwrap();

        run(&["checkout", "-q", "-b", "niki/strict-ok"]);
        fs::write(dir.join("b.txt"), "work").unwrap();
        run(&["add", "b.txt"]);
        run(&["commit", "-q", "-m", "niki work"]);

        let proof = prove(&pre, &dir, "niki/strict-ok", "test", true).unwrap();
        assert!(proof.hermetic, "expected hermetic in strict mode");
        assert!(proof.no_rebase_or_force_push);

        drop(guard);
    }

    struct TestDir<'a>(&'a std::path::Path);
    impl<'a> Drop for TestDir<'a> {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0);
        }
    }
}
