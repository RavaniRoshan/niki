use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::config::DockerConfig;
use crate::sandbox::{ExecOutput, Sandbox};

/// Cloud execution backend (beta).
///
/// This is the architectural seam only: it implements the [`Sandbox`] trait so
/// the orchestrator can target NIKI's infrastructure without code changes, but
/// the real remote executor (image provisioning, agent dispatch, result
/// streaming) requires NIKI infra and credentials that are out of scope for a
/// local build. Every operation therefore fails fast with a clear message.
/// Selected via `[docker] backend = "cloud"` or `niki run --cloud`.
pub struct CloudSandbox {
    _role: AgentRole,
}

fn unavailable() -> anyhow::Error {
    crate::NikiError::NotImplemented(
        "cloud execution (beta) is not available in this build — NIKI infrastructure is required"
            .into(),
    )
    .into()
}

impl CloudSandbox {
    pub async fn create(
        _role: AgentRole,
        _source_repo: &Path,
        _task_id: &Uuid,
        _config: &DockerConfig,
    ) -> Result<Self> {
        Err(unavailable())
    }
}

#[async_trait]
impl Sandbox for CloudSandbox {
    async fn ensure_tools(&self, _tools: &[String]) -> Result<()> {
        Err(unavailable())
    }
    async fn apply_patch(&self, _patch: &str, _host_workspace: &Path) -> Result<()> {
        Err(unavailable())
    }
    async fn get_diff(&self) -> Result<String> {
        Err(unavailable())
    }
    async fn exec(&self, _cmd: &[&str]) -> Result<ExecOutput> {
        Err(unavailable())
    }
    async fn destroy(&self) -> Result<()> {
        // Nothing was provisioned, so there is nothing to clean up.
        Ok(())
    }
}
