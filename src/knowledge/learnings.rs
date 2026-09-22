//! Append-only project learnings: the durable memory of what past runs and
//! history mining discovered.
//!
//! Stored as JSONL at `<output_dir>/learnings.jsonl` (one object per line).
//! Appends use `O_APPEND`-equivalent semantics (open with append + write +
//! best-effort sync), so concurrent runs interleave lines without truncating
//! each other. Readers tolerate malformed lines by skipping them.

use crate::config::NikiConfig;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single durable learning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearningEntry {
    /// Machine-readable kind: `verification_failure`, `review_correction`,
    /// `security_fix`, `cost_anomaly`, or `history`.
    #[serde(default)]
    pub kind: String,
    /// Owning task id, when the learning came from a run.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Snapshot anchor (`niki-task-<8 hex>`) the learning was derived under.
    #[serde(default)]
    pub snapshot_id: String,
    /// Role or subsystem that authored the entry (e.g. `reviewer`,
    /// `history-miner`, `reflect`).
    #[serde(default)]
    pub author_role: String,
    /// Provenance tier: `authoritative`, `inferred`, or `advisory`.
    #[serde(default)]
    pub authority: String,
    /// Human-readable detail (bounded by writers; readers do not enforce).
    #[serde(default)]
    pub details: String,
    /// Optional ordering hint; higher surfaces first in prompts.
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub created_at: DateTime<Utc>,
}

impl LearningEntry {
    pub fn new(
        kind: &str,
        snapshot_id: &str,
        author_role: &str,
        authority: &str,
        details: String,
    ) -> Self {
        Self {
            kind: kind.to_string(),
            task_id: None,
            snapshot_id: snapshot_id.to_string(),
            author_role: author_role.to_string(),
            authority: authority.to_string(),
            details,
            priority: None,
            created_at: Utc::now(),
        }
    }
}

/// Path of the project learnings file, honoring `general.output_dir`.
pub fn learnings_path(project_path: &Path, config: &NikiConfig) -> std::path::PathBuf {
    project_path
        .join(&config.general.output_dir)
        .join("learnings.jsonl")
}

/// Append one entry. Creates parent dirs as needed. The fsync is best-effort:
/// durability is nice-to-have, never fatal.
pub fn append_learning(
    project_path: &Path,
    config: &NikiConfig,
    entry: &LearningEntry,
) -> Result<()> {
    let path = learnings_path(project_path, config);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", serde_json::to_string(entry)?)?;
    let _ = file.sync_all();
    Ok(())
}

/// Read the last `n` entries (oldest first). Malformed lines are skipped, so
/// one bad write never poisons the whole file.
pub fn tail_learnings(project_path: &Path, config: &NikiConfig, n: usize) -> Vec<LearningEntry> {
    let path = learnings_path(project_path, config);
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut entries: Vec<LearningEntry> = content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if entries.len() > n {
        entries.drain(..entries.len() - n);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_tail_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let config = NikiConfig::default();
        for i in 0..3 {
            append_learning(
                tmp.path(),
                &config,
                &LearningEntry::new(
                    "history",
                    "niki-task-test",
                    "history-miner",
                    "inferred",
                    format!("detail {i}"),
                ),
            )
            .unwrap();
        }
        // Tolerate a corrupt line in the middle.
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(learnings_path(tmp.path(), &config))
            .unwrap();
        writeln!(f, "not json{{").unwrap();

        let tail = tail_learnings(tmp.path(), &config, 2);
        assert_eq!(tail.len(), 2);
        assert!(tail[0].details.contains('1'));
        assert!(tail[1].details.contains('2'));
        assert_eq!(tail[0].authority, "inferred");
    }

    #[test]
    fn sparse_learning_parses_with_defaults() {
        // Phase 2.1: old fixtures missing new fields must still parse.
        let entry: LearningEntry = serde_json::from_str(r#"{"kind":"history"}"#).unwrap();
        assert_eq!(entry.kind, "history");
        assert!(entry.details.is_empty());
    }

    #[test]
    fn interleaved_appends_lose_no_entries() {
        // Phase 2.1: sequential appends (interleaved-line model) keep every
        // entry and leave valid JSONL.
        let tmp = tempfile::tempdir().unwrap();
        let config = NikiConfig::default();
        for i in 0..20 {
            append_learning(
                tmp.path(),
                &config,
                &LearningEntry::new(
                    "history",
                    "niki-task-test",
                    "history-miner",
                    "inferred",
                    format!("detail {i}"),
                ),
            )
            .unwrap();
        }
        let content = std::fs::read_to_string(learnings_path(tmp.path(), &config)).unwrap();
        assert_eq!(content.lines().count(), 20);
        for line in content.lines() {
            serde_json::from_str::<LearningEntry>(line).expect("every JSONL line must parse");
        }
        assert_eq!(tail_learnings(tmp.path(), &config, 50).len(), 20);
    }

    #[test]
    fn tail_on_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(tail_learnings(tmp.path(), &NikiConfig::default(), 5).is_empty());
    }
}
