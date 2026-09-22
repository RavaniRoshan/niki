//! Agent Runtime — the central execution engine for NIKI.
//!
//! Exposes a structured execution loop built on:
//! - [`AgentSession`]: execution context owning pooled clients, context, and state
//! - [`AgentTurn`]: coherent objective / turn cycle
//! - [`AgentStep`]: discrete inference + tool execution step
//! - [`ContextStore`]: bounded, incremental, priority-sorted context fragments
//! - [`ToolRouter`]: validates and routes tool calls through [`ToolPolicy`] to [`ToolExecutor`]
//! - [`AgentEvent`]: typed, persistent event stream
//! - [`SessionCheckpoint`]: persistent snapshot for resumability

pub mod cancellation;
pub mod checkpoint;
pub mod compaction;
pub mod context;
pub mod events;
pub mod metrics;
pub mod policy;
pub mod session;
pub mod step;
pub mod tools;
pub mod turn;

pub use cancellation::*;
pub use checkpoint::*;
pub use compaction::*;
pub use context::*;
pub use events::*;
pub use metrics::*;
pub use policy::*;
pub use session::*;
pub use step::*;
pub use tools::*;
pub use turn::*;

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;

// ---------------------------------------------------------------------------
// ToolExecutor & ToolRouter
// ---------------------------------------------------------------------------

/// Trait responsible for executing an individual tool call after routing and policy validation.
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, tool: &dyn Tool, input: ToolInput, ctx: &ToolContext) -> ToolResult;
}

/// Standard tool executor delegating to `Tool::execute`.
#[derive(Debug, Default)]
pub struct DefaultToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for DefaultToolExecutor {
    async fn execute(&self, tool: &dyn Tool, input: ToolInput, ctx: &ToolContext) -> ToolResult {
        tool.execute(input, ctx).await
    }
}

/// Router that resolves tools from the registry, validates permissions/policies,
/// and delegates execution to the ToolExecutor.
pub struct ToolRouter {
    registry: Arc<ToolRegistry>,
    executor: Arc<dyn ToolExecutor>,
}

impl ToolRouter {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            executor: Arc::new(DefaultToolExecutor),
        }
    }

    pub fn with_executor(registry: Arc<ToolRegistry>, executor: Arc<dyn ToolExecutor>) -> Self {
        Self { registry, executor }
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    pub fn executor(&self) -> &dyn ToolExecutor {
        self.executor.as_ref()
    }

    /// Route a tool invocation, check policy, and execute.
    pub async fn route_and_execute(
        &self,
        tool_name: &str,
        input: ToolInput,
        ctx: &ToolContext,
        policy: Option<&ToolPolicy>,
    ) -> ToolResult {
        let start = Instant::now();

        // 1. Locate tool in registry
        let tool = match self.registry.get(tool_name) {
            Some(t) => t,
            None => {
                return ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: tool_name.to_string(),
                    status: ToolStatus::Failed,
                    summary: format!("tool not found: {}", tool_name),
                    data: ToolData::None,
                    duration: start.elapsed(),
                    artifacts: Vec::new(),
                    diagnostics: vec![format!("unknown tool: {}", tool_name)],
                    metadata: HashMap::new(),
                };
            }
        };

        let def = tool.def();

        // 2. Validate against ToolPolicy if provided
        if let Some(pol) = policy {
            if let Err(violation) =
                pol.check_permission(tool_name, def.category, def.risk_level, def.permission)
            {
                return ToolResult {
                    tool_id: ToolId::generate(),
                    tool_name: tool_name.to_string(),
                    status: ToolStatus::PermissionDenied,
                    summary: violation.to_string(),
                    data: ToolData::None,
                    duration: start.elapsed(),
                    artifacts: Vec::new(),
                    diagnostics: vec![violation.to_string()],
                    metadata: HashMap::new(),
                };
            }
        }

        // 3. Execute via executor after registry permissions check
        let mut result = self.executor.execute(tool, input, ctx).await;
        result.tool_name = tool_name.to_string();
        result.duration = start.elapsed();
        result
    }
}

// ---------------------------------------------------------------------------
// AgentRuntime — Central Runtime Engine
// ---------------------------------------------------------------------------

/// Central agent runtime coordinating sessions, turns, tools, and events.
pub struct AgentRuntime {
    config: NikiConfig,
    tool_router: Arc<ToolRouter>,
}

impl AgentRuntime {
    /// Create a new runtime configured with the given NikiConfig.
    pub fn new(config: NikiConfig) -> Self {
        let registry = Arc::new(build_baseline_registry());
        let tool_router = Arc::new(ToolRouter::new(registry));
        Self {
            config,
            tool_router,
        }
    }

    pub fn tool_router(&self) -> &ToolRouter {
        &self.tool_router
    }

    /// Start a new long-lived agent session for a task.
    pub async fn start_session(
        &self,
        project_path: PathBuf,
        task_id: Uuid,
        description: String,
        event_sink: Option<Arc<dyn EventSink>>,
        cancellation: Option<CancellationToken>,
    ) -> Result<AgentSession> {
        let session = AgentSession::new(
            project_path,
            self.config.clone(),
            task_id,
            description.clone(),
            event_sink,
            cancellation,
        )?;

        session
            .emit_event(
                "root",
                "init",
                AgentEventKind::SessionStarted,
                serde_json::json!({
                    "task_id": task_id.to_string(),
                    "description": description,
                }),
            )
            .await?;

        Ok(session)
    }

    /// Start a new turn for an agent role.
    pub async fn start_turn(
        &self,
        session: &mut AgentSession,
        role: AgentRole,
        objective: impl Into<String>,
    ) -> Result<AgentTurn> {
        let obj = objective.into();
        let turn_number = session.turns.len() + 1;
        let turn = AgentTurn::new(&session.session_id, turn_number, role, obj.clone());

        session
            .emit_event(
                &turn.turn_id,
                "start",
                AgentEventKind::TurnStarted,
                serde_json::json!({
                    "role": format!("{:?}", role),
                    "objective": obj,
                    "turn_number": turn_number,
                }),
            )
            .await?;

        Ok(turn)
    }

    /// Execute a tool within a session and turn, enforcing policies and logging events.
    pub async fn execute_tool(
        &self,
        session: &AgentSession,
        turn_id: &str,
        step_id: &str,
        tool_name: &str,
        input: ToolInput,
        ctx: &ToolContext,
        policy: Option<&ToolPolicy>,
    ) -> Result<ToolResult> {
        session
            .emit_event(
                turn_id,
                step_id,
                AgentEventKind::ToolExecutionStarted,
                serde_json::json!({
                    "tool": tool_name,
                }),
            )
            .await?;

        let res = self
            .tool_router
            .route_and_execute(tool_name, input, ctx, policy)
            .await;

        let kind = match res.status {
            ToolStatus::Success => AgentEventKind::ToolExecutionCompleted,
            _ => AgentEventKind::ToolExecutionFailed,
        };

        session
            .emit_event(
                turn_id,
                step_id,
                kind,
                serde_json::json!({
                    "tool": tool_name,
                    "status": format!("{:?}", res.status),
                    "summary": res.summary,
                    "duration_ms": res.duration.as_millis() as u64,
                }),
            )
            .await?;

        Ok(res)
    }

    /// Save a persistent checkpoint of the current session state.
    pub async fn checkpoint(
        &self,
        session: &AgentSession,
        role: AgentRole,
        active_branch: Option<String>,
        risk_level: Option<String>,
        produced_artifacts: Vec<(AgentRole, String)>,
    ) -> Result<PathBuf> {
        let current_turn = session.turns.len();
        let current_step = session.turns.last().map(|t| t.steps.len()).unwrap_or(0);

        let fragments = {
            let store = session.context_store.read().await;
            store.fragments().to_vec()
        };

        let checkpoint = SessionCheckpoint {
            checkpoint_id: format!("chk-{}", &Uuid::new_v4().to_string()[..8]),
            session_id: session.session_id.clone(),
            task_id: session.task_id,
            task_description: session.task_description.clone(),
            current_role: role,
            current_turn,
            current_step,
            provider: session.config.agents.planner.provider.clone(),
            model: session.config.agents.planner.model.clone(),
            produced_artifacts,
            active_branch,
            risk_level,
            metrics: session.metrics.clone(),
            fragments,
            turns: session.turns.clone(),
            timestamp: chrono::Utc::now(),
        };

        let path = checkpoint.save(&session.project_path)?;

        session
            .emit_event(
                "checkpoint",
                &checkpoint.checkpoint_id,
                AgentEventKind::CheckpointCreated,
                serde_json::json!({
                    "checkpoint_id": checkpoint.checkpoint_id,
                    "path": path.display().to_string(),
                }),
            )
            .await?;

        Ok(path)
    }

    /// Resume an interrupted agent session from a checkpoint.
    pub async fn resume(
        &self,
        project_path: &Path,
        session_or_task_id: &str,
    ) -> Result<(AgentSession, SessionCheckpoint)> {
        let cp = SessionCheckpoint::find(project_path, session_or_task_id)?;

        let mut session = AgentSession::new(
            project_path.to_path_buf(),
            self.config.clone(),
            cp.task_id,
            cp.task_description.clone(),
            None,
            None,
        )?;

        session.session_id = cp.session_id.clone();
        session.turns = cp.turns.clone();
        session.metrics = cp.metrics.clone();

        // Restore context store fragments
        {
            let mut store = session.context_store.write().await;
            for frag in &cp.fragments {
                store.upsert(frag.clone());
            }
        }

        session
            .emit_event(
                "resume",
                &cp.checkpoint_id,
                AgentEventKind::SessionResumed,
                serde_json::json!({
                    "checkpoint_id": cp.checkpoint_id,
                    "role": format!("{:?}", cp.current_role),
                    "turn": cp.current_turn,
                }),
            )
            .await?;

        Ok((session, cp))
    }
}
