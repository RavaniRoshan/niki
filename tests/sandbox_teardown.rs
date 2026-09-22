//! Phase 5.2: interrupt cleanup and sandbox teardown.
//!
//! - Real worktree paths are recorded and cleaned (signal handlers use the
//!   same helper tested here).
//! - Drop guards remove worktrees/containers on paths that skip `destroy`.
//! - Same-task-id collision errors instead of clobbering the other run.
//! - Pruning never deletes an active worktree (recursive newest-mtime).

use niki::artifacts::types::AgentRole;
use niki::config::{DockerConfig, SecurityPolicyConfig};
use niki::sandbox::Sandbox;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

fn git(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
}

fn fixture_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t"]);
    git(repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("a.txt"), "hello\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", "init"]);
    dir
}

async fn worktree_sandbox(
    repo: &std::path::Path,
    task_id: &Uuid,
) -> niki::sandbox::worktree::WorktreeSandbox {
    let (tx, _) = mpsc::channel();
    niki::sandbox::worktree::WorktreeSandbox::create(
        AgentRole::Coder,
        repo,
        task_id,
        &DockerConfig::default(),
        &niki::config::NikiConfig::default(),
        SecurityPolicyConfig::default(),
        tx,
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn dropped_sandbox_leaves_no_worktree() {
    // An interrupted run (panic/error path skipping `destroy`) still loses
    // its worktree via the Drop guard.
    let dir = fixture_repo();
    let repo = dir.path();
    let wt_path = {
        let sb = worktree_sandbox(repo, &Uuid::new_v4()).await;
        assert!(sb.worktree_path.is_dir());
        sb.worktree_path.clone()
        // `sb` drops here without `destroy()`.
    };
    assert!(!wt_path.exists(), "Drop guard must remove the worktree dir");
    // And the worktree registration is gone too.
    let out = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .unwrap();
    let list = String::from_utf8_lossy(&out.stdout);
    assert!(!list.contains(&wt_path.display().to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn colliding_task_ids_error_instead_of_clobbering() {
    let dir = fixture_repo();
    let repo = dir.path();
    let id = Uuid::new_v4();
    let first = worktree_sandbox(repo, &id).await;
    // Marker proving the first sandbox owns its dir.
    std::fs::write(first.worktree_path.join("owned.txt"), "mine\n").unwrap();

    // Second create with the same id while the first is live → loud error.
    let (tx, _) = mpsc::channel();
    let err = match niki::sandbox::worktree::WorktreeSandbox::create(
        AgentRole::Coder,
        repo,
        &id,
        &DockerConfig::default(),
        &niki::config::NikiConfig::default(),
        SecurityPolicyConfig::default(),
        tx,
    )
    .await
    {
        Ok(_) => panic!("colliding task id must error"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("already exists"), "{err:?}");
    assert!(
        first.worktree_path.join("owned.txt").is_file(),
        "the other run's dir must not be clobbered"
    );

    // After explicit destroy the id is reusable (no debris blocking retry).
    first.destroy().await.unwrap();
    let retry = worktree_sandbox(repo, &id).await;
    assert!(retry.worktree_path.is_dir());
    retry.destroy().await.unwrap();
}

#[test]
fn cleanup_helper_removes_exact_and_suffixed_dirs() {
    // The Ctrl+C/SIGTERM path (same helper): removes <id> and <id>-N
    // parallel-coder siblings, leaves other runs alone.
    let dir = fixture_repo();
    let repo = dir.path();
    let base = repo.join(".niki-worktrees");
    std::fs::create_dir_all(base.join("task-1")).unwrap();
    std::fs::create_dir_all(base.join("task-1-1")).unwrap();
    std::fs::create_dir_all(base.join("task-2")).unwrap();

    let removed = niki::sandbox::worktree::cleanup_worktrees_for_task(repo, "task-1");
    assert_eq!(removed, 2);
    assert!(!base.join("task-1").exists());
    assert!(!base.join("task-1-1").exists());
    assert!(base.join("task-2").exists());
}

#[test]
fn prune_keeps_active_worktree_with_fresh_file() {
    // A >24h dir containing a fresh file is NOT pruned; a fully stale dir is.
    let dir = fixture_repo();
    let repo = dir.path();
    let base = repo.join(".niki-worktrees");
    let active = base.join("active-old");
    let stale = base.join("stale-old");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("old.txt"), "old\n").unwrap();
    // Backdate everything 25h, then freshen one file in the active dir.
    for d in [&active, &stale] {
        let out = std::process::Command::new("touch")
            .args(["-d", "25 hours ago"])
            .arg(d)
            .output()
            .unwrap();
        assert!(out.status.success(), "touch backdate failed");
    }
    let out = std::process::Command::new("touch")
        .args(["-d", "25 hours ago"])
        .arg(stale.join("old.txt"))
        .output()
        .unwrap();
    assert!(out.status.success());
    std::fs::write(active.join("fresh.txt"), "fresh\n").unwrap();

    let cleaned =
        niki::sandbox::worktree::cleanup_stale_worktrees(repo, Duration::from_secs(86400));
    assert_eq!(cleaned, 1, "only the fully stale dir goes");
    assert!(active.is_dir(), "active worktree must survive pruning");
    assert!(!stale.exists());
}
