//! Mission, Session, and Agent stores — the domain model for autonomous work.
//!
//! A `Mission` is the top-level unit of work (e.g. "fix authentication race condition").
//! A `Mission` contains one or more `Session` (approaches/branches).
//! A `Session` contains `Agent` instances that execute tool calls.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::RwLock;

use crate::activity::AgentState;
use crate::event::EventBus;

// ---------------------------------------------------------------------------
// Newtypes
// ---------------------------------------------------------------------------

/// Mission identifier (display-friendly).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MissionId(pub String);

impl fmt::Display for MissionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MissionId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SessionId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Agent identifier (unique within a mission).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId(pub String);

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for AgentId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Mission status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl MissionStatus {
    pub fn status_str(&self) -> &'static str {
        match self {
            MissionStatus::Created => "CREATED",
            MissionStatus::Running => "RUNNING",
            MissionStatus::Paused => "PAUSED",
            MissionStatus::Completed => "COMPLETED",
            MissionStatus::Failed => "FAILED",
            MissionStatus::Cancelled => "CANCELLED",
        }
    }
}

/// Attention priority for Fleet display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionPriority {
    /// Normal — no action needed.
    Normal = 0,
    /// Waiting — agent is idle, may need input.
    Waiting = 1,
    /// Needs attention — requires human decision.
    NeedsAttention = 2,
    /// Error — something went wrong.
    Error = 3,
}

/// A single mission.
#[derive(Debug, Clone)]
pub struct Mission {
    pub id: MissionId,
    pub description: String,
    pub status: MissionStatus,
    pub sessions: Vec<SessionId>,
    pub active_session: Option<SessionId>,
    pub progress: f64,
    pub cost_usd: f64,
    pub created_at: Instant,
    pub started_at: Option<Instant>,
    pub completed_at: Option<Instant>,
    pub error: Option<String>,
    pub branch: Option<String>,
    pub model: String,
    pub attention: AttentionPriority,
}

impl Mission {
    pub fn new(id: MissionId, description: String, model: String) -> Self {
        Self {
            id,
            description,
            status: MissionStatus::Created,
            sessions: Vec::new(),
            active_session: None,
            progress: 0.0,
            cost_usd: 0.0,
            created_at: Instant::now(),
            started_at: None,
            completed_at: None,
            error: None,
            branch: None,
            model,
            attention: AttentionPriority::Normal,
        }
    }

    pub fn elapsed(&self) -> std::time::Duration {
        match self.started_at {
            Some(start) => start.elapsed(),
            None => std::time::Duration::ZERO,
        }
    }
}

/// A session (approach/branch within a mission).
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub mission_id: MissionId,
    pub agents: Vec<AgentId>,
    pub messages: Vec<ChatMessage>,
    pub created_at: Instant,
}

impl Session {
    pub fn new(id: SessionId, mission_id: MissionId) -> Self {
        Self {
            id,
            mission_id,
            agents: Vec::new(),
            messages: Vec::new(),
            created_at: Instant::now(),
        }
    }
}

/// A chat message within a session.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

/// An agent within a session.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: AgentId,
    pub session_id: SessionId,
    pub role: String,
    pub state: AgentState,
    pub tool_calls: Vec<ToolCallRecord>,
    pub started_at: Instant,
}

/// Record of a tool call made by an agent.
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub input_summary: String,
    pub output_summary: Option<String>,
    pub success: bool,
    pub duration_ms: u64,
    pub started_at: Instant,
}

// ---------------------------------------------------------------------------
// Stores — thread-safe collections
// ---------------------------------------------------------------------------

/// Mission store — thread-safe collection of missions.
#[derive(Debug)]
pub struct MissionStore {
    missions: RwLock<HashMap<MissionId, Mission>>,
    event_bus: EventBus,
    next_id: AtomicU64,
}

impl MissionStore {
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            missions: RwLock::new(HashMap::new()),
            event_bus,
            next_id: AtomicU64::new(1),
        }
    }

    /// Create a new mission and emit MissionCreated.
    pub async fn create(&self, description: String, model: String) -> MissionId {
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = MissionId(format!("mission-{}", id_num));
        let mission = Mission::new(id.clone(), description.clone(), model);
        self.missions.write().await.insert(id.clone(), mission);
        let _ = self.event_bus.emit(crate::event::Event::MissionCreated {
            id: id.clone(),
            description,
            timestamp: Instant::now(),
        });
        id
    }

    /// Get a mission by ID.
    pub async fn get(&self, id: &MissionId) -> Option<Mission> {
        self.missions.read().await.get(id).cloned()
    }

    /// Get all missions.
    pub async fn list(&self) -> Vec<Mission> {
        self.missions.read().await.values().cloned().collect()
    }

    /// Update a mission's status.
    pub async fn set_status(&self, id: &MissionId, status: MissionStatus) {
        if let Some(m) = self.missions.write().await.get_mut(id) {
            m.status = status;
        }
    }
}

/// Session store.
#[derive(Debug)]
pub struct SessionStore {
    sessions: RwLock<HashMap<SessionId, Session>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn create(&self, id: SessionId, mission_id: MissionId) {
        let session = Session::new(id.clone(), mission_id);
        self.sessions.write().await.insert(id, session);
    }

    pub async fn get(&self, id: &SessionId) -> Option<Session> {
        self.sessions.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Session> {
        self.sessions.read().await.values().cloned().collect()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent store.
#[derive(Debug)]
pub struct AgentStore {
    agents: RwLock<HashMap<AgentId, Agent>>,
}

impl AgentStore {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
        }
    }

    pub async fn create(&self, id: AgentId, session_id: SessionId, role: String) {
        let agent = Agent {
            id: id.clone(),
            session_id,
            role,
            state: AgentState::Idle,
            tool_calls: Vec::new(),
            started_at: Instant::now(),
        };
        self.agents.write().await.insert(id, agent);
    }

    pub async fn get(&self, id: &AgentId) -> Option<Agent> {
        self.agents.read().await.get(id).cloned()
    }

    pub async fn set_state(&self, id: &AgentId, state: AgentState) {
        if let Some(a) = self.agents.write().await.get_mut(id) {
            a.state = state;
        }
    }

    pub async fn list(&self) -> Vec<Agent> {
        self.agents.read().await.values().cloned().collect()
    }

    pub async fn for_session(&self, session_id: &SessionId) -> Vec<Agent> {
        self.agents
            .read()
            .await
            .values()
            .filter(|a| a.session_id == *session_id)
            .cloned()
            .collect()
    }
}

impl Default for AgentStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined application stores (shared across the system).
#[derive(Debug, Clone)]
pub struct Stores {
    pub missions: Arc<MissionStore>,
    pub sessions: Arc<SessionStore>,
    pub agents: Arc<AgentStore>,
    pub event_bus: EventBus,
}

impl Stores {
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            missions: Arc::new(MissionStore::new(event_bus.clone())),
            sessions: Arc::new(SessionStore::new()),
            agents: Arc::new(AgentStore::new()),
            event_bus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mission_store_create_and_get() {
        let bus = EventBus::new();
        let store = MissionStore::new(bus);
        let id = store.create("fix auth".into(), "sonnet".into()).await;
        let mission = store.get(&id).await.unwrap();
        assert_eq!(mission.description, "fix auth");
        assert_eq!(mission.status, MissionStatus::Created);
    }

    #[tokio::test]
    async fn session_store_create() {
        let store = SessionStore::new();
        let mid = MissionId("m1".into());
        let sid = SessionId("s1".into());
        store.create(sid.clone(), mid).await;
        let session = store.get(&sid).await.unwrap();
        assert_eq!(session.mission_id, MissionId("m1".into()));
    }

    #[tokio::test]
    async fn agent_store_set_state() {
        let store = AgentStore::new();
        let aid = AgentId("a1".into());
        let sid = SessionId("s1".into());
        store.create(aid.clone(), sid, "coder".into()).await;
        store.set_state(&aid, AgentState::Thinking).await;
        let agent = store.get(&aid).await.unwrap();
        assert_eq!(agent.state, AgentState::Thinking);
    }
}
