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

/// Capture the current working-tree diff on the host. The sandbox applies the Coder's
/// patch to the bind-mounted project directory, so the host working tree already holds
/// the change — we read it from there rather than from inside the container.
///
/// `git diff` only reports changes to *tracked* files, so a brand-new (untracked) file
/// the Coder created would be invisible and `changes.patch` would come back empty. We
/// mark new files with intent-to-add (`-N`) first, which makes them show up in the diff
/// as a normal `@@ -0,0 +1,N @@` hunk without actually staging their content.
///
/// The diff is restricted to real source changes: the `.niki` working directory
/// (task artifacts) and `niki.toml` (may contain secrets) are excluded, mirroring the
/// files `create_branch_and_commit` strips from the committed branch. This keeps the
/// published `changes.patch` free of internal state and secrets.
pub fn working_tree_diff(repo_path: &Path) -> String {
    let _ = run_git(repo_path, &["add", "-A", "-N"]);
    let out = std::process::Command::new("git")
        .args([
            "diff",
            "--",
            ".",
            ":(exclude).niki",
            ":(exclude)niki.toml",
        ])
        .current_dir(repo_path)
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}

pub fn create_branch_and_commit(
    repo_path: &Path,
    branch_name: &str,
    _diff: &str,
    task_id: &str,
) -> Result<()> {
    let repo = Repository::open(repo_path)?;
    let head = repo.head()?;
    let target = head.target().unwrap();
    let commit = repo.find_commit(target)?;

    // Create a fresh branch for this task pointing at the current HEAD commit, then
    // move HEAD onto it. The new branch and the old HEAD reference the SAME commit,
    // so the working tree — which already holds the sandbox-applied patch — stays
    // intact. Do NOT `checkout_head(force)` here: a force checkout resets the working
    // tree to the branch's committed state and silently discards the applied patch,
    // producing an empty commit with none of the Coder's changes.
    let _branch = repo.branch(branch_name, &commit, false)?;
    repo.set_head(format!("refs/heads/{}", branch_name).as_str())?;

    // The sandbox already applied the patch to the host working tree. Stage everything,
    // then unstage the `.niki` working directory (task artifacts) and `niki.toml`
    // (may contain secrets) so they aren't committed to the task branch.
    run_git(repo_path, &["add", "-A"])?;
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
    let parent = repo.find_commit(repo.head()?.target().unwrap())?;
    let commit_msg = format!(
        "NIKI implementation for task {}\n\nCreated automatically by NIKI.",
        task_id
    );
    repo.commit(Some("HEAD"), &sig, &sig, &commit_msg, &tree, &[&parent])?;

    Ok(())
}
