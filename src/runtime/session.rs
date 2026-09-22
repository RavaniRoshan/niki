//! AgentSession — the long-lived execution context owning reusable resources.
//!
//! Reusable components:
//! - ModelSession: connection-pooled HTTP client and cached LlmProvider instances.
//! - RepositorySnapshot: cached repo manifest and git status.
//! - ContextStore: incremental fragments with stable prefixes.
//! - EventSink: persistent event streaming.
//! - CancellationToken: unified cancellation tree.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::NikiConfig;
use crate::llm::provider::{LlmProvider, create_provider, http_client};
use crate::repo_intel::RepoManifest;
use crate::runtime::cancellation::CancellationToken;
use crate::runtime::context::ContextStore;
use crate::runtime::events::{AgentEvent, AgentEventKind, EventSink, NoopEventSink};
use crate::runtime::metrics::RuntimeMetrics;
use crate::runtime::turn::AgentTurn;

/// Reusable model session holding connection-pooled clients and cached providers.
pub struct ModelSession {
    http_client: reqwest::Client,
    providers: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
}

impl ModelSession {
    pub fn new() -> Result<Self> {
        let client = http_client()?;
        Ok(Self {
            http_client: client,
            providers: RwLock::new(HashMap::new()),
        })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Get or create an LLM provider instance, reusing connections and configuration.
    pub async fn get_or_create_provider(
        &self,
        provider_name: &str,
        config: &NikiConfig,
    ) -> Result<Arc<dyn LlmProvider>> {
        {
            let map = self.providers.read().await;
            if let Some(p) = map.get(provider_name) {
                return Ok(p.clone());
            }
        }

        let mut map = self.providers.write().await;
        if let Some(p) = map.get(provider_name) {
            return Ok(p.clone());
        }

        let p_cfg = config.providers.get(provider_name).ok_or_else(|| {
            crate::NikiError::Config(format!("Provider '{}' not configured", provider_name))
        })?;

        let provider: Arc<dyn LlmProvider> = Arc::from(create_provider(provider_name, p_cfg)?);
        map.insert(provider_name.to_string(), provider.clone());
        Ok(provider)
    }
}

/// A cached snapshot of repository state to avoid repeated rescanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub git_head: Option<String>,
    pub manifest: RepoManifest,
    pub timestamp: DateTime<Utc>,
}

impl RepositorySnapshot {
    pub fn capture(project_path: &Path, config: &NikiConfig) -> Self {
        let git_head = if let Ok(repo) = git2::Repository::open(project_path) {
            repo.head()
                .ok()
                .and_then(|h| h.target().map(|oid| oid.to_string()))
        } else {
            None
        };

        let manifest = crate::repo_intel::build_manifest(project_path, config);

        Self {
            git_head,
            manifest,
            timestamp: Utc::now(),
        }
    }
}

/// Result of prewarming expensive runtime resources concurrently.
#[derive(Debug, Clone)]
pub struct PrewarmResult {
    pub prewarm_ms: u64,
    pub repository_ready: bool,
    pub model_session_ready: bool,
    pub tools_ready: bool,
    pub knowledge_ready: bool,
}

/// AgentSession manages the entire lifecycle of an agent execution context.
pub struct AgentSession {
    pub session_id: String,
    pub task_id: Uuid,
    pub task_description: String,
    pub project_path: PathBuf,
    pub config: NikiConfig,
    pub model_session: Arc<ModelSession>,
    pub context_store: Arc<RwLock<ContextStore>>,
    pub event_sink: Arc<dyn EventSink>,
    pub cancellation: CancellationToken,
    pub repo_snapshot: Arc<RwLock<Option<RepositorySnapshot>>>,
    pub turns: Vec<AgentTurn>,
    pub metrics: RuntimeMetrics,
    pub created_at: DateTime<Utc>,
}

impl AgentSession {
    pub fn new(
        project_path: PathBuf,
        config: NikiConfig,
        task_id: Uuid,
        task_description: String,
        event_sink: Option<Arc<dyn EventSink>>,
        cancellation: Option<CancellationToken>,
    ) -> Result<Self> {
        let session_id = format!("sess-{}", &task_id.to_string()[..8]);
        let model_session = Arc::new(ModelSession::new()?);
        let max_tokens = config.general.max_context_chars / 4;
        let context_store = Arc::new(RwLock::new(ContextStore::new(max_tokens)));
        let event_sink = event_sink.unwrap_or_else(|| Arc::new(NoopEventSink));
        let cancellation = cancellation.unwrap_or_default();

        Ok(Self {
            session_id,
            task_id,
            task_description,
            project_path,
            config,
            model_session,
            context_store,
            event_sink,
            cancellation,
            repo_snapshot: Arc::new(RwLock::new(None)),
            turns: Vec::new(),
            metrics: RuntimeMetrics::default(),
            created_at: Utc::now(),
        })
    }

    /// Prewarm expensive resources concurrently before starting inference turns.
    pub async fn prewarm(&self) -> PrewarmResult {
        let start = Instant::now();
        let project_path = self.project_path.clone();
        let config = self.config.clone();

        // 1. Task: Repository snapshot (manifest + git)
        let repo_task = tokio::spawn(async move {
            tokio::task::spawn_blocking(move || RepositorySnapshot::capture(&project_path, &config))
                .await
                .ok()
        });

        // 2. Task: Tool registry construction
        let tools_task = tokio::spawn(async {
            tokio::task::spawn_blocking(crate::runtime::tools::build_baseline_registry)
                .await
                .is_ok()
        });

        // 3. Task: Prompt templates verification
        let prompts_task = tokio::spawn(async {
            tokio::task::spawn_blocking(|| {
                crate::load_asset("prompts/base.md").is_ok()
                    && crate::load_asset("prompts/planner.md").is_ok()
            })
            .await
            .unwrap_or(false)
        });

        let (repo_res, tools_res, prompts_res) = tokio::join!(repo_task, tools_task, prompts_task);

        let mut repo_ready = false;
        if let Ok(Some(snap)) = repo_res {
            let mut guard = self.repo_snapshot.write().await;
            *guard = Some(snap);
            repo_ready = true;
        }

        let tools_ready = tools_res.unwrap_or(false);
        let knowledge_ready = prompts_res.unwrap_or(false);
        let prewarm_ms = start.elapsed().as_millis() as u64;

        PrewarmResult {
            prewarm_ms,
            repository_ready: repo_ready,
            model_session_ready: true,
            tools_ready,
            knowledge_ready,
        }
    }

    /// Emit an event into the session's event sink.
    pub async fn emit_event(
        &self,
        turn_id: &str,
        step_id: &str,
        kind: AgentEventKind,
        payload: serde_json::Value,
    ) -> Result<()> {
        let event = AgentEvent::new(&self.session_id, turn_id, step_id, kind, payload);
        self.event_sink.emit(&event).await
    }

    /// Get or refresh the repository snapshot.
    pub async fn repository_snapshot(&self) -> RepositorySnapshot {
        {
            let guard = self.repo_snapshot.read().await;
            if let Some(ref snap) = *guard {
                return snap.clone();
            }
        }
        let snap = RepositorySnapshot::capture(&self.project_path, &self.config);
        let mut guard = self.repo_snapshot.write().await;
        *guard = Some(snap.clone());
        snap
    }

    /// Invalidate the repository snapshot after file modifications.
    pub async fn invalidate_repo_snapshot(&self) {
        let mut guard = self.repo_snapshot.write().await;
        *guard = None;
    }
}
