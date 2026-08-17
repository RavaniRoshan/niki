//! Mission-scoped persistence — crash-safe, relaunch-restorable storage.
//!
//! Missions are stored as JSON files under `<project_root>/.niki/missions/<id>.json`.
//! Because `Mission` contains monotonic `Instant` timestamps (not serializable),
//! we snapshot them to wall-clock Unix seconds and reconstruct relative `Instant`s
//! on load so elapsed-time displays remain correct after a relaunch.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::mission::{Mission, MissionId, MissionStatus, SessionId};

/// Serialized, restorable view of a `Mission`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSnapshot {
    pub id: String,
    pub description: String,
    pub status: String,
    pub sessions: Vec<String>,
    pub active_session: Option<String>,
    pub progress: f64,
    pub cost_usd: f64,
    pub created_unix: u64,
    pub started_unix: Option<u64>,
    pub completed_unix: Option<u64>,
    pub error: Option<String>,
    pub branch: Option<String>,
    pub model: String,
    pub attention: u8,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn instant_to_unix(i: Instant) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let elapsed = i.elapsed();
    now.saturating_sub(elapsed).as_secs()
}

fn unix_to_instant(u: u64) -> Instant {
    let now = unix_now();
    let delta = now.saturating_sub(u);
    Instant::now() - Duration::from_secs(delta)
}

fn attention_to_u8(m: &Mission) -> u8 {
    match m.attention {
        crate::mission::AttentionPriority::Normal => 0,
        crate::mission::AttentionPriority::Waiting => 1,
        crate::mission::AttentionPriority::NeedsAttention => 2,
        crate::mission::AttentionPriority::Error => 3,
    }
}

/// Convert a live `Mission` into a storable snapshot.
pub fn snapshot_of(m: &Mission) -> MissionSnapshot {
    MissionSnapshot {
        id: m.id.0.clone(),
        description: m.description.clone(),
        status: m.status.status_str().to_string(),
        sessions: m.sessions.iter().map(|s| s.0.clone()).collect(),
        active_session: m.active_session.as_ref().map(|s| s.0.clone()),
        progress: m.progress,
        cost_usd: m.cost_usd,
        created_unix: instant_to_unix(m.created_at),
        started_unix: m.started_at.map(instant_to_unix),
        completed_unix: m.completed_at.map(instant_to_unix),
        error: m.error.clone(),
        branch: m.branch.clone(),
        model: m.model.clone(),
        attention: attention_to_u8(m),
    }
}

/// Reconstruct a live `Mission` from a snapshot.
pub fn mission_from(s: &MissionSnapshot) -> Mission {
    use crate::mission::AttentionPriority;

    let status = match s.status.as_str() {
        "RUNNING" => MissionStatus::Running,
        "PAUSED" => MissionStatus::Paused,
        "COMPLETED" => MissionStatus::Completed,
        "FAILED" => MissionStatus::Failed,
        "CANCELLED" => MissionStatus::Cancelled,
        _ => MissionStatus::Created,
    };
    let attention = match s.attention {
        1 => AttentionPriority::Waiting,
        2 => AttentionPriority::NeedsAttention,
        3 => AttentionPriority::Error,
        _ => AttentionPriority::Normal,
    };

    Mission {
        id: MissionId(s.id.clone()),
        description: s.description.clone(),
        status,
        sessions: s.sessions.iter().map(|x| SessionId(x.clone())).collect(),
        active_session: s.active_session.as_ref().map(|x| SessionId(x.clone())),
        progress: s.progress,
        cost_usd: s.cost_usd,
        created_at: unix_to_instant(s.created_unix),
        started_at: s.started_unix.map(unix_to_instant),
        completed_at: s.completed_unix.map(unix_to_instant),
        error: s.error.clone(),
        branch: s.branch.clone(),
        model: s.model.clone(),
        attention,
    }
}

/// Directory holding per-mission JSON files: `<root>/.niki/missions`.
pub fn missions_dir(root: &Path) -> std::path::PathBuf {
    root.join(".niki").join("missions")
}

fn snapshot_path(root: &Path, id: &str) -> std::path::PathBuf {
    missions_dir(root).join(format!("{}.json", sanitize_id(id)))
}

/// Sanitize an id so it can't escape the missions directory (path traversal).
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Persist a mission to disk (creates `.niki/missions` if needed).
pub fn save_mission(root: &Path, m: &Mission) -> Result<()> {
    let dir = missions_dir(root);
    std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow!("failed to create missions dir {}: {}", dir.display(), e))?;
    let path = snapshot_path(root, &m.id.0);
    let json = serde_json::to_string_pretty(&snapshot_of(m))?;
    std::fs::write(&path, json)
        .map_err(|e| anyhow!("failed to write mission {}: {}", path.display(), e))?;
    Ok(())
}

/// Load a single mission by id. Returns `Ok(None)` if the file is absent.
pub fn load_mission(root: &Path, id: &str) -> Result<Option<Mission>> {
    let path = snapshot_path(root, id);
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read mission {}: {}", path.display(), e))?;
    let snap: MissionSnapshot = serde_json::from_str(&json)?;
    Ok(Some(mission_from(&snap)))
}

/// List all persisted missions under the root.
pub fn list_missions(root: &Path) -> Result<Vec<Mission>> {
    let dir = missions_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let json = std::fs::read_to_string(&path)?;
        match serde_json::from_str::<MissionSnapshot>(&json) {
            Ok(snap) => out.push(mission_from(&snap)),
            Err(e) => {
                tracing::warn!(target: "niki::persistence", "skipping corrupt mission {}: {}", path.display(), e)
            }
        }
    }
    Ok(out)
}

/// Delete a persisted mission file.
pub fn delete_mission(root: &Path, id: &str) -> Result<()> {
    let path = snapshot_path(root, id);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| anyhow!("failed to delete mission {}: {}", path.display(), e))?;
    }
    Ok(())
}

/// Restore the most-recently-created persisted mission (used on relaunch).
pub fn restore_latest(root: &Path) -> Result<Option<Mission>> {
    let mut missions = list_missions(root)?;
    if missions.is_empty() {
        return Ok(None);
    }
    missions.sort_by_key(|m| m.created_at);
    Ok(missions.into_iter().last())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(suffix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "niki-persist-test-{}-{}",
            std::process::id(),
            suffix
        ))
    }

    #[test]
    fn roundtrip_mission() {
        let root = test_root("roundtrip");
        let _ = std::fs::remove_dir_all(&root);
        let m = Mission::new(
            MissionId("mission-1".into()),
            "fix auth bug".into(),
            "sonnet".into(),
        );
        save_mission(&root, &m).unwrap();

        let loaded = load_mission(&root, "mission-1").unwrap().unwrap();
        assert_eq!(loaded.id.0, "mission-1");
        assert_eq!(loaded.description, "fix auth bug");
        assert_eq!(loaded.model, "sonnet");
        assert_eq!(loaded.status, MissionStatus::Created);

        let all = list_missions(&root).unwrap();
        assert_eq!(all.len(), 1);

        delete_mission(&root, "mission-1").unwrap();
        assert!(load_mission(&root, "mission-1").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sanitize_prevents_traversal() {
        let root = test_root("sanitize");
        let _ = std::fs::remove_dir_all(&root);
        // An id with slashes/dots must be sanitized so the write stays in-dir
        // (and can be read back via the same sanitization path).
        let m = Mission::new(MissionId("../evil".into()), "x".into(), "s".into());
        save_mission(&root, &m).unwrap();
        let loaded = load_mission(&root, "../evil").unwrap();
        assert!(loaded.is_some());
        // The file must live *inside* the missions dir, never escape it.
        let dir = missions_dir(&root);
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            assert!(
                p.starts_with(&dir),
                "mission file escaped its dir: {}",
                p.display()
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_preserved() {
        let root = test_root("status");
        let _ = std::fs::remove_dir_all(&root);
        let mut m = Mission::new(MissionId("m2".into()), "d".into(), "s".into());
        m.status = MissionStatus::Running;
        m.progress = 0.5;
        m.cost_usd = 1.25;
        save_mission(&root, &m).unwrap();
        let loaded = load_mission(&root, "m2").unwrap().unwrap();
        assert_eq!(loaded.status, MissionStatus::Running);
        assert_eq!(loaded.progress, 0.5);
        assert_eq!(loaded.cost_usd, 1.25);
        let _ = std::fs::remove_dir_all(&root);
    }
}
