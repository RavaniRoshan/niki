use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use std::path::Path;
use crate::artifacts::types::{AgentRole, ReviewFeedback, ArtifactEnvelope};
use crate::llm::provider::TokenUsage;

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

/// Per-agent cost & latency captured for one pipeline stage.
///
/// Persisted on the `TaskRecord` so `niki report` and `niki status` can show
/// real accounting long after the run finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageMetric {
    pub role: AgentRole,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Wall-clock time for this stage's LLM call, in milliseconds.
    pub latency_ms: u64,
    /// Estimated USD cost; `0.0` when the model is not in the price table.
    pub cost_usd: f64,
}

impl StageMetric {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Reconstitute the provider usage for this stage.
    pub fn usage(&self) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
        }
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
    /// Per-agent cost & latency, in execution order.
    pub agent_metrics: Vec<StageMetric>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub total_cost_usd: f64,
    pub total_latency_ms: u64,
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
            agent_metrics: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            total_latency_ms: 0,
        }
    }

    /// Fold per-agent metrics into the record's running totals.
    pub fn add_metrics(&mut self, metrics: &[StageMetric]) {
        for m in metrics {
            self.total_input_tokens += m.input_tokens;
            self.total_output_tokens += m.output_tokens;
            self.total_cost_usd += m.cost_usd;
            self.total_latency_ms += m.latency_ms;
        }
        self.agent_metrics.extend(metrics.iter().cloned());
    }

    pub fn save_to_disk(&self, task_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(task_dir)?;
        let path = task_dir.join("task.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
