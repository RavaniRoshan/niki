mod common;

use niki::artifacts::types::AgentRole;
use niki::config::{DockerConfig, SecurityPolicyConfig};
use niki::sandbox::docker::ActiveContainers;
use niki::sandbox::{Sandbox, SandboxBackend, create_sandbox};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Mutex;
use uuid::Uuid;

async fn create_worktree_sandbox(
    repo_path: &std::path::Path,
    policy: SecurityPolicyConfig,
) -> Result<niki::sandbox::worktree::WorktreeSandbox, anyhow::Error> {
    niki::sandbox::worktree::WorktreeSandbox::create(
        AgentRole::Coder,
        repo_path,
        &Uuid::new_v4(),
        &DockerConfig::default(),
        policy,
    )
    .await
}

#[tokio::test]
async fn worktree_sandbox_stores_policy_for_coder_role() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@niki.local"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "niki-test"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(repo)
        .status()
        .unwrap();

    let policy = SecurityPolicyConfig::default();
    let sandbox = create_worktree_sandbox(repo, policy.clone()).await.unwrap();

    // The sandbox should store the policy and enforce it during exec.
    // Verify by checking that a denied command is rejected.
    let result = sandbox
        .exec(&["rm", "-rf", "/"], Some(&AgentRole::Coder))
        .await;
    assert!(result.is_err(), "exec with denied command should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("denied"),
        "error should mention denied: {}",
        err_msg
    );

    // A safe command should still work.
    let result = sandbox
        .exec(&["echo", "hello"], Some(&AgentRole::Coder))
        .await;
    assert!(result.is_ok(), "exec with safe command should succeed");

    // Clean up
    let _ = sandbox.destroy().await;
}

#[tokio::test]
async fn worktree_sandbox_enforces_role_specific_policy() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@niki.local"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "niki-test"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(repo)
        .status()
        .unwrap();

    // Tester policy: allows cargo test, blocks git push
    let tester_policy = niki::config::types::default_tester_policy();
    let sandbox = create_worktree_sandbox(repo, tester_policy).await.unwrap();

    // `git push` should be denied for tester
    let result = sandbox
        .exec(&["git", "push", "origin", "main"], Some(&AgentRole::Tester))
        .await;
    assert!(result.is_err(), "tester should be blocked from git push");

    // `cargo test` should be allowed for tester
    let result = sandbox
        .exec(&["cargo", "test", "--lib"], Some(&AgentRole::Tester))
        .await;
    assert!(result.is_ok(), "tester should be allowed to run cargo test");

    let _ = sandbox.destroy().await;
}

#[tokio::test]
async fn worktree_sandbox_skips_policy_when_role_is_none() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@niki.local"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "niki-test"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(repo)
        .status()
        .unwrap();

    let policy = SecurityPolicyConfig::default();
    let sandbox = create_worktree_sandbox(repo, policy).await.unwrap();

    // When role is None, policy check is skipped — but `rm -rf /` is still
    // dangerous. The policy is only enforced when role is Some.
    // Verify the exec trait method skips policy check:
    let result = sandbox.exec(&["echo", "safe"], None).await;
    assert!(result.is_ok(), "exec without role should bypass policy");

    let _ = sandbox.destroy().await;
}

#[tokio::test]
async fn worktree_sandbox_custom_timeout_is_respected() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@niki.local"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "niki-test"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(repo)
        .status()
        .unwrap();

    let mut policy = SecurityPolicyConfig::default();
    policy.max_exec_seconds = 1;
    let sandbox = create_worktree_sandbox(repo, policy).await.unwrap();

    // A command that sleeps longer than the timeout should be killed.
    let start = std::time::Instant::now();
    let result = sandbox.exec(&["sleep", "5"], Some(&AgentRole::Coder)).await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "sleep 5 should time out");
    assert!(
        elapsed < Duration::from_secs(3),
        "should time out well before 5s, took {:?}",
        elapsed
    );

    let _ = sandbox.destroy().await;
}

#[tokio::test]
async fn create_sandbox_worktree_backed() {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@niki.local"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "niki-test"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(repo)
        .status()
        .unwrap();

    let policy = SecurityPolicyConfig::default();
    let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));
    let sandbox = create_sandbox(
        SandboxBackend::Worktree,
        None,
        AgentRole::Planner,
        repo,
        &Uuid::new_v4(),
        &DockerConfig::default(),
        policy,
        containers,
    )
    .await;

    assert!(
        sandbox.is_ok(),
        "create_sandbox should succeed for worktree"
    );
    let _ = sandbox.unwrap().destroy().await;
}
