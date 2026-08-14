// Allow lint categories that are design choices, not bugs
#![allow(clippy::too_many_arguments)]
#![allow(clippy::format_in_format_args)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::from_str_radix_10)]
#![allow(clippy::unnecessary_sort_by)]
#![allow(clippy::single_match)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::from_over_into)]
#![allow(clippy::redundant_closure_call)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::manual_strip)]
#![allow(clippy::needless_return)]
#![allow(clippy::match_single_binding)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::branches_sharing_code)]
#![allow(clippy::derived_hash_with_manual_eq)]

use anyhow::anyhow;
use include_dir::{Dir, include_dir};

pub mod agents;
pub mod artifacts;
pub mod audit;
pub mod cli;
pub mod commands;
pub mod config;
pub mod cost;
pub mod display;
pub mod errors;
pub mod eval;
pub mod goal;
pub mod knowledge;
pub mod llm;
pub mod mcp;
pub mod memory;
pub mod observability;
pub mod orchestrator;
pub mod output;
pub mod permissions;
pub mod recommend;
pub mod safety;
pub mod sandbox;
pub mod session;
pub mod tools;
pub mod util;

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
    ArtifactValidation {
        agent: artifacts::types::AgentRole,
        errors: String,
    },

    #[error("Agent {agent:?} failed after {retries} retries: {message}")]
    AgentFailure {
        agent: artifacts::types::AgentRole,
        retries: u32,
        message: String,
    },

    #[error(
        "No API key configured for provider '{0}'. Set it in niki.toml or via environment variable."
    )]
    MissingApiKey(String),

    #[error(
        "No container runtime found. Start Podman (systemctl --user enable --now podman.socket) or Docker and try again."
    )]
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

/// Prompts and JSON schemas are embedded into the binary at compile time so the
/// released executable works regardless of the current working directory or
/// whether the source tree is present (e.g. when installed via Homebrew/Scoop).
/// `include_dir` resolves paths relative to this source file (`src/lib.rs`),
/// hence the `../` prefix to reach the crate root.
static EMBEDDED_PROMPTS: Dir = include_dir!("prompts");
static EMBEDDED_SCHEMAS: Dir = include_dir!("schemas");

/// Load an embedded asset (a `prompts/` or `schemas/` path) as a UTF-8 string.
///
/// Resolution order:
/// 1. The embedded copy baked into the binary (always available).
/// 2. The local filesystem (for development / editable prompts).
///
/// This guarantees `niki run` works from any directory with the shipped
/// binary, which is not true of `resolve_asset` alone.
pub fn load_asset(rel: &str) -> anyhow::Result<String> {
    let (dir, name) = match rel.split_once('/') {
        Some((d, n)) => (d, n),
        None => (rel, ""),
    };
    let embedded = match dir {
        "prompts" => EMBEDDED_PROMPTS.get_file(name),
        "schemas" => EMBEDDED_SCHEMAS.get_file(name),
        _ => None,
    };
    if let Some(file) = embedded {
        return String::from_utf8(file.contents().to_vec())
            .map_err(|e| anyhow!("Asset {} is not valid UTF-8: {}", rel, e));
    }
    let path = resolve_asset(rel);
    std::fs::read_to_string(&path)
        .map_err(|e| anyhow!("Failed to read asset {}: {}", path.display(), e))
}
