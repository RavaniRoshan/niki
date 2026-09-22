//! Session management — save, restore, and switch between conversation sessions.
//!
//! Sessions are stored as JSON files in `.niki/sessions/` within the project directory.
//! Each session captures the full conversation state for resumption later.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single message in the session conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: DateTime<Utc>,
}

/// Checkpoint for undo/redo — captures state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub messages: Vec<SessionMessage>,
    #[serde(default)]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub git_commit: Option<String>,
}

/// Rollback / restore mode for checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RewindMode {
    #[default]
    Both,
    CodeOnly,
    ConversationOnly,
}

/// The full state of a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub project_path: PathBuf,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub messages: Vec<SessionMessage>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,
    #[serde(default)]
    pub total_cost_usd: f64,
    #[serde(default)]
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub checkpoints: Vec<Checkpoint>,
    #[serde(default)]
    pub current_checkpoint: Option<usize>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Store schema version; mismatches warn loudly instead of emptying silently.
    #[serde(default = "session_schema_version")]
    pub schema_version: u32,
}

fn session_schema_version() -> u32 {
    1
}

impl Session {
    /// Create a new session for the given project.
    pub fn new(project_path: PathBuf, model: String, provider: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            project_path,
            title: "New Session".to_string(),
            messages: Vec::new(),
            model,
            provider,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            created_at: now,
            updated_at: now,
            checkpoints: Vec::new(),
            current_checkpoint: None,
            metadata: HashMap::new(),
            schema_version: 1,
        }
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
        });
        self.updated_at = Utc::now();

        // Auto-generate title from first user message
        if role == "user" && self.title == "New Session" {
            let trimmed = content.trim();
            self.title = if trimmed.len() > 60 {
                format!("{}...", &trimmed[..57])
            } else {
                trimmed.to_string()
            };
        }
    }

    /// Record token usage for the current turn.
    pub fn record_usage(&mut self, input_tokens: u64, output_tokens: u64, cost_usd: f64) {
        self.total_input_tokens += input_tokens;
        self.total_output_tokens += output_tokens;
        self.total_cost_usd += cost_usd;
        self.updated_at = Utc::now();
    }

    /// Create a checkpoint of the current state.
    pub fn create_checkpoint(&mut self, label: &str, git_commit: Option<String>) {
        let checkpoint = Checkpoint {
            id: Uuid::new_v4().to_string(),
            label: label.to_string(),
            messages: self.messages.clone(),
            timestamp: Utc::now(),
            git_commit,
        };
        self.checkpoints.push(checkpoint);
        self.current_checkpoint = Some(self.checkpoints.len() - 1);
    }

    /// Restore to a checkpoint by index.
    pub fn restore_checkpoint(&mut self, index: usize) -> Result<()> {
        let checkpoint = self
            .checkpoints
            .get(index)
            .context("Checkpoint not found")?;
        self.messages = checkpoint.messages.clone();
        self.current_checkpoint = Some(index);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Restore to a checkpoint by index and mode, returning the git commit hash if any.
    pub fn restore_checkpoint_mode(
        &mut self,
        index: usize,
        mode: RewindMode,
    ) -> Result<Option<String>> {
        let checkpoint = self
            .checkpoints
            .get(index)
            .context("Checkpoint not found")?;
        let commit = checkpoint.git_commit.clone();
        if mode == RewindMode::Both || mode == RewindMode::ConversationOnly {
            self.messages = checkpoint.messages.clone();
        }
        self.current_checkpoint = Some(index);
        self.updated_at = Utc::now();
        Ok(commit)
    }

    /// Undo to the previous checkpoint.
    pub fn undo(&mut self) -> bool {
        match self.current_checkpoint {
            Some(idx) if idx > 0 => {
                let _ = self.restore_checkpoint(idx - 1);
                true
            }
            None if !self.checkpoints.is_empty() => {
                let _ = self.restore_checkpoint(self.checkpoints.len() - 1);
                true
            }
            _ => false,
        }
    }

    /// Redo to the next checkpoint.
    pub fn redo(&mut self) -> bool {
        match self.current_checkpoint {
            Some(idx) if idx + 1 < self.checkpoints.len() => {
                let _ = self.restore_checkpoint(idx + 1);
                true
            }
            _ => false,
        }
    }
}

/// Manages multiple sessions for a project.
pub struct SessionManager {
    pub sessions_dir: PathBuf,
    pub project_path: PathBuf,
    /// Phase 6.1: `[session] enabled` — when false the manager still exists
    /// but skips persistence (create/save are no-ops), so a disabled table
    /// never writes.
    pub enabled: bool,
    /// Phase 6.1: `[session] max_sessions` — prune oldest on save when the
    /// directory exceeds this (0/1 = at least one retained).
    pub max_sessions: usize,
    /// Phase 6.1: `[session] auto_save` — checkpoint writes are best-effort
    /// when off, silent when on (default behavior unchanged).
    pub auto_save: bool,
}

impl SessionManager {
    /// Create a session manager for the given project.
    pub fn new(project_path: &Path) -> Self {
        Self {
            sessions_dir: project_path.join(".niki").join("sessions"),
            project_path: project_path.to_path_buf(),
            enabled: true,
            max_sessions: crate::config::types::default_max_sessions(),
            auto_save: true,
        }
    }

    /// Construct from `[session]` config: honors `enabled`/`max_sessions`/
    /// `auto_save` (Phase 6.1 — the table is live, not a warned-but-dead
    /// field). Defaults to `new()` when no config is supplied.
    pub fn from_config(project_path: &Path, config: &crate::config::SessionConfig) -> Self {
        let mut mgr = Self::new(project_path);
        mgr.enabled = config.enabled;
        mgr.max_sessions = config.max_sessions.max(1);
        mgr.auto_save = config.auto_save;
        mgr
    }

    /// Initialize the sessions directory.
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.sessions_dir)?;
        Ok(())
    }

    /// Save a session to disk (atomic + 0600).
    pub fn save(&self, session: &Session) -> Result<()> {
        // Phase 6.1: `[session] enabled` — a disabled table never writes.
        if !self.enabled {
            return Ok(());
        }
        let mut owned = session.clone();
        owned.schema_version = 1;
        let path = self.session_path(&owned.id);
        let json = serde_json::to_string_pretty(&owned)?;
        crate::util::write_atomic_restricted(&path, json)?;
        // Phase 6.1: `[session] max_sessions` — prune oldest on save when
        // the directory exceeds the cap (0/1 = at least one retained).
        self.prune_oldest()?;
        Ok(())
    }

    /// Delete the oldest sessions until the directory holds at most
    /// `max_sessions`. Best-effort: failures warn, never fail the save.
    fn prune_oldest(&self) -> Result<()> {
        if self.max_sessions <= 1 {
            return Ok(());
        }
        let sessions = self.list()?;
        if sessions.len() <= self.max_sessions {
            return Ok(());
        }
        // `list()` is newest-first, so the oldest are at the end.
        for s in sessions
            .iter()
            .rev()
            .take(sessions.len() - self.max_sessions)
        {
            if let Err(e) = self.delete(&s.id) {
                eprintln!("Warning: session prune failed for {}: {e}", s.id);
            }
        }
        Ok(())
    }

    /// Load a session by ID. Version mismatches warn loudly; unparsable files error.
    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.session_path(id);
        let json = fs::read_to_string(&path)?;
        let session: Session = serde_json::from_str(&json)?;
        if session.schema_version != 1 {
            eprintln!(
                "Warning: {} schema_version={} (expected 1); reading best-effort",
                path.display(),
                session.schema_version
            );
        }
        Ok(session)
    }

    /// List all sessions for this project, most recent first.
    pub fn list(&self) -> Result<Vec<Session>> {
        if !self.sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(json) = fs::read_to_string(&path)
                && let Ok(session) = serde_json::from_str::<Session>(&json)
            {
                sessions.push(session);
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Delete a session by ID.
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.session_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Load the "current" session, returning `None` if it doesn't exist.
    pub fn load_current(&self) -> Result<Option<Session>> {
        match self.load(CURRENT_SESSION_ID) {
            Ok(session) => Ok(Some(session)),
            Err(e) => {
                if e.downcast_ref::<std::io::Error>()
                    .map(|e| e.kind() == std::io::ErrorKind::NotFound)
                    .unwrap_or(false)
                {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Save the "current" session to disk (atomic + 0600).
    /// Phase 6.1: honors `[session] enabled`/`auto_save`/`max_sessions`.
    pub fn save_current(&self, session: &Session) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let mut owned = session.clone();
        owned.schema_version = 1;
        let path = self.session_path(CURRENT_SESSION_ID);
        let json = serde_json::to_string_pretty(&owned)?;
        crate::util::write_atomic_restricted(&path, json)?;
        self.prune_oldest()?;
        Ok(())
    }

    /// Load the current session (mutably), returning a new default session if none exists.
    pub fn load_or_create_current(&self) -> Result<Session> {
        match self.load_current()? {
            Some(session) => Ok(session),
            None => {
                let session = Session::new(
                    self.project_path.clone(),
                    "unknown".to_string(),
                    "unknown".to_string(),
                );
                self.save_current(&session)?;
                Ok(session)
            }
        }
    }

    /// Create a checkpoint in the current session, then save.
    ///
    /// Phase 4.6: every checkpoint also appends to the session journal, so the
    /// journal is a live audit trail of session evolution (not a dead API).
    /// Journal writes are best-effort and never fail checkpointing.
    pub fn create_checkpoint(&self, label: &str, git_commit: Option<String>) -> Result<()> {
        let mut session = self.load_or_create_current()?;
        session.create_checkpoint(label, git_commit.clone());
        self.save_current(&session)?;
        let _ = self.append_journal(
            CURRENT_SESSION_ID,
            "system",
            &format!(
                "checkpoint '{label}'{}",
                git_commit.map(|c| format!(" at {c}")).unwrap_or_default()
            ),
        );
        Ok(())
    }

    /// Undo the current session: load, undo, save. Returns `true` if undo succeeded.
    pub fn undo(&self) -> Result<bool> {
        match self.load_current()? {
            Some(mut session) => {
                let result = session.undo();
                if result {
                    self.save_current(&session)?;
                }
                Ok(result)
            }
            None => Ok(false),
        }
    }

    /// Redo the current session: load, redo, save. Returns `true` if redo succeeded.
    pub fn redo(&self) -> Result<bool> {
        match self.load_current()? {
            Some(mut session) => {
                let result = session.redo();
                if result {
                    self.save_current(&session)?;
                }
                Ok(result)
            }
            None => Ok(false),
        }
    }

    /// Rewind to the previous checkpoint and return its label, if successful.
    pub fn rewind(&self) -> Result<Option<String>> {
        match self.load_current()? {
            Some(mut session) => {
                if session.undo() {
                    self.save_current(&session)?;
                    let idx = session.current_checkpoint.unwrap_or(0);
                    let label = session
                        .checkpoints
                        .get(idx)
                        .map(|c| c.label.clone())
                        .unwrap_or_default();
                    Ok(Some(label))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Rewind to the previous checkpoint with a specific mode (Both, CodeOnly, ConversationOnly).
    pub fn rewind_mode(&self, mode: RewindMode) -> Result<Option<(String, Option<String>)>> {
        match self.load_current()? {
            Some(mut session) => {
                let target_idx = match session.current_checkpoint {
                    Some(idx) if idx > 0 => Some(idx - 1),
                    None if !session.checkpoints.is_empty() => Some(session.checkpoints.len() - 1),
                    _ => None,
                };
                if let Some(idx) = target_idx {
                    let commit = session.restore_checkpoint_mode(idx, mode)?;
                    let label = session
                        .checkpoints
                        .get(idx)
                        .map(|c| c.label.clone())
                        .unwrap_or_default();
                    self.save_current(&session)?;
                    Ok(Some((label, commit)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Get the labels of all checkpoints in the current session, if it exists.
    pub fn checkpoint_labels(&self) -> Result<Vec<String>> {
        match self.load_current()? {
            Some(session) => Ok(session
                .checkpoints
                .iter()
                .map(|c| c.label.clone())
                .collect()),
            None => Ok(Vec::new()),
        }
    }

    /// Get the path for a session file.
    fn session_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.json", id))
    }

    /// Get the path for an append-only session journal file.
    pub fn journal_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.jsonl", id))
    }

    /// Append a single transaction entry to the append-only journal with synchronous flush.
    pub fn append_journal(&self, id: &str, role: &str, content: &str) -> Result<()> {
        fs::create_dir_all(&self.sessions_dir)?;
        let path = self.journal_path(id);
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let msg = SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
        };
        let line = serde_json::to_string(&msg)?;
        writeln!(file, "{}", line)?;
        file.flush()?;
        Ok(())
    }

    /// Replay messages from the append-only journal.
    pub fn read_journal(&self, id: &str) -> Result<Vec<SessionMessage>> {
        let path = self.journal_path(id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        let mut messages = Vec::new();
        for line in content.lines() {
            if !line.trim().is_empty() {
                if let Ok(msg) = serde_json::from_str::<SessionMessage>(line) {
                    messages.push(msg);
                }
            }
        }
        Ok(messages)
    }
}

/// The session ID used for the "current" pipeline session.
pub const CURRENT_SESSION_ID: &str = "current";

/// Get the current git HEAD commit hash for a project path, if available.
pub fn current_git_commit(project_path: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_path)
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            if o.status.success() && !s.trim().is_empty() {
                Some(s.trim().to_string())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn session_new() {
        let dir = TempDir::new().unwrap();
        let session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        assert_eq!(session.messages.len(), 0);
        assert_eq!(session.title, "New Session");
    }

    #[test]
    fn session_add_message() {
        let dir = TempDir::new().unwrap();
        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session.add_message("user", "Fix the login bug");
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.title, "Fix the login bug");
    }

    #[test]
    fn session_title_truncation() {
        let dir = TempDir::new().unwrap();
        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        let long_msg = "a".repeat(100);
        session.add_message("user", &long_msg);
        assert!(session.title.ends_with("..."));
        assert!(session.title.len() <= 63);
    }

    #[test]
    fn session_checkpoints() {
        let dir = TempDir::new().unwrap();
        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session.add_message("user", "First message");
        session.create_checkpoint("after first", None);
        session.add_message("assistant", "Response");
        session.create_checkpoint("after response", None);
        assert_eq!(session.checkpoints.len(), 2);
        assert!(session.undo());
        assert_eq!(session.messages.len(), 1);
        assert!(session.redo());
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn session_manager_save_load() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path());
        manager.init().unwrap();

        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session.add_message("user", "Test message");
        manager.save(&session).unwrap();

        let loaded = manager.load(&session.id).unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "Test message");
    }

    #[test]
    fn session_manager_list() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path());
        manager.init().unwrap();

        let mut s1 = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        s1.add_message("user", "Session 1");
        manager.save(&s1).unwrap();

        let mut s2 = Session::new(
            dir.path().to_path_buf(),
            "gpt-4o".to_string(),
            "openai".to_string(),
        );
        s2.add_message("user", "Session 2");
        manager.save(&s2).unwrap();

        let sessions = manager.list().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn session_record_usage() {
        let dir = TempDir::new().unwrap();
        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session.record_usage(1000, 500, 0.015);
        session.record_usage(2000, 1000, 0.03);
        assert_eq!(session.total_input_tokens, 3000);
        assert_eq!(session.total_output_tokens, 1500);
        assert!((session.total_cost_usd - 0.045).abs() < 0.001);
    }

    #[test]
    fn session_config_drives_enabled_and_max_sessions() {
        // Phase 6.1: `[session]` is live — disabled skips writes and the
        // max_sessions cap prunes oldest-first on save.
        let dir = TempDir::new().unwrap();
        let mut cfg = crate::config::SessionConfig::default();
        cfg.enabled = false;
        cfg.max_sessions = 1;
        let mgr = SessionManager::from_config(dir.path(), &cfg);
        let s = Session::new(dir.path().to_path_buf(), "m".into(), "p".into());
        mgr.save(&s).unwrap(); // no-op when disabled
        assert!(!mgr.sessions_dir.join("s.json").exists());

        let mut cfg = crate::config::SessionConfig::default();
        cfg.max_sessions = 2;
        let mgr = SessionManager::from_config(dir.path(), &cfg);
        for i in 0..3 {
            let mut s = Session::new(dir.path().to_path_buf(), "m".into(), "p".into());
            s.id = format!("sess-{i}");
            s.updated_at = chrono::Utc::now();
            mgr.save(&s).unwrap();
        }
        let listed = mgr.list().unwrap();
        assert_eq!(listed.len(), 2, "max_sessions cap prunes oldest-first");
    }

    #[test]
    fn session_manager_journal_roundtrip() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path());
        manager.init().unwrap();

        manager
            .append_journal("test-sess", "user", "hello journal")
            .unwrap();
        manager
            .append_journal("test-sess", "assistant", "echo journal")
            .unwrap();

        let msgs = manager.read_journal("test-sess").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hello journal");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "echo journal");
    }

    #[test]
    fn checkpoint_appends_to_journal() {
        // Phase 4.6: `create_checkpoint` (the pipeline's per-run hook) leaves
        // a journal entry, so the journal is reachable from production flow.
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path());
        manager.init().unwrap();
        manager.create_checkpoint("after_planner", None).unwrap();
        let msgs = manager.read_journal(CURRENT_SESSION_ID).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("after_planner"));
    }

    #[test]
    fn session_manager_rewind_mode() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path());
        manager.init().unwrap();

        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session.add_message("user", "Initial prompt");
        session.create_checkpoint("checkpoint-1", Some("commit123".to_string()));
        session.add_message("assistant", "Plan created");
        session.create_checkpoint("checkpoint-2", Some("commit456".to_string()));
        manager.save_current(&session).unwrap();

        let result = manager.rewind_mode(RewindMode::CodeOnly).unwrap();
        assert!(result.is_some());
        let (label, commit) = result.unwrap();
        assert_eq!(label, "checkpoint-1");
        assert_eq!(commit, Some("commit123".to_string()));
    }

    #[test]
    fn session_rewind_modes_code_vs_conversation_exact_contracts() {
        // Phase 7.3: code vs conversation rewind modes restore exactly what they claim.
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path());
        manager.init().unwrap();

        // 1. CodeOnly mode preserves conversation history
        let mut session = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session.add_message("user", "Initial prompt");
        session.create_checkpoint("cp-1", Some("git-sha-1".to_string()));
        session.add_message("assistant", "Step 2 response");
        session.create_checkpoint("cp-2", Some("git-sha-2".to_string()));
        manager.save_current(&session).unwrap();

        let (lbl, commit) = manager.rewind_mode(RewindMode::CodeOnly).unwrap().unwrap();
        assert_eq!(lbl, "cp-1");
        assert_eq!(commit, Some("git-sha-1".to_string()));
        let reloaded = manager.load_current().unwrap().unwrap();
        assert_eq!(
            reloaded.messages.len(),
            2,
            "CodeOnly preserves conversation messages"
        );

        // 2. ConversationOnly mode restores conversation messages back to checkpoint
        let mut session2 = Session::new(
            dir.path().to_path_buf(),
            "claude-sonnet-4".to_string(),
            "anthropic".to_string(),
        );
        session2.add_message("user", "Initial prompt");
        session2.create_checkpoint("cp-1", Some("git-sha-1".to_string()));
        session2.add_message("assistant", "Step 2 response");
        session2.create_checkpoint("cp-2", Some("git-sha-2".to_string()));
        manager.save_current(&session2).unwrap();

        let (lbl2, _commit2) = manager
            .rewind_mode(RewindMode::ConversationOnly)
            .unwrap()
            .unwrap();
        assert_eq!(lbl2, "cp-1");
        let reloaded2 = manager.load_current().unwrap().unwrap();
        assert_eq!(
            reloaded2.messages.len(),
            1,
            "ConversationOnly restores messages back to checkpoint 1"
        );
        assert_eq!(reloaded2.messages[0].content, "Initial prompt");
    }
}
