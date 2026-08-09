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
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

/// Checkpoint for undo/redo — captures state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub label: String,
    pub messages: Vec<SessionMessage>,
    pub timestamp: DateTime<Utc>,
    pub git_commit: Option<String>,
}

/// The full state of a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project_path: PathBuf,
    pub title: String,
    pub messages: Vec<SessionMessage>,
    pub model: String,
    pub provider: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub checkpoints: Vec<Checkpoint>,
    pub current_checkpoint: Option<usize>,
    pub metadata: HashMap<String, String>,
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
    sessions_dir: PathBuf,
}

impl SessionManager {
    /// Create a session manager for the given project.
    pub fn new(project_path: &Path) -> Self {
        Self {
            sessions_dir: project_path.join(".niki").join("sessions"),
        }
    }

    /// Initialize the sessions directory.
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.sessions_dir)?;
        Ok(())
    }

    /// Save a session to disk.
    pub fn save(&self, session: &Session) -> Result<()> {
        fs::create_dir_all(&self.sessions_dir)?;
        let path = self.session_path(&session.id);
        let json = serde_json::to_string_pretty(session)?;
        crate::util::write_restricted(&path, json)?;
        Ok(())
    }

    /// Load a session by ID.
    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.session_path(id);
        let json = fs::read_to_string(&path)?;
        let session: Session = serde_json::from_str(&json)?;
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

    /// Get the path for a session file.
    fn session_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.json", id))
    }
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
}
