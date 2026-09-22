//! Checkpoint and resume subsystem for agent sessions.
//!
//! Captures enough runtime state to inspect, recover, or resume an interrupted run.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::runtime::context::ContextFragment;
use crate::runtime::metrics::RuntimeMetrics;
use crate::runtime::turn::AgentTurn;

/// Persistent representation of an agent session state at a specific point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCheckpoint {
    pub checkpoint_id: String,
    pub session_id: String,
    pub task_id: Uuid,
    pub task_description: String,
    pub current_role: AgentRole,
    pub current_turn: usize,
    pub current_step: usize,
    pub provider: String,
    pub model: String,
    pub produced_artifacts: Vec<(AgentRole, String)>,
    pub active_branch: Option<String>,
    pub risk_level: Option<String>,
    pub metrics: RuntimeMetrics,
    pub fragments: Vec<ContextFragment>,
    pub turns: Vec<AgentTurn>,
    pub timestamp: DateTime<Utc>,
}

/// Brief summary of a saved checkpoint for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCheckpointSummary {
    pub checkpoint_id: String,
    pub session_id: String,
    pub task_id: Uuid,
    pub task_description: String,
    pub current_role: AgentRole,
    pub timestamp: DateTime<Utc>,
    pub file_path: PathBuf,
}

impl SessionCheckpoint {
    /// Save this checkpoint to `.niki/sessions/<session_id>/checkpoint.json`.
    pub fn save(&self, project_path: &Path) -> Result<PathBuf> {
        let dir = project_path
            .join(".niki")
            .join("sessions")
            .join(&self.session_id);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create session directory: {}", dir.display()))?;

        let file_path = dir.join("checkpoint.json");
        let json = serde_json::to_string_pretty(self)?;
        crate::util::write_restricted(&file_path, json)?;

        // Also save to task dir if task dir exists
        let task_checkpoint = project_path
            .join(".niki")
            .join("tasks")
            .join(self.task_id.to_string())
            .join("checkpoint.json");
        if let Some(task_dir) = task_checkpoint.parent() {
            if task_dir.exists() {
                let _ = crate::util::write_restricted(
                    &task_checkpoint,
                    serde_json::to_string_pretty(self)?,
                );
            }
        }

        Ok(file_path)
    }

    /// Load a checkpoint from a specific file path.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read checkpoint from {}", path.display()))?;
        let cp: Self = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse checkpoint JSON from {}", path.display()))?;
        Ok(cp)
    }

    /// Locate and load a checkpoint by session ID or task ID (exact or short prefix).
    pub fn find(project_path: &Path, id_query: &str) -> Result<Self> {
        let sessions_dir = project_path.join(".niki").join("sessions");
        if sessions_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name == id_query || name.starts_with(id_query) || name.contains(id_query) {
                        let cp_file = entry.path().join("checkpoint.json");
                        if cp_file.exists() {
                            return Self::load(&cp_file);
                        }
                    }
                }
            }
        }

        let tasks_dir = project_path.join(".niki").join("tasks");
        if tasks_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name == id_query || name.starts_with(id_query) || name.contains(id_query) {
                        let cp_file = entry.path().join("checkpoint.json");
                        if cp_file.exists() {
                            return Self::load(&cp_file);
                        }
                    }
                }
            }
        }

        Err(anyhow!(
            "no checkpoint found matching '{}' under .niki/sessions/ or .niki/tasks/",
            id_query
        ))
    }

    /// List all checkpoints under the project.
    pub fn list(project_path: &Path) -> Result<Vec<SessionCheckpointSummary>> {
        let mut summaries = Vec::new();
        let sessions_dir = project_path.join(".niki").join("sessions");
        if !sessions_dir.exists() {
            return Ok(summaries);
        }

        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.flatten() {
                let cp_file = entry.path().join("checkpoint.json");
                if cp_file.exists() {
                    if let Ok(cp) = Self::load(&cp_file) {
                        summaries.push(SessionCheckpointSummary {
                            checkpoint_id: cp.checkpoint_id,
                            session_id: cp.session_id,
                            task_id: cp.task_id,
                            task_description: cp.task_description,
                            current_role: cp.current_role,
                            timestamp: cp.timestamp,
                            file_path: cp_file,
                        });
                    }
                }
            }
        }

        summaries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(summaries)
    }
}
