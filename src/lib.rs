pub mod cli;
pub mod config;
pub mod artifacts;
pub mod agents;
pub mod orchestrator;
pub mod sandbox;
pub mod llm;
pub mod knowledge;
pub mod output;
pub mod display;
pub mod cost;
pub mod recommend;

use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum NikiError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Docker error: {0}")]
    Docker(#[from] bollard::errors::Error),
    
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    
    #[error("LLM provider error ({provider}): {message}")]
    LlmProvider { provider: String, message: String },
    
    #[error("Artifact validation failed for {agent:?}: {errors}")]
    ArtifactValidation { agent: artifacts::types::AgentRole, errors: String },
    
    #[error("Agent {agent:?} failed after {retries} retries: {message}")]
    AgentFailure { agent: artifacts::types::AgentRole, retries: u32, message: String },
    
    #[error("No API key configured for provider '{0}'. Set it in niki.toml or via environment variable.")]
    MissingApiKey(String),
    
    #[error("Docker is not running. Please start Docker and try again.")]
    DockerNotRunning,
    
    #[error("Task {0} not found")]
    TaskNotFound(Uuid),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Anyhow error: {0}")]
    Other(#[from] anyhow::Error),
}

/// Resolve a crate-relative asset path (e.g. "prompts/planner.md" or
/// "schemas/task_spec.schema.json") to an absolute path rooted at the crate
/// manifest directory. Falls back to the given path (relative to CWD) when the
/// manifest asset is missing, so the binary still works when invoked from a
/// different working directory.
pub fn resolve_asset(rel: &str) -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    if manifest.exists() {
        return manifest;
    }
    std::path::PathBuf::from(rel)
}
