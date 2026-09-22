//! AgentStep — represents one model inference plus its resulting tool interaction.

use crate::artifacts::types::AgentRole;
use crate::runtime::metrics::RuntimeMetrics;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Execution status of an agent step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// A recorded tool call made by the model during a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A recorded tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultRecord {
    pub tool_id: String,
    pub tool_name: String,
    pub status: String,
    pub summary: String,
    pub duration_ms: u64,
}

/// A single discrete step in an agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub step_id: String,
    pub step_number: usize,
    pub turn_id: String,
    pub session_id: String,
    pub role: AgentRole,
    pub status: StepStatus,
    pub request_summary: Option<String>,
    pub response_text: Option<String>,
    pub tool_calls: Vec<ToolCallRecord>,
    pub tool_results: Vec<ToolResultRecord>,
    pub artifacts_produced: Vec<String>,
    pub metrics: RuntimeMetrics,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl AgentStep {
    pub fn new(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        step_number: usize,
        role: AgentRole,
    ) -> Self {
        Self {
            step_id: Uuid::new_v4().to_string(),
            step_number,
            turn_id: turn_id.into(),
            session_id: session_id.into(),
            role,
            status: StepStatus::Running,
            request_summary: None,
            response_text: None,
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
            artifacts_produced: Vec::new(),
            metrics: RuntimeMetrics::default(),
            created_at: Utc::now(),
            finished_at: None,
        }
    }

    pub fn complete(&mut self) {
        self.status = StepStatus::Completed;
        self.finished_at = Some(Utc::now());
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = StepStatus::Failed(error.into());
        self.finished_at = Some(Utc::now());
    }

    pub fn cancel(&mut self) {
        self.status = StepStatus::Cancelled;
        self.finished_at = Some(Utc::now());
    }
}
