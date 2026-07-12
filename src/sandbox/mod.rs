use anyhow::Result;
use async_trait::async_trait;
use bollard::Docker;
use std::path::Path;
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::config::DockerConfig;
use crate::NikiError;

pub mod docker;
pub mod worktree;
pub mod cloud;

pub use docker::{ActiveContainers, DockerSandbox, ExecOutput};

/// Which sandbox implementation backs agent execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SandboxBackend {
    /// Containerized isolation via the pre-baked `niki-sandbox` image (default).
    #[default]
    Docker,
    /// Lightweight `git worktree` + local process isolation — no Docker required.
    Worktree,
    /// Run agents on NIKI's infrastructure (beta; requires NIKI infra).
    Cloud,
}

/// Abstraction over an isolated execution environment for one agent stage.
///
/// `DockerSandbox` (container), `WorktreeSandbox` (git worktree + local process)
/// and `CloudSandbox` (NIKI infra) all implement this. The orchestrator talks
/// only to the trait, so backends are interchangeable — this is what makes
/// alternative sandboxing (#8) and cloud execution (#9) drop-in changes.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Fail fast if any required tool binary is missing from the sandbox.
    async fn ensure_tools(&self, tools: &[String]) -> Result<()>;
    /// Apply a unified diff to the sandbox's working copy.
    async fn apply_patch(&self, patch: &str, host_workspace: &Path) -> Result<()>;
    /// Return the working-tree diff produced inside the sandbox.
    async fn get_diff(&self) -> Result<String>;
    /// Run a command inside the sandbox, returning its exit code + output.
    async fn exec(&self, cmd: &[&str]) -> Result<ExecOutput>;
    /// Tear the sandbox down (remove containers / worktrees).
    async fn destroy(&self) -> Result<()>;
}

/// Create the sandbox for `backend`. `docker` is only required for the Docker
/// backend (pass `None` for worktree/cloud).
pub async fn create_sandbox(
    backend: SandboxBackend,
    docker: Option<&Docker>,
    agent_role: AgentRole,
    source_repo: &Path,
    task_id: &Uuid,
    config: &DockerConfig,
    containers: ActiveContainers,
) -> Result<Box<dyn Sandbox>> {
    match backend {
        SandboxBackend::Docker => {
            let d = docker.ok_or_else(|| {
                NikiError::Config("Docker backend selected but Docker is not available".into())
            })?;
            Ok(Box::new(
                DockerSandbox::create(d, agent_role, source_repo, task_id, config, containers)
                    .await?,
            ))
        }
        SandboxBackend::Worktree => Ok(Box::new(
            worktree::WorktreeSandbox::create(agent_role, source_repo, task_id, config).await?,
        )),
        SandboxBackend::Cloud => Ok(Box::new(
            cloud::CloudSandbox::create(agent_role, source_repo, task_id, config).await?,
        )),
    }
}
