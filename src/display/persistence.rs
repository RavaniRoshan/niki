//! Chat session persistence — resume conversations across TUI restarts.
//!
//! Mirrors the onboarding persistence convention: state is stored under the
//! project-local `.niki/` directory as `.niki/chat.json`. Only lightweight,
//! reconstructable state is persisted (chat log, notes, model, revision round);
//! transient pipeline/stage output is intentionally not saved.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::display::state::AppState;

const STATE_DIR: &str = ".niki";
const CHAT_FILE: &str = ".niki/chat.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatSession {
    pub chat_log: Vec<(String, String)>,
    pub notes: Vec<String>,
    pub model: String,
    pub revision_round: u32,
}

fn state_path(project_path: &Path) -> PathBuf {
    project_path.join(CHAT_FILE)
}

fn state_dir(project_path: &Path) -> PathBuf {
    project_path.join(STATE_DIR)
}

/// Load a saved chat session, if present.
pub fn load_chat_session(project_path: &Path) -> Option<ChatSession> {
    let path = state_path(project_path);
    let content = fs::read_to_string(&path).ok()?;
    let session: ChatSession = serde_json::from_str(&content).ok()?;
    Some(session)
}

/// Persist the current chat session to disk.
pub fn save_chat_session(project_path: &Path, session: &ChatSession) -> bool {
    let path = state_path(project_path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(session) {
        Ok(json) => fs::write(&path, json).is_ok(),
        Err(_) => false,
    }
}

/// Populate `state` with a resumed session's chat log / notes / model.
pub fn apply_session(state: &mut AppState, session: ChatSession) {
    state.chat_log = session.chat_log;
    state.notes = session.notes.into_iter().map(|note| (note, ratatui::style::Color::Yellow)).collect();
    if !session.model.is_empty() {
        state.model = session.model;
    }
    state.revision_round = session.revision_round;
    // Rebuild the rendered chat lines so the resumed log shows immediately.
    let width = state.chat_width.get().max(80);
    state.chat_lines = crate::display::pages::chat::build_chat_lines(state, width);
}

/// Extract a saveable snapshot from the live state.
pub fn snapshot(state: &AppState) -> ChatSession {
    ChatSession {
        chat_log: state.chat_log.clone(),
        notes: state.notes.iter().map(|(t, _)| t.clone()).collect(),
        model: state.model.clone(),
        revision_round: state.revision_round,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;
    use std::path::PathBuf;

    fn tmp_project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("niki_persist_{}", std::process::id()));
        let niki_dir = dir.join(".niki");
        let _ = fs::create_dir_all(&niki_dir);
        dir
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tmp_project();
        let session = ChatSession {
            chat_log: vec![("user".to_string(), "hi".to_string())],
            notes: vec!["note one".to_string()],
            model: "test-model".to_string(),
            revision_round: 2,
        };
        assert!(save_chat_session(&dir, &session));
        let loaded = load_chat_session(&dir).expect("session should load");
        assert_eq!(loaded.chat_log, session.chat_log);
        assert_eq!(loaded.notes, session.notes);
        assert_eq!(loaded.model, session.model);
        assert_eq!(loaded.revision_round, 2);
        let _ = fs::remove_dir_all(dir.join(".niki"));
    }

    #[test]
    fn load_missing_session_returns_none() {
        let dir = std::env::temp_dir().join(format!("niki_persist_missing_{}", std::process::id()));
        assert!(load_chat_session(&dir).is_none());
    }

    #[test]
    fn snapshot_captures_state() {
        let config = NikiConfig::default();
        let mut state = AppState::new("task".to_string(), config, PathBuf::from("."));
        state.chat_log.push(("user".to_string(), "hello".to_string()));
        state.model = "m".to_string();
        state.revision_round = 3;
        let snap = snapshot(&state);
        assert_eq!(snap.chat_log.len(), 1);
        assert_eq!(snap.model, "m");
        assert_eq!(snap.revision_round, 3);
    }
}
