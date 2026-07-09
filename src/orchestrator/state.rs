use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::path::Path;
use crate::artifacts::types::{AgentRole, ReviewFeedback, ArtifactEnvelope};

#[derive(Clone, Serialize, Deserialize)]
pub struct PipelineState {
    pub task_id: Uuid,
}

impl PipelineState {
    pub fn new(task_id: Uuid) -> Self {
        Self { task_id }
    }

    pub fn set_artifact<T: Serialize>(&mut self, _agent: AgentRole, _artifact: &ArtifactEnvelope<T>) -> Result<()> {
        Ok(())
    }

    pub fn set_feedback(&mut self, _feedback: ReviewFeedback) {
    }

    pub fn get_latest_feedback(&self) -> Option<ReviewFeedback> {
        None
    }
}

/// Persisted record of a task's lifecycle, written to `.niki/tasks/<id>/task.json`.
#[derive(Debug, Serialize, Deserialize)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed { error: String },
    Cancelled,
}

#[derive(Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: Uuid,
    pub description: String,
    pub status: TaskStatus,
    pub branch: Option<String>,
    pub verdict: Option<String>,
    pub revision_rounds: u32,
    pub created_at: DateTime<Utc>,
}

impl TaskRecord {
    pub fn new(task_id: Uuid, description: &str) -> Self {
        Self {
            task_id,
            description: description.to_string(),
            status: TaskStatus::Running,
            branch: None,
            verdict: None,
            revision_rounds: 0,
            created_at: Utc::now(),
        }
    }

    pub fn save_to_disk(&self, task_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(task_dir)?;
        let path = task_dir.join("task.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
