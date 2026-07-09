use anyhow::Result;
use git2::{Repository, Signature, Oid};
use std::path::Path;
use std::fs;

pub fn create_branch_and_commit(
    repo_path: &Path,
    branch_name: &str,
    diff: &str,
    task_id: &str,
) -> Result<()> {
    // If no diff, don't create branch
    if diff.trim().is_empty() {
        return Ok(());
    }

    let repo = Repository::open(repo_path)?;
    let head = repo.head()?;
    let target = head.target().unwrap();
    let commit = repo.find_commit(target)?;

    // Create and checkout branch
    let branch = repo.branch(branch_name, &commit, false)?;
    repo.set_head(format!("refs/heads/{}", branch_name).as_str())?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;

    // We can't apply diff easily using git2-rs cleanly without worktrees.
    // So we apply the diff by writing the patch file and shelling out to git apply
    let patch_path = repo_path.join(format!(".niki-tmp-{}.patch", task_id));
    fs::write(&patch_path, diff)?;

    let status = std::process::Command::new("git")
        .arg("apply")
        .arg(&patch_path)
        .current_dir(repo_path)
        .status()?;

    fs::remove_file(&patch_path)?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to apply git diff"));
    }

    // Add all files
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    let oid = index.write_tree()?;
    let tree = repo.find_tree(oid)?;

    // Commit
    let sig = Signature::now("NIKI", "niki@localhost")?;
    let commit_msg = format!("NIKI implementation for task {}\n\nCreated automatically by NIKI.", task_id);
    
    let head = repo.head()?;
    let parent = repo.find_commit(head.target().unwrap())?;

    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &commit_msg,
        &tree,
        &[&parent],
    )?;

    Ok(())
}
