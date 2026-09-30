use crate::artifacts::types::AgentRole;
use crate::config::types::TopologyMode;
use crate::llm::provider::TokenUsage;
use crate::memory::compression::ContextBudget;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState {
    pub task_id: Uuid,
    pub context_budget: ContextBudget,
    /// Unified run hysteresis (Phase 5.5). Resolved from config at run start;
    /// every stage boundary accrues new metrics into it.
    #[serde(default)]
    pub run_budget: super::budget::RunBudget,
}

impl PipelineState {
    pub fn new(task_id: Uuid) -> Self {
        Self {
            task_id,
            context_budget: ContextBudget::new(200_000),
            run_budget: super::budget::RunBudget::default(),
        }
    }

    /// Accrue not-yet-accounted stage metrics into the run budget and enforce
    /// all three dimensions. Call after every stage boundary; idempotent.
    pub fn accrue_budget(&mut self, metrics: &[StageMetric]) -> anyhow::Result<()> {
        self.run_budget.accrue_new_stages(metrics);
        self.run_budget.check()
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
    /// Prompt-cache hits for this stage (`0` when the provider reported no split).
    #[serde(default)]
    pub cached_input_tokens: u32,
    /// Thinking/reasoning tokens for this stage (priced at output rate).
    #[serde(default)]
    pub reasoning_tokens: u32,
    /// Wall-clock time for this stage's LLM call, in milliseconds.
    pub latency_ms: u64,
    /// Estimated USD cost; `0.0` when the model is not in the price table.
    pub cost_usd: f64,
    /// Number of transient-error retry attempts the agent made (0 = no retries).
    #[serde(default)]
    pub retry_count: u32,
    /// Time to first token (ms) — how long from the request until the first
    /// text chunk arrived. `0` when unavailable.
    #[serde(default)]
    pub ttft_ms: u32,
}

impl StageMetric {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens + self.cached_input_tokens + self.reasoning_tokens
    }

    /// Reconstitute the provider usage for this stage.
    pub fn usage(&self) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            reasoning_tokens: self.reasoning_tokens,
        }
    }
}

/// Persisted record of a task's lifecycle, written to `.niki/tasks/<id>/task.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed { error: String },
    Cancelled,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Running => write!(f, "Running"),
            TaskStatus::Completed => write!(f, "Completed"),
            TaskStatus::Failed { error } => write!(f, "Failed: {}", error),
            TaskStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: Uuid,
    pub description: String,
    pub status: TaskStatus,
    pub branch: Option<String>,
    pub verdict: Option<String>,
    pub revision_rounds: u32,
    /// Who produced `verdict`.
    ///
    /// The SingleAgent fast path assigns `Verdict::Approved` without running a
    /// Reviewer, so "Approved" alone does not tell a reader whether anything
    /// independently checked the work. This makes the difference explicit and
    /// machine-readable instead of implied.
    #[serde(default)]
    pub verdict_source: Option<String>,
    /// The full outcome, including whether anything independently reviewed the
    /// work. `verdict` alone cannot express "nobody looked".
    #[serde(default)]
    pub outcome: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    /// When this record was last written.
    ///
    /// `save_to_disk` runs at every stage boundary, so this is a heartbeat. It
    /// exists because a `Running` record with no heartbeat is indistinguishable
    /// from a live run: measured against a real four-agent pipeline killed with
    /// SIGKILL (which no handler can catch, so the SIGTERM path never wrote a
    /// terminal state), `niki status` reported **Running** indefinitely, with no
    /// way to tell the process was gone. `created_at` is not enough — a long run
    /// looks old while it is working perfectly.
    #[serde(default)]
    pub last_update: Option<DateTime<Utc>>,
    /// Per-agent cost & latency, in execution order.
    pub agent_metrics: Vec<StageMetric>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    pub total_cost_usd: f64,
    pub total_latency_ms: u64,
    /// Sum of all retry attempts across agents.
    #[serde(default)]
    pub total_retry_count: u32,
    /// Maximum TTFT across all agents (ms).
    #[serde(default)]
    pub max_ttft_ms: u32,
    /// Topology the run executed under (recorded post-run; absent in old records).
    #[serde(default)]
    pub topology: Option<TopologyMode>,
    /// Why that topology was selected (auto-rule or explicit config).
    #[serde(default)]
    pub topology_reason: Option<String>,
    /// Risk tier the spec classified into (absent in old records).
    #[serde(default)]
    pub risk_level: Option<String>,
    /// Why that tier was assigned.
    #[serde(default)]
    pub risk_rationale: Option<String>,
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
            verdict_source: None,
            outcome: None,
            last_update: None,
            created_at: Utc::now(),
            agent_metrics: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            total_latency_ms: 0,
            total_retry_count: 0,
            max_ttft_ms: 0,
            topology: None,
            topology_reason: None,
            risk_level: None,
            risk_rationale: None,
        }
    }

    /// Fold per-agent metrics into the record's running totals.
    pub fn add_metrics(&mut self, metrics: &[StageMetric]) {
        for m in metrics {
            self.total_input_tokens += m.input_tokens;
            self.total_output_tokens += m.output_tokens;
            self.total_cost_usd += m.cost_usd;
            self.total_latency_ms += m.latency_ms;
            self.total_retry_count += m.retry_count;
            if m.ttft_ms > self.max_ttft_ms {
                self.max_ttft_ms = m.ttft_ms;
            }
        }
        self.agent_metrics.extend(metrics.iter().cloned());
    }

    pub fn save_to_disk(&self, task_dir: &Path) -> Result<()> {
        // Atomic: `task.json` is polled by the TUI while the run writes it, and
        // a torn read there shows the user a half-written run record. It used
        // to be a plain `fs::write`, which truncates before it writes.
        // Stamp the heartbeat on the way out, so every write is also evidence
        // that the process is alive and this is how far it got.
        let mut stamped = self.clone();
        stamped.last_update = Some(chrono::Utc::now());
        let json = serde_json::to_string_pretty(&stamped)?;
        crate::knowledge::kb::write_atomic(&task_dir.join("task.json"), json.as_bytes())
    }

    /// Is this a `Running` record whose process is no longer writing?
    ///
    /// A run killed outright — SIGKILL, a killed container, a closed terminal
    /// — never gets to write a terminal state, because there is no code left to
    /// write it with. The record then says `Running` forever.
    ///
    /// Measured against a real four-agent pipeline: the SIGTERM handler never
    /// ran, `task.json` said `Running` indefinitely, and `niki status` reported
    /// a run that had not been alive for ten minutes as in progress.
    ///
    /// `created_at` is not enough — a long run legitimately looks old while it
    /// is working perfectly. Ten minutes is generous enough that a slow model is
    /// never misreported, and short enough that coming back after lunch is not.
    pub fn is_stale_running(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        if !matches!(self.status, TaskStatus::Running) {
            return false;
        }
        match self.last_update {
            None => true, // no heartbeat: an older NIKI, or a lost write
            Some(t) => (now - t).num_minutes() >= 10,
        }
    }
}
