//! Persistent typed event model for runtime activity.
//!
//! Decoupled from any specific UI surface: can feed TUI rendering,
//! JSON output, telemetry, persistence journals, and replay.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, broadcast};

/// Kinds of events emitted during an agent session execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventKind {
    SessionStarted,
    SessionResumed,
    TurnStarted,
    ContextBuilt,
    ModelRequestStarted,
    ModelResponseStarted,
    ModelResponseCompleted,
    ToolCallRequested,
    ToolExecutionStarted,
    ToolExecutionCompleted,
    ToolExecutionFailed,
    ArtifactProduced,
    TestStarted,
    TestCompleted,
    CompactionStarted,
    CompactionCompleted,
    CheckpointCreated,
    TurnCompleted,
    SessionCompleted,
    SessionFailed,
}

/// A structured, serializable runtime event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub session_id: String,
    pub turn_id: String,
    pub step_id: String,
    pub timestamp: DateTime<Utc>,
    pub event_kind: AgentEventKind,
    pub payload: serde_json::Value,
}

impl AgentEvent {
    pub fn new(
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
        step_id: impl Into<String>,
        event_kind: AgentEventKind,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            step_id: step_id.into(),
            timestamp: Utc::now(),
            event_kind,
            payload,
        }
    }
}

/// Destination for runtime events.
#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn emit(&self, event: &AgentEvent) -> Result<()>;
}

/// No-op event sink that drops events.
pub struct NoopEventSink;

#[async_trait::async_trait]
impl EventSink for NoopEventSink {
    async fn emit(&self, _event: &AgentEvent) -> Result<()> {
        Ok(())
    }
}

/// An event sink that appends events as JSON lines to a file on disk.
pub struct JournalEventSink {
    file_path: PathBuf,
    lock: Mutex<()>,
}

impl JournalEventSink {
    pub fn new(file_path: impl AsRef<Path>) -> Self {
        Self {
            file_path: file_path.as_ref().to_path_buf(),
            lock: Mutex::new(()),
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

#[async_trait::async_trait]
impl EventSink for JournalEventSink {
    async fn emit(&self, event: &AgentEvent) -> Result<()> {
        let _guard = self.lock.lock().await;
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let line = serde_json::to_string(event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }
}

/// In-memory broadcast sink for live subscribers (e.g., TUI or WebSocket).
pub struct BroadcastEventSink {
    sender: broadcast::Sender<AgentEvent>,
}

impl BroadcastEventSink {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.sender.subscribe()
    }
}

#[async_trait::async_trait]
impl EventSink for BroadcastEventSink {
    async fn emit(&self, event: &AgentEvent) -> Result<()> {
        // Failing to send because there are no receivers is normal and not an error.
        let _ = self.sender.send(event.clone());
        Ok(())
    }
}

/// Composite event sink broadcasting to multiple underlying sinks.
pub struct CompositeEventSink {
    sinks: Vec<Arc<dyn EventSink>>,
}

impl CompositeEventSink {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { sinks }
    }

    pub fn add_sink(&mut self, sink: Arc<dyn EventSink>) {
        self.sinks.push(sink);
    }
}

#[async_trait::async_trait]
impl EventSink for CompositeEventSink {
    async fn emit(&self, event: &AgentEvent) -> Result<()> {
        for sink in &self.sinks {
            if let Err(e) = sink.emit(event).await {
                tracing::warn!(target: "niki::runtime", "EventSink error: {}", e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_journal_sink() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let sink = JournalEventSink::new(&path);

        let evt = AgentEvent::new(
            "sess-1",
            "turn-1",
            "step-1",
            AgentEventKind::SessionStarted,
            serde_json::json!({"test": true}),
        );
        sink.emit(&evt).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("session_started"));
        assert!(content.contains("sess-1"));
    }

    #[tokio::test]
    async fn test_broadcast_sink() {
        let sink = BroadcastEventSink::new(16);
        let mut rx = sink.subscribe();

        let evt = AgentEvent::new(
            "sess-1",
            "turn-1",
            "step-1",
            AgentEventKind::TurnStarted,
            serde_json::json!({}),
        );
        sink.emit(&evt).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_kind, AgentEventKind::TurnStarted);
    }
}
