use anyhow::Result;
use git2::{Repository, Signature};
use std::path::Path;

/// Run a git subcommand in `repo_path`, returning an error if it fails.
fn run_git(repo_path: &Path, args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "git {} failed (exit {:?})",
            args.join(" "),
            status.code()
        ));
    }
    Ok(())
}

/// Extract the file paths a unified diff touches, so the task commit stages only
/// those files (never pre-existing uncommitted user changes). Parses `+++ b/<path>`
/// lines; paths are made repo-relative (dropping the `a/`/`b/` prefix).
fn diff_files(diff: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for line in diff.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("+++ ") {
            let path = rest.trim();
            // `+++ /dev/null` (deletions) has no b/ path.
            if path.starts_with("b/") {
                let p = path[2..].to_string();
                if !files.contains(&p) {
                    files.push(p);
                }
            } else if !path.starts_with('/')
                && !path.is_empty()
                && !files.contains(&path.to_string())
            {
                files.push(path.to_string());
            }
        }
    }
    files
}

/// Capture the working-tree diff scoped to agent-produced changes.
///
/// Phase 5.1: the old implementation ran `git add -A -N` on the host and
/// diffed everything, so pre-existing unrelated dirt landed in
/// `changes.patch`, the commit, and the final diff. This version diffs ONLY
/// the files the agent reported (`CodeDiff.files_changed`), intent-to-adding
/// just those — the host index is otherwise untouched.
///
/// Paths under `.niki/` / `niki.toml` and escapes (`..`, absolute) are
/// dropped: task artifacts and secrets never belong in the published patch.
/// An empty file list yields an empty diff without touching the host at all.
pub fn working_tree_diff_scoped(repo_path: &Path, agent_files: &[String]) -> String {
    let files: Vec<&str> = agent_files
        .iter()
        .map(|s| s.as_str())
        .filter(|s| {
            !s.is_empty()
                && !s.starts_with('.')
                && !s.starts_with('/')
                && !s.contains("..")
                && *s != "niki.toml"
                && !s.starts_with(".niki/")
                && !s.starts_with(".niki\\")
        })
        .collect();
    if files.is_empty() {
        return String::new();
    }
    // Intent-to-add ONLY agent files that are new on disk, so `git diff`
    // reports them. Pre-existing untracked user dirt stays invisible.
    let new_files: Vec<&str> = files
        .iter()
        .filter(|f| repo_path.join(f).is_file() && is_untracked(repo_path, f))
        .copied()
        .collect();
    if !new_files.is_empty() {
        let mut args = vec!["add", "-N", "--"];
        args.extend(new_files);
        let _ = run_git(repo_path, &args);
    }
    let mut args = vec!["diff", "--"];
    args.extend(files);
    let out = std::process::Command::new("git")
        .args(&args)
        .current_dir(repo_path)
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}

/// True when `path` (repo-relative) is untracked in the host repo.
fn is_untracked(repo_path: &Path, path: &str) -> bool {
    let out = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", "--", path])
        .current_dir(repo_path)
        .output();
    matches!(out, Ok(o) if !o.stdout.is_empty())
}

/// File paths the Coder stage reported changing, for diff scoping.
/// Unparseable coder JSON yields an empty list (empty scope, never everything).
pub fn agent_files_from_coder_json(coder_json: Option<&str>) -> Vec<String> {
    let Some(json) = coder_json else {
        return Vec::new();
    };
    serde_json::from_str::<crate::artifacts::types::CodeDiff>(json)
        .map(|d| d.files_changed.into_iter().map(|f| f.path).collect())
        .unwrap_or_default()
}

/// Apply a unified diff (produced by the sandbox `get_diff`) to the host working
/// tree. Used for the worktree backend, where the change lives only inside
/// the sandbox copy and must be replayed onto the host before we commit the
/// `niki/<id>` branch.
///
/// Strategy (single temp file, two attempts): `git apply` first, then
/// `git -c apply.whitespace=nowarn apply -p1 --3way`. The patch is normalized
/// once up front (see [`normalize_patch`]) so `git apply` doesn't reject the
/// final context line; the temp file is removed on every path.
pub fn apply_diff_to_working_tree(repo_path: &Path, diff: &str) -> Result<()> {
    let patch_path = repo_path.join(".niki-tmp.patch");
    std::fs::write(&patch_path, normalize_patch(diff))?;
    let patch_str = patch_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("patch path is not valid UTF-8"))?;
    let res = run_git(repo_path, &["apply", patch_str]).or_else(|_| {
        run_git(
            repo_path,
            &[
                "-c",
                "apply.whitespace=nowarn",
                "apply",
                "-p1",
                "--3way",
                patch_str,
            ],
        )
    });
    let _ = std::fs::remove_file(&patch_path);
    res
}

/// Scan the diff's files for unresolved merge-conflict markers
/// (`<<<<<<<` / `>>>>>>>` at line start). Phase 5.6: called after patch
/// application and before branch creation — markers abort the branch instead
/// of committing a conflicted tree.
pub fn ensure_no_conflict_markers(repo_path: &Path, diff: &str) -> Result<()> {
    let files = diff_files(diff);
    let mut bad = Vec::new();
    for f in &files {
        let path = repo_path.join(f);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let marked = text
            .lines()
            .any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> "));
        if marked {
            bad.push(f.clone());
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "unresolved conflict markers in {} — refusing to commit",
            bad.join(", ")
        ))
    }
}

/// Normalize a unified diff: unify CRLF→LF line endings and guarantee a trailing
/// newline. `git apply` treats a patch ending mid-line (no final newline) as a
/// "corrupt patch" at the last context line.
///
/// Phase 5.1: the single shared helper — the former per-backend copies in
/// `sandbox/docker.rs` and `sandbox/worktree.rs` delegate here.
pub(crate) fn normalize_patch(patch: &str) -> String {
    let mut s = patch.replace("\r\n", "\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

pub fn create_branch_and_commit(
    repo_path: &Path,
    branch_name: &str,
    diff: &str,
    task_id: &str,
) -> Result<()> {
    let repo = Repository::open(repo_path)?;
    let head = repo.head()?;
    let target = head
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD is not a direct reference (detached HEAD)"))?;
    let commit = repo.find_commit(target)?;

    // No-op fast path: an empty diff means nothing to commit. Return BEFORE
    // creating the branch — otherwise the user's HEAD is moved onto a stray
    // empty `niki/<id>` branch for a run that produced nothing.
    if diff_files(diff).is_empty() {
        return Ok(());
    }

    // Create a fresh branch for this task pointing at the current HEAD commit, then
    // move HEAD onto it. The new branch and the old HEAD reference the SAME commit,
    // so the working tree — which already holds the sandbox-applied patch — stays
    // intact. Do NOT `checkout_head(force)` here: a force checkout resets the working
    // tree to the branch's committed state and silently discards the applied patch,
    // producing an empty commit with none of the Coder's changes.
    let _branch = repo.branch(branch_name, &commit, false)?;
    repo.set_head(format!("refs/heads/{}", branch_name).as_str())?;

    // Stage ONLY the files the task's diff touches. `git add -A` would sweep in any
    // pre-existing uncommitted user changes, contaminating the task commit.
    let files = diff_files(diff);
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add", "--"];
    for f in &files {
        args.push(f);
    }
    run_git(repo_path, &args)?;
    let _ = run_git(repo_path, &["reset", ".niki"]);
    let _ = run_git(repo_path, &["reset", "niki.toml"]);

    let mut index = repo.index()?;
    let oid = index.write_tree()?;
    let tree = repo.find_tree(oid)?;

    // If the staged tree is identical to the parent commit's tree, there is nothing
    // to commit. (`index.is_empty()` is the wrong check — after `add -A` the index
    // always contains the tracked files, so it never reports "no change".)
    let parent_tree = commit.tree()?;
    if tree.id() == parent_tree.id() {
        return Ok(());
    }

    let sig = Signature::now("NIKI", "niki@localhost")?;
    let parent_target = repo
        .head()?
        .target()
        .ok_or_else(|| anyhow::anyhow!("HEAD is not a direct reference (detached HEAD)"))?;
    let parent = repo.find_commit(parent_target)?;
    let commit_msg = format!(
        "NIKI implementation for task {}\n\nCreated automatically by NIKI.",
        task_id
    );
    repo.commit(Some("HEAD"), &sig, &sig, &commit_msg, &tree, &[&parent])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_files_parses_unified_diff_paths() {
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n";
        assert_eq!(diff_files(diff), vec!["src/lib.rs"]);
    }

    #[test]
    fn diff_files_skips_dev_null_and_dedupes() {
        let diff =
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n--- /dev/null\n+++ b/README.md\n+++ b/README.md\n";
        assert_eq!(diff_files(diff), vec!["src/lib.rs", "README.md"]);
    }

    #[test]
    fn diff_files_empty_for_non_diff() {
        assert!(diff_files("hello world").is_empty());
    }

    #[test]
    fn normalize_patch_unifies_endings_and_trailing_newline() {
        // Phase 5.1: the single shared normalizer both backends use.
        assert_eq!(normalize_patch("a\r\nb"), "a\nb\n");
        assert_eq!(normalize_patch("a\n"), "a\n");
        assert_eq!(normalize_patch(""), "\n");
    }

    #[test]
    fn empty_diff_creates_no_branch_and_leaves_head() {
        let dir = std::env::temp_dir().join(format!("niki-empty-branch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap()
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(dir.join("f.txt"), "x\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-qm", "init"]);
        let head_before =
            String::from_utf8_lossy(&run(&["rev-parse", "--abbrev-ref", "HEAD"]).stdout)
                .trim()
                .to_string();

        create_branch_and_commit(&dir, "niki/empty", "", "task-id").unwrap();

        // No branch created, HEAD unmoved: an empty run leaves no trace.
        let out = run(&["branch", "--list", "niki/*"]);
        let branches = String::from_utf8_lossy(&out.stdout);
        assert!(!branches.contains("niki/empty"), "{branches}");
        let out = run(&["rev-parse", "--abbrev-ref", "HEAD"]);
        let head_after = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(head_before, head_after);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conflict_markers_abort_before_commit() {
        // Phase 5.6: a file with merge markers fails the check (cli blocks
        // the branch); a clean file passes.
        let dir = std::env::temp_dir().join(format!("niki-markers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("clean.rs"), "fn a() {}\n").unwrap();
        std::fs::write(
            dir.join("conflicted.rs"),
            "fn a() {}\n<<<<<<< HEAD\nfn b() {}\n=======\nfn c() {}\n>>>>>>> other\n",
        )
        .unwrap();
        let diff = "diff --git a/conflicted.rs b/conflicted.rs\n--- a/conflicted.rs\n+++ b/conflicted.rs\n@@\n";
        let err = ensure_no_conflict_markers(&dir, diff).unwrap_err();
        assert!(err.to_string().contains("conflicted.rs"), "{err:?}");
        let clean_diff = "diff --git a/clean.rs b/clean.rs\n--- a/clean.rs\n+++ b/clean.rs\n@@\n";
        assert!(ensure_no_conflict_markers(&dir, clean_diff).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
