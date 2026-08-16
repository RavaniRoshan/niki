use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Detect the host agent environment and return the appropriate goals directory name.
///
/// Detection order (SKILL.md §0):
/// 1. OpenCode — `.opencode/` dir or `OPENCODE_*` env or `opencode.json` exists
/// 2. KiloCode — `.kilo/` dir or `KILO_*` env
/// 3. Claude Code — `.claude/` dir (fallback)
pub fn env_dir() -> &'static str {
    env_dir_at(std::env::current_dir().unwrap_or_default())
}

fn env_dir_at(cwd: std::path::PathBuf) -> &'static str {
    if cwd.join(".opencode").exists()
        || std::env::var("OPENCODE_SESSION_ID").is_ok()
        || cwd.join("opencode.json").exists()
        || cwd.join("opencode.jsonc").exists()
    {
        return ".opencode/goals";
    }
    if cwd.join(".kilo").exists() || std::env::var("KILO_SESSION_ID").is_ok() {
        return ".kilo/goals";
    }
    if cwd.join(".claude").exists() {
        return ".claude/goals";
    }
    ".opencode/goals"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalState {
    pub id: String,
    pub slug: String,
    pub objective: String,
    pub status: GoalStatus,
    pub branch: String,
    pub scope: String,
    pub scope_lock: Vec<String>,
    pub scope_flex: Vec<String>,
    pub criteria: Vec<GoalCriterion>,
    pub tasks: Vec<GoalTask>,
    pub current_task: usize,
    pub iterations: u32,
    pub budget_used: u64,
    pub max_iterations: u32,
    pub negative_knowledge: Vec<String>,
    pub context_summary: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    /// Drift detection signals (Phase 13).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift: Option<DriftSignals>,
    /// Fork artifacts directory (set when goal is forked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalStatus {
    Active,
    Paused,
    Complete,
    Cancelled,
    Drifting,
}

impl std::fmt::Display for GoalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GoalStatus::Active => write!(f, "active"),
            GoalStatus::Paused => write!(f, "paused"),
            GoalStatus::Complete => write!(f, "complete"),
            GoalStatus::Cancelled => write!(f, "cancelled"),
            GoalStatus::Drifting => write!(f, "drifting"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalCriterion {
    pub label: String,
    pub check: String,
    pub must_pass: bool,
    pub coverage_gate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftSignals {
    /// Cosine similarity between current output and goal embedding (0..1).
    pub goal_adherence: f64,
    /// Environment-action coherence score (0..1).
    pub env_coherence: f64,
    /// Re-entry repetition rate (0..1).
    pub reentry_rate: f64,
    /// Last-checked timestamp.
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalTask {
    pub id: u32,
    pub desc: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Todo => write!(f, "todo"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Done => write!(f, "done"),
            TaskStatus::Blocked => write!(f, "blocked"),
        }
    }
}

pub fn goals_dir() -> std::path::PathBuf {
    std::env::current_dir()
        .map(|cwd| cwd.join(env_dir()))
        .unwrap_or_else(|_| std::env::temp_dir().join("niki-goals"))
}

pub fn state_path(slug: &str, id: &str) -> std::path::PathBuf {
    goals_dir().join(format!("{}-{}.json", slug, id))
}

pub fn claim_path(session_id: &str) -> std::path::PathBuf {
    goals_dir().join(format!("session-{}.goal", session_id))
}

impl GoalState {
    pub fn save(&self) -> Result<()> {
        let dir = goals_dir();
        fs::create_dir_all(&dir)?;
        let path = state_path(&self.slug, &self.id);
        let json = serde_json::to_string_pretty(self)?;
        crate::util::write_restricted(&path, json)?;
        Ok(())
    }

    pub fn load(slug: &str, id: &str) -> Result<Self> {
        let path = state_path(slug, id);
        let content = fs::read_to_string(&path)?;
        let state: GoalState = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub fn load_all() -> Result<Vec<GoalState>> {
        let dir = goals_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut states = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(state) = serde_json::from_str::<GoalState>(&content)
            {
                states.push(state);
            }
        }
        Ok(states)
    }

    pub fn active_goal() -> Result<Option<Self>> {
        let claims = claim_files()?;
        if claims.is_empty() {
            return Ok(None);
        }
        let latest = claims
            .into_iter()
            .max_by_key(|c| c.claimed_at.clone())
            .ok_or_else(|| anyhow::anyhow!("No claim files found"))?;
        Self::find_by_id(&latest.goal_id)
    }

    pub fn find_by_id(id: &str) -> Result<Option<Self>> {
        let states = Self::load_all()?;
        Ok(states.into_iter().find(|s| s.id == id))
    }

    pub fn archive(&self) -> Result<()> {
        let dir = goals_dir();
        let archived = dir.join("archive");
        fs::create_dir_all(&archived)?;
        let path = state_path(&self.slug, &self.id);
        if path.exists() {
            fs::rename(&path, archived.join(path.file_name().unwrap()))?;
        }
        Ok(())
    }

    /// Write fork artifacts (goal.md, progress.json, drift.jsonl, environment.lock)
    /// to a new directory under `.niki/goals/forks/<goal-id>/`.
    pub fn fork(&self) -> Result<PathBuf> {
        let fork_base = goals_dir().join("forks").join(&self.id);
        fs::create_dir_all(&fork_base)?;

        // goal.md — immutable objective
        let goal_md = format!("# Goal: {}\n\n{}\n", self.objective, self.slug);
        fs::write(fork_base.join("goal.md"), goal_md)?;

        // progress.json — current milestones + task states
        let progress = serde_json::json!({
            "id": self.id,
            "slug": self.slug,
            "status": self.status,
            "iterations": self.iterations,
            "budget_used": self.budget_used,
            "current_task": self.current_task,
            "tasks": self.tasks,
            "criteria": self.criteria,
        });
        fs::write(
            fork_base.join("progress.json"),
            serde_json::to_string_pretty(&progress)?,
        )?;

        // drift.jsonl — one JSON line per checkpoint
        if let Some(ref drift) = self.drift {
            let line = serde_json::to_string(drift)?;
            fs::write(fork_base.join("drift.jsonl"), format!("{}\n", line))?;
        }

        // environment.lock — lockfile hash + branch SHA
        let env_lock = serde_json::json!({
            "branch": self.branch,
            "scope": self.scope,
            "created_at": self.created_at,
        });
        fs::write(
            fork_base.join("environment.lock"),
            serde_json::to_string_pretty(&env_lock)?,
        )?;

        // open-questions.md
        let oq = format!(
            "# Open Questions\n\nGenerated by NIKI fork at {}\n\n",
            chrono::Utc::now().to_rfc3339()
        );
        fs::write(fork_base.join("open-questions.md"), oq)?;

        Ok(fork_base)
    }

    /// Compute cheap drift signals without an LLM call.
    /// Returns `Some(DriftSignals)` if any signal breaches its threshold,
    /// indicating the goal is drifting.
    pub fn check_drift(&self) -> Option<DriftSignals> {
        // Placeholder: in a full implementation these would be computed from
        // recent pipeline outputs. For now we return None (no drift detected)
        // so existing behavior is preserved.
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimFile {
    pub session_id: String,
    pub goal_id: String,
    pub claimed_at: String,
}

pub fn claim_files() -> Result<Vec<ClaimFile>> {
    let dir = goals_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut claims = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("session-") && name.ends_with(".goal") {
            let content = fs::read_to_string(&path)?;
            if let Ok(claim) = serde_json::from_str::<ClaimFile>(&content) {
                claims.push(claim);
            }
        }
    }
    Ok(claims)
}

pub fn create_claim(session_id: &str, goal_id: &str) -> Result<()> {
    let dir = goals_dir();
    fs::create_dir_all(&dir)?;
    let path = claim_path(session_id);
    let claim = ClaimFile {
        session_id: session_id.to_string(),
        goal_id: goal_id.to_string(),
        claimed_at: Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_string_pretty(&claim)?;
    crate::util::write_restricted(&path, json)?;
    Ok(())
}

pub fn remove_claim(session_id: &str) -> Result<()> {
    let path = claim_path(session_id);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn remove_claim_by_goal(goal_id: &str) -> Result<()> {
    let claims = claim_files()?;
    for claim in claims {
        if claim.goal_id == goal_id {
            remove_claim(&claim.session_id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal::TEST_CWD_LOCK;
    use tempfile::TempDir;

    fn make_state() -> GoalState {
        GoalState {
            id: "test12".to_string(),
            slug: "test-goal".to_string(),
            objective: "Test objective".to_string(),
            status: GoalStatus::Active,
            branch: "goal/test-goal-test12".to_string(),
            scope: ".".to_string(),
            scope_lock: vec![".".to_string()],
            scope_flex: vec![],
            criteria: vec![],
            tasks: vec![],
            current_task: 0,
            iterations: 0,
            budget_used: 0,
            max_iterations: 30,
            negative_knowledge: vec![],
            context_summary: String::new(),
            created_at: Utc::now().to_rfc3339(),
            completed_at: None,
            drift: None,
            fork_dir: None,
        }
    }

    #[test]
    fn test_state_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let _guard = TEST_CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let state = make_state();
        state.save().unwrap();
        let loaded = GoalState::load("test-goal", "test12").unwrap();
        assert_eq!(loaded.objective, "Test objective");
        assert_eq!(loaded.status, GoalStatus::Active);

        std::env::set_current_dir(original_cwd).unwrap();
    }

    #[test]
    fn test_claim_create_and_remove() {
        let tmp = TempDir::new().unwrap();
        let _guard = TEST_CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        create_claim("sess-abc", "goal123").unwrap();
        let claims = claim_files().unwrap();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].goal_id, "goal123");

        remove_claim("sess-abc").unwrap();
        let claims = claim_files().unwrap();
        assert_eq!(claims.len(), 0);

        std::env::set_current_dir(original_cwd).unwrap();
    }

    #[test]
    fn test_load_all_empty() {
        let tmp = TempDir::new().unwrap();
        let _guard = TEST_CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let states = GoalState::load_all().unwrap();
        assert!(states.is_empty());

        std::env::set_current_dir(original_cwd).unwrap();
    }

    #[test]
    fn test_active_goal_with_claim() {
        let tmp = TempDir::new().unwrap();
        let _guard = TEST_CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let state = make_state();
        state.save().unwrap();
        create_claim("sess-test12", "test12").unwrap();

        let active = GoalState::active_goal().unwrap();
        assert!(active.is_some());
        assert_eq!(active.unwrap().objective, "Test objective");

        remove_claim("sess-test12").unwrap();
        std::env::set_current_dir(original_cwd).unwrap();
    }

    #[test]
    fn test_find_by_id() {
        let tmp = TempDir::new().unwrap();
        let _guard = TEST_CWD_LOCK.lock().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let mut state = make_state();
        state.id = "abc123".to_string();
        state.slug = "my-goal".to_string();
        state.save().unwrap();

        let found = GoalState::find_by_id("abc123").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().slug, "my-goal");

        let not_found = GoalState::find_by_id("nonexistent").unwrap();
        assert!(not_found.is_none());

        std::env::set_current_dir(original_cwd).unwrap();
    }

    #[test]
    fn test_env_dir_default() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(env_dir_at(tmp.path().to_path_buf()), ".opencode/goals");
    }

    #[test]
    fn test_env_dir_opencode() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".opencode")).unwrap();
        assert_eq!(env_dir_at(tmp.path().to_path_buf()), ".opencode/goals");
        std::fs::remove_dir_all(tmp.path().join(".opencode")).unwrap();
    }

    #[test]
    fn test_env_dir_kilocode() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".kilo")).unwrap();
        assert_eq!(env_dir_at(tmp.path().to_path_buf()), ".kilo/goals");
        std::fs::remove_dir_all(tmp.path().join(".kilo")).unwrap();
    }

    #[test]
    fn test_env_dir_claudecode() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
        assert_eq!(env_dir_at(tmp.path().to_path_buf()), ".claude/goals");
        std::fs::remove_dir_all(tmp.path().join(".claude")).unwrap();
    }

    #[test]
    fn test_env_dir_opencode_wins_over_kilo() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".opencode")).unwrap();
        std::fs::create_dir_all(tmp.path().join(".kilo")).unwrap();
        assert_eq!(env_dir_at(tmp.path().to_path_buf()), ".opencode/goals");
        std::fs::remove_dir_all(tmp.path().join(".opencode")).unwrap();
        std::fs::remove_dir_all(tmp.path().join(".kilo")).unwrap();
    }
}
