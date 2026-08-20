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

pub fn write_audit_entry(task_id: &str, entry: &AuditEntry) {
    let audit_dir = Path::new(".niki").join("audit");
    if let Err(e) = fs::create_dir_all(&audit_dir) {
        eprintln!("Warning: could not create audit directory: {}", e);
        return;
    }

    let file_path = audit_dir.join(format!("{}.jsonl", task_id));
    let line = entry.to_json_line();
    if let Err(e) = crate::util::write_restricted(&file_path, format!("{}\n", line)) {
        eprintln!("Warning: could not write audit entry: {}", e);
    }
}

pub fn append_audit_entry(task_id: &str, entry: &AuditEntry) {
    let audit_dir = Path::new(".niki").join("audit");
    if let Err(e) = fs::create_dir_all(&audit_dir) {
        eprintln!("Warning: could not create audit directory: {}", e);
        return;
    }

    let file_path = audit_dir.join(format!("{}.jsonl", task_id));
    let line = entry.to_json_line();

    let existing = if file_path.exists() {
        fs::read_to_string(&file_path).unwrap_or_default()
    } else {
        String::new()
    };

    if let Err(e) = crate::util::write_restricted(&file_path, format!("{}{}\n", existing, line)) {
        eprintln!("Warning: could not append audit entry: {}", e);
    }
}
