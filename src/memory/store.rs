use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Utc;

use crate::artifacts::types::AgentRole;

/// A single memory entry recorded after a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// ISO timestamp of when this entry was recorded.
    pub timestamp: String,
    /// The task description that produced this memory.
    pub task: String,
    /// Role-specific tags for filtering (e.g. "error-pattern", "convention", "edge-case").
    pub tags: Vec<String>,
    /// The actual memory content (free-form markdown).
    pub content: String,
    /// The git branch this was recorded on (for traceability).
    pub branch: Option<String>,
}

/// Role-specific memory file, stored at `.niki/memory/{role}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMemory {
    pub role: AgentRole,
    pub entries: Vec<MemoryEntry>,
}

impl Default for RoleMemory {
    fn default() -> Self {
        Self {
            role: AgentRole::Planner,
            entries: Vec::new(),
        }
    }
}

/// Load role memory from `.niki/memory/{role}.json`. Returns empty memory if file doesn't exist.
pub fn load_memory(project_dir: &Path, role: AgentRole) -> RoleMemory {
    let path = memory_path(project_dir, role);
    if !path.exists() {
        return RoleMemory { role, entries: Vec::new() };
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(RoleMemory { role, entries: Vec::new() })
}

/// Save role memory to `.niki/memory/{role}.json`.
pub fn save_memory(project_dir: &Path, memory: &RoleMemory) -> Result<()> {
    let dir = project_dir.join(".niki").join("memory");
    fs::create_dir_all(&dir).context("creating .niki/memory directory")?;
    let path = memory_path(project_dir, memory.role);
    let json = serde_json::to_string_pretty(memory)?;
    fs::write(&path, json)?;
    Ok(())
}

/// Append a new entry to a role's memory and save.
pub fn append_memory(
    project_dir: &Path,
    role: AgentRole,
    task: &str,
    tags: Vec<String>,
    content: String,
    branch: Option<String>,
) -> Result<()> {
    let mut memory = load_memory(project_dir, role);
    memory.entries.push(MemoryEntry {
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        task: task.to_string(),
        tags,
        content,
        branch,
    });
    // Keep memory bounded: retain last 100 entries per role
    if memory.entries.len() > 100 {
        let drain_count = memory.entries.len() - 100;
        memory.entries.drain(..drain_count);
    }
    save_memory(project_dir, &memory)
}

/// Render memory entries as a string suitable for injection into prompts.
/// Returns empty string if no memory exists.
pub fn render_memory_for_prompt(project_dir: &Path, role: AgentRole, max_entries: usize) -> String {
    let memory = load_memory(project_dir, role);
    if memory.entries.is_empty() {
        return String::new();
    }
    let mut output = String::from("## Project Memory (Learned from previous runs)\n\n");
    // Show most recent entries first
    for entry in memory.entries.iter().rev().take(max_entries) {
        output.push_str(&format!(
            "- [{}] {}\n  Tags: {}\n  {}\n\n",
            &entry.timestamp[..10], // YYYY-MM-DD
            entry.task.chars().take(100).collect::<String>(),
            entry.tags.join(", "),
            entry.content.chars().take(500).collect::<String>(),
        ));
    }
    output
}

/// Query memory entries by tag across all roles. Returns owned copies.
pub fn query_memory_by_tag(project_dir: &Path, tag: &str) -> Vec<(AgentRole, MemoryEntry)> {
    let roles = vec![
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
        AgentRole::SecurityAuditor,
        AgentRole::Red,
        AgentRole::Synthesizer,
    ];
    let mut results = Vec::new();
    for role in roles {
        let memory = load_memory(project_dir, role);
        for entry in memory.entries {
            if entry.tags.iter().any(|t| t == tag) {
                results.push((role, entry));
            }
        }
    }
    results
}

/// Get all unique tags across all roles.
pub fn get_all_tags(project_dir: &Path) -> Vec<String> {
    let roles = vec![
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
        AgentRole::SecurityAuditor,
        AgentRole::Red,
        AgentRole::Synthesizer,
    ];
    let mut tags = std::collections::HashSet::new();
    for role in roles {
        let memory = load_memory(project_dir, role);
        for entry in &memory.entries {
            for tag in &entry.tags {
                tags.insert(tag.clone());
            }
        }
    }
    let mut tags: Vec<String> = tags.into_iter().collect();
    tags.sort();
    tags
}

fn memory_path(project_dir: &Path, role: AgentRole) -> PathBuf {
    let role_name = match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
    };
    project_dir.join(".niki").join("memory").join(format!("{}.json", role_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn memory_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Empty memory on fresh project
        let mem = load_memory(dir, AgentRole::Coder);
        assert!(mem.entries.is_empty());

        // Append and reload
        append_memory(
            dir,
            AgentRole::Coder,
            "fix null check",
            vec!["error-pattern".into()],
            "Always check Option before unwrap".to_string(),
            Some("main".into()),
        ).unwrap();

        let mem = load_memory(dir, AgentRole::Coder);
        assert_eq!(mem.entries.len(), 1);
        assert_eq!(mem.entries[0].tags, vec!["error-pattern"]);

        // Prompt rendering
        let rendered = render_memory_for_prompt(dir, AgentRole::Coder, 10);
        assert!(rendered.contains("Project Memory"));
        assert!(rendered.contains("fix null check"));

        // Query by tag
        let results = query_memory_by_tag(dir, "error-pattern");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, AgentRole::Coder);
    }

    #[test]
    fn memory_bounded_at_100() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        for i in 0..120 {
            append_memory(
                dir,
                AgentRole::Tester,
                &format!("task {}", i),
                vec![],
                format!("content {}", i),
                None,
            ).unwrap();
        }
        let mem = load_memory(dir, AgentRole::Tester);
        assert_eq!(mem.entries.len(), 100);
        // Oldest entries should be trimmed (task 0..19 gone)
        assert_eq!(mem.entries[0].task, "task 20");
    }

    #[test]
    fn all_tags_collected() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        append_memory(dir, AgentRole::Planner, "t", vec!["convention".into()], "c".to_string(), None).unwrap();
        append_memory(dir, AgentRole::Coder, "t", vec!["error-pattern".into()], "c".to_string(), None).unwrap();
        let tags = get_all_tags(dir);
        assert_eq!(tags, vec!["convention", "error-pattern"]);
    }
}
