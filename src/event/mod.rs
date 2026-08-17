//! Event stream — single source of truth for all NIKI UI surfaces.
//!
//! Every mutation in the system flows through the `EventBus` as a typed `Event`.
//! Chat, Fleet, Session, and any future UI surface consume the same events.

use std::fmt;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

use crate::mission::{AgentId, MissionId, SessionId};
use crate::runtime::ToolId;

// ---------------------------------------------------------------------------
// Event enum — canonical domain events
// ---------------------------------------------------------------------------

/// All domain events that flow through the system.
#[derive(Debug, Clone)]
pub enum Event {
    // -- Mission lifecycle --
    MissionCreated {
        id: MissionId,
        description: String,
        timestamp: Instant,
    },
    MissionStarted {
        id: MissionId,
        timestamp: Instant,
    },
    MissionPaused {
        id: MissionId,
        timestamp: Instant,
    },
    MissionResumed {
        id: MissionId,
        timestamp: Instant,
    },
    MissionCompleted {
        id: MissionId,
        summary: String,
        timestamp: Instant,
    },
    MissionFailed {
        id: MissionId,
        error: String,
        timestamp: Instant,
    },

    // -- Agent lifecycle --
    AgentStarted {
        mission_id: MissionId,
        agent_id: AgentId,
        role: String,
        timestamp: Instant,
    },
    AgentStateChanged {
        mission_id: MissionId,
        agent_id: AgentId,
        state: String,
        timestamp: Instant,
    },
    AgentThinking {
        mission_id: MissionId,
        agent_id: AgentId,
        timestamp: Instant,
    },
    AgentWaiting {
        mission_id: MissionId,
        agent_id: AgentId,
        reason: String,
        timestamp: Instant,
    },
    AgentCompleted {
        mission_id: MissionId,
        agent_id: AgentId,
        summary: String,
        timestamp: Instant,
    },
    AgentFailed {
        mission_id: MissionId,
        agent_id: AgentId,
        error: String,
        timestamp: Instant,
    },

    // -- Tool calls --
    ToolStarted {
        mission_id: MissionId,
        agent_id: AgentId,
        tool_id: ToolId,
        tool_name: String,
        input_summary: String,
        timestamp: Instant,
    },
    ToolProgress {
        mission_id: MissionId,
        agent_id: AgentId,
        tool_id: ToolId,
        message: String,
        timestamp: Instant,
    },
    ToolCompleted {
        mission_id: MissionId,
        agent_id: AgentId,
        tool_id: ToolId,
        summary: String,
        duration_ms: u64,
        timestamp: Instant,
    },
    ToolFailed {
        mission_id: MissionId,
        agent_id: AgentId,
        tool_id: ToolId,
        error: String,
        timestamp: Instant,
    },

    // -- Human interaction --
    ApprovalRequired {
        mission_id: MissionId,
        agent_id: AgentId,
        tool_name: String,
        command: String,
        description: String,
        timestamp: Instant,
    },
    ApprovalGranted {
        mission_id: MissionId,
        agent_id: AgentId,
        timestamp: Instant,
    },
    ApprovalDenied {
        mission_id: MissionId,
        agent_id: AgentId,
        reason: String,
        timestamp: Instant,
    },

    // -- Artifacts & evidence --
    ArtifactCreated {
        mission_id: MissionId,
        artifact_type: String,
        path: String,
        timestamp: Instant,
    },
    DiffUpdated {
        mission_id: MissionId,
        files_changed: usize,
        insertions: usize,
        deletions: usize,
        timestamp: Instant,
    },
    TestsStarted {
        mission_id: MissionId,
        target: String,
        timestamp: Instant,
    },
    TestsCompleted {
        mission_id: MissionId,
        passed: usize,
        failed: usize,
        skipped: usize,
        timestamp: Instant,
    },

    // -- Chat messages --
    UserMessage {
        content: String,
        timestamp: Instant,
    },
    AssistantMessage {
        content: String,
        role: String,
        timestamp: Instant,
    },

    // -- System --
    QueuedPrompt {
        content: String,
        position: usize,
        timestamp: Instant,
    },
    CancelRequested {
        reason: String,
        timestamp: Instant,
    },
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::MissionCreated { id, .. } => write!(f, "Mission {} created", id),
            Event::MissionStarted { id, .. } => write!(f, "Mission {} started", id),
            Event::MissionCompleted { id, summary, .. } => {
                write!(f, "Mission {} completed: {}", id, summary)
            }
            Event::MissionFailed { id, error, .. } => write!(f, "Mission {} failed: {}", id, error),
            Event::AgentStarted { agent_id, role, .. } => {
                write!(f, "Agent {} ({}) started", agent_id, role)
            }
            Event::AgentStateChanged {
                agent_id, state, ..
            } => write!(f, "Agent {} → {}", agent_id, state),
            Event::ToolStarted {
                tool_name, input_summary, ..
            } => write!(f, "{}({})", tool_name, input_summary),
            Event::ToolCompleted {
                tool_id, summary, ..
            } => write!(f, "tool {}: {}", tool_id, summary),
            Event::ToolFailed {
                tool_id, error, ..
            } => write!(f, "tool {} failed: {}", tool_id, error),
            _ => write!(f, "{:?}", self),
        }
    }
}

// ---------------------------------------------------------------------------
// EventBus — broadcast channel wrapper
// ---------------------------------------------------------------------------

/// Capacity of the broadcast channel.
const BUS_CAPACITY: usize = 1024;

/// Multi-producer, multi-consumer event bus backed by `tokio::sync::broadcast`.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    /// Create a new event bus.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { tx }
    }

    /// Publish an event. Returns `Err` only if there are zero receivers.
    pub fn emit(&self, event: Event) -> Result<(), broadcast::error::SendError<Event>> {
        self.tx.send(event).map(|_| ())
    }

    /// Subscribe to the event bus. Each subscriber gets its own receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Get a sender clone (for producers that don't need to receive).
    pub fn sender(&self) -> broadcast::Sender<Event> {
        self.tx.clone()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared event bus reference (clone-cheap).
pub type SharedEventBus = Arc<EventBus>;

/// Create a shared event bus.
pub fn shared_bus() -> SharedEventBus {
    Arc::new(EventBus::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_bus_emit_and_receive() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.emit(Event::MissionCreated {
            id: "test-1".parse().unwrap(),
            description: "test".into(),
            timestamp: Instant::now(),
        })
        .unwrap();

        let received = rx.try_recv().unwrap();
        match received {
            Event::MissionCreated { id, .. } => assert_eq!(id.0, "test-1"),
            _ => panic!("expected MissionCreated"),
        }
    }

    #[test]
    fn event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(Event::UserMessage {
            content: "hello".into(),
            timestamp: Instant::now(),
        })
        .unwrap();

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn event_display() {
        let event = Event::ToolCompleted {
            mission_id: "m1".parse().unwrap(),
            agent_id: "a1".parse().unwrap(),
            tool_id: "t1".parse().unwrap(),
            summary: "100 lines".into(),
            duration_ms: 50,
            timestamp: Instant::now(),
        };
        assert!(format!("{}", event).contains("t1"));
    }
}
