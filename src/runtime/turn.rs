//! AgentTurn — represents one coherent objective or model interaction cycle.

use crate::artifacts::types::AgentRole;
use crate::runtime::metrics::RuntimeMetrics;
use crate::runtime::step::AgentStep;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of an agent turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// One coherent turn of agent interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub turn_id: String,
    pub turn_number: usize,
    pub session_id: String,
    pub role: AgentRole,
    pub objective: String,
    pub steps: Vec<AgentStep>,
    pub status: TurnStatus,
    pub metrics: RuntimeMetrics,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl AgentTurn {
    pub fn new(
        session_id: impl Into<String>,
        turn_number: usize,
        role: AgentRole,
        objective: impl Into<String>,
    ) -> Self {
        Self {
            turn_id: Uuid::new_v4().to_string(),
            turn_number,
            session_id: session_id.into(),
            role,
            objective: objective.into(),
            steps: Vec::new(),
            status: TurnStatus::Running,
            metrics: RuntimeMetrics::default(),
            created_at: Utc::now(),
            finished_at: None,
        }
    }

    /// Add a finished step and aggregate its metrics into the turn.
    pub fn add_step(&mut self, step: AgentStep) {
        self.metrics.merge(&step.metrics);
        self.steps.push(step);
    }

    pub fn complete(&mut self) {
        self.status = TurnStatus::Completed;
        self.finished_at = Some(Utc::now());
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = TurnStatus::Failed(error.into());
        self.finished_at = Some(Utc::now());
    }

    pub fn cancel(&mut self) {
        self.status = TurnStatus::Cancelled;
        self.finished_at = Some(Utc::now());
    }
}
