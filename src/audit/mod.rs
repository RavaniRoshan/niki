pub mod hooks;
pub use hooks::*;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub action: String,
    pub details: Value,
}

impl AuditEntry {
    pub fn new(action: &str, details: Value) -> Self {
        AuditEntry {
            timestamp: Utc::now().to_rfc3339(),
            action: action.to_string(),
            details,
        }
    }

    pub fn to_json_line(&self) -> String {
        serde_json::to_string(&self).unwrap_or_default()
    }
}

/// Append one entry to the task's audit trail
/// (`<project>/.niki/audit/<task_id>.jsonl`).
///
/// Phase 4.6: the single live writer. Uses `O_APPEND` so concurrent runs
/// interleave lines instead of clobbering each other (the old
/// `write_audit_entry` overwrote the file and the read-modify-write append
/// raced — both deleted). Best-effort: failures warn, never abort the run.
pub fn append_audit_entry(project_path: &Path, task_id: &str, entry: &AuditEntry) {
    let audit_dir = project_path.join(".niki").join("audit");
    if let Err(e) = fs::create_dir_all(&audit_dir) {
        eprintln!("Warning: could not create audit directory: {}", e);
        return;
    }
    let file_path = audit_dir.join(format!("{}.jsonl", task_id));
    let line = entry.to_json_line();
    let res = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .and_then(|mut f| {
            use std::io::Write as _;
            writeln!(f, "{line}").and_then(|_| {
                f.sync_all()
                    .map_err(|_| std::io::Error::other("fsync failed"))
            })
        });
    if let Err(e) = res {
        eprintln!("Warning: could not append audit entry: {}", e);
    }
}

#[cfg(test)]
mod audit_writer_tests {
    use super::*;

    #[test]
    fn append_creates_project_scoped_jsonl() {
        // Phase 4.6: entries land under the project dir, one JSON object per
        // line, and repeated appends accumulate instead of overwriting.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        append_audit_entry(dir, "task-1", &AuditEntry::new("a", serde_json::json!({})));
        append_audit_entry(dir, "task-1", &AuditEntry::new("b", serde_json::json!({})));
        let text = std::fs::read_to_string(dir.join(".niki").join("audit").join("task-1.jsonl"))
            .expect("audit file written under the project dir");
        assert_eq!(text.lines().count(), 2, "appends must accumulate");
        for line in text.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("every line parses");
        }
    }
}
