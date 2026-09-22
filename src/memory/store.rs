use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::artifacts::types::AgentRole;

/// A single memory entry recorded after a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// ISO timestamp of when this entry was recorded.
    #[serde(default)]
    pub timestamp: String,
    /// The task description that produced this memory.
    #[serde(default)]
    pub task: String,
    /// Role-specific tags for filtering (e.g. "error-pattern", "convention", "edge-case").
    #[serde(default)]
    pub tags: Vec<String>,
    /// The actual memory content (free-form markdown).
    #[serde(default)]
    pub content: String,
    /// The git branch this was recorded on (for traceability).
    #[serde(default)]
    pub branch: Option<String>,
    /// Stable content hash used for dedupe (task + tags + content).
    #[serde(default = "default_content_hash")]
    pub content_hash: String,
    /// Number of times retrieval injected this entry into a prompt.
    #[serde(default)]
    pub use_count: u32,
    /// ISO timestamp of the last retrieval that injected this entry.
    #[serde(default)]
    pub last_used: Option<String>,
}

fn default_content_hash() -> String {
    String::new()
}

/// Compute a stable dedupe key from task + tags + content.
pub fn memory_content_hash(task: &str, tags: &[String], content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    task.hash(&mut h);
    for t in tags {
        t.hash(&mut h);
    }
    content.hash(&mut h);
    format!("{:x}", h.finish())
}

/// Role-specific memory file, stored at `.niki/memory/{role}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleMemory {
    #[serde(default = "default_role")]
    pub role: AgentRole,
    #[serde(default)]
    pub entries: Vec<MemoryEntry>,
    /// Store schema version; mismatches warn loudly instead of emptying silently.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_role() -> AgentRole {
    AgentRole::Planner
}

fn default_schema_version() -> u32 {
    1
}

impl Default for RoleMemory {
    fn default() -> Self {
        Self {
            role: AgentRole::Planner,
            entries: Vec::new(),
            schema_version: 1,
        }
    }
}

/// Load role memory from `.niki/memory/{role}.json`. Returns empty memory if file doesn't exist.
pub fn load_memory(project_dir: &Path, role: AgentRole) -> RoleMemory {
    let path = memory_path(project_dir, role);
    if !path.exists() {
        return RoleMemory {
            role,
            entries: Vec::new(),
            schema_version: 1,
        };
    }
    let text = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", path.display());
            return RoleMemory {
                role,
                entries: Vec::new(),
                schema_version: 1,
            };
        }
    };
    match serde_json::from_str::<RoleMemory>(&text) {
        Ok(mem) => {
            if mem.schema_version != 1 {
                eprintln!(
                    "Warning: {} schema_version={} (expected 1); reading best-effort",
                    path.display(),
                    mem.schema_version
                );
                tracing::warn!(
                    target: "niki::memory",
                    path = %path.display(),
                    version = mem.schema_version,
                    "memory schema version mismatch"
                );
            }
            mem
        }
        Err(e) => {
            eprintln!(
                "Warning: {} unparsable ({}); starting empty instead of failing",
                path.display(),
                e
            );
            RoleMemory {
                role,
                entries: Vec::new(),
                schema_version: 1,
            }
        }
    }
}

/// Save role memory to `.niki/memory/{role}.json` via atomic temp+rename.
pub fn save_memory(project_dir: &Path, memory: &RoleMemory) -> Result<()> {
    let path = memory_path(project_dir, memory.role);
    let mut owned = memory.clone();
    owned.schema_version = 1;
    let json = serde_json::to_string_pretty(&owned)?;
    crate::knowledge::kb::write_atomic(&path, json.as_bytes())?;
    Ok(())
}

/// Append a new entry to a role's memory and save.
///
/// Phase 4.3: dedupe by content hash (task + tags + content) before append so
/// three identical Approved runs produce at most one entry. Value-aware
/// retention: entries referenced by retrieval survive pressure that evicts
/// unused noise. The 100-entry bound stays configurable via the constant below.
pub fn append_memory(
    project_dir: &Path,
    role: AgentRole,
    task: &str,
    tags: Vec<String>,
    content: String,
    branch: Option<String>,
) -> Result<()> {
    let hash = memory_content_hash(task, &tags, &content);
    let mut memory = load_memory(project_dir, role);

    // Dedupe: if an entry with the same hash already exists, bump its
    // timestamp instead of appending a duplicate.
    if let Some(existing) = memory.entries.iter_mut().find(|e| e.content_hash == hash) {
        existing.timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        if let Some(b) = &branch {
            existing.branch = Some(b.clone());
        }
        return save_memory(project_dir, &memory);
    }

    memory.entries.push(MemoryEntry {
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        task: task.to_string(),
        tags,
        content,
        branch,
        content_hash: hash,
        use_count: 0,
        last_used: None,
    });

    // Value-aware retention: keep entries referenced by retrieval (use_count > 0)
    // and recent successes, then evict oldest-first within tiers.
    if memory.entries.len() > MAX_MEMORY_ENTRIES {
        // Sort by (use_count desc, timestamp desc) — used entries first.
        memory.entries.sort_by(|a, b| {
            b.use_count
                .cmp(&a.use_count)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        memory.entries.truncate(MAX_MEMORY_ENTRIES);
    }

    save_memory(project_dir, &memory)
}

/// Record that retrieval injected this entry into a prompt (bumps use_count).
///
/// Matches on task + content (not the content hash): callers that render
/// entries generally do not retain the original tag list, and hashing with an
/// empty tag list would never match an entry created with tags — silently
/// defeating value-aware retention.
pub fn record_memory_use(
    project_dir: &Path,
    role: AgentRole,
    task: &str,
    content: &str,
) -> Result<()> {
    let mut memory = load_memory(project_dir, role);
    let mut changed = false;
    for entry in &mut memory.entries {
        if entry.task == task && entry.content == content {
            entry.use_count = entry.use_count.saturating_add(1);
            entry.last_used = Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            changed = true;
            break;
        }
    }
    if changed {
        save_memory(project_dir, &memory)?;
    }
    Ok(())
}

const MAX_MEMORY_ENTRIES: usize = 100;

/// Render memory entries as a string suitable for injection into prompts.
/// Returns empty string if no memory exists.
///
/// Phase 4.3: each entry rendered here is marked as retrieval-used so
/// value-aware retention keeps it under pressure.
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
    // Record retrieval use for the rendered entries (best-effort, never fails the run).
    for entry in memory.entries.iter().rev().take(max_entries) {
        let _ = record_memory_use(project_dir, role, &entry.task, &entry.content);
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
        AgentRole::Critic,
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
        AgentRole::Critic,
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
        AgentRole::Critic => "critic",
    };
    project_dir
        .join(".niki")
        .join("memory")
        .join(format!("{}.json", role_name))
}

// ── Phase 15: Hierarchical memory (user + team) ──────────────────────────────

/// Load user-level memory from `.niki/memory/user.json`.
pub fn load_user_memory(project_dir: &Path) -> Vec<MemoryEntry> {
    let path = project_dir.join(".niki").join("memory").join("user.json");
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str::<Vec<MemoryEntry>>(&s).unwrap_or_else(|e| {
            eprintln!(
                "Warning: {} unparsable ({}); starting empty",
                path.display(),
                e
            );
            Vec::new()
        }),
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Save user-level memory to `.niki/memory/user.json` via atomic temp+rename.
pub fn save_user_memory(project_dir: &Path, entries: &[MemoryEntry]) -> Result<()> {
    let path = project_dir.join(".niki").join("memory").join("user.json");
    let json = serde_json::to_string_pretty(entries)?;
    crate::knowledge::kb::write_atomic(&path, json.as_bytes())?;
    Ok(())
}

/// Append a user memory entry.
pub fn append_user_memory(project_dir: &Path, task: &str, content: String) -> Result<()> {
    let mut entries = load_user_memory(project_dir);
    let hash = memory_content_hash(task, &["user".to_string()], &content);
    if let Some(existing) = entries.iter_mut().find(|e| e.content_hash == hash) {
        existing.timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        return save_user_memory(project_dir, &entries);
    }
    entries.push(MemoryEntry {
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        task: task.to_string(),
        tags: vec!["user".to_string()],
        content,
        branch: None,
        content_hash: hash,
        use_count: 0,
        last_used: None,
    });
    // Keep bounded
    if entries.len() > MAX_MEMORY_ENTRIES {
        entries.drain(..entries.len() - MAX_MEMORY_ENTRIES);
    }
    save_user_memory(project_dir, &entries)
}

/// Load team-level memory from `.niki/memory/team.json`.
pub fn load_team_memory(project_dir: &Path) -> Vec<MemoryEntry> {
    let path = project_dir.join(".niki").join("memory").join("team.json");
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str::<Vec<MemoryEntry>>(&s).unwrap_or_else(|e| {
            eprintln!(
                "Warning: {} unparsable ({}); starting empty",
                path.display(),
                e
            );
            Vec::new()
        }),
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Save team-level memory to `.niki/memory/team.json` via atomic temp+rename.
pub fn save_team_memory(project_dir: &Path, entries: &[MemoryEntry]) -> Result<()> {
    let path = project_dir.join(".niki").join("memory").join("team.json");
    let json = serde_json::to_string_pretty(entries)?;
    crate::knowledge::kb::write_atomic(&path, json.as_bytes())?;
    Ok(())
}

/// Append a team memory entry.
pub fn append_team_memory(project_dir: &Path, task: &str, content: String) -> Result<()> {
    let mut entries = load_team_memory(project_dir);
    let hash = memory_content_hash(task, &["team".to_string()], &content);
    if let Some(existing) = entries.iter_mut().find(|e| e.content_hash == hash) {
        existing.timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        return save_team_memory(project_dir, &entries);
    }
    entries.push(MemoryEntry {
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        task: task.to_string(),
        tags: vec!["team".to_string()],
        content,
        branch: None,
        content_hash: hash,
        use_count: 0,
        last_used: None,
    });
    if entries.len() > MAX_MEMORY_ENTRIES {
        entries.drain(..entries.len() - MAX_MEMORY_ENTRIES);
    }
    save_team_memory(project_dir, &entries)
}

/// Render hierarchical memory for prompt injection (user > team > project role).
pub fn render_hierarchical_memory(
    project_dir: &Path,
    role: AgentRole,
    max_entries: usize,
) -> String {
    let mut parts = Vec::new();

    // User memory (highest precedence)
    let user = load_user_memory(project_dir);
    if !user.is_empty() {
        let mut out = String::from("## User Memory\n\n");
        for entry in user.iter().rev().take(max_entries) {
            out.push_str(&format!(
                "- [{}] {}: {}\n",
                &entry.timestamp[..10],
                entry.task.chars().take(50).collect::<String>(),
                entry.content.chars().take(200).collect::<String>(),
            ));
        }
        parts.push(out);
    }

    // Team memory
    let team = load_team_memory(project_dir);
    if !team.is_empty() {
        let mut out = String::from("## Team Memory\n\n");
        for entry in team.iter().rev().take(max_entries) {
            out.push_str(&format!(
                "- [{}] {}: {}\n",
                &entry.timestamp[..10],
                entry.task.chars().take(50).collect::<String>(),
                entry.content.chars().take(200).collect::<String>(),
            ));
        }
        parts.push(out);
    }

    // Project role memory
    let project = render_memory_for_prompt(project_dir, role, max_entries);
    if !project.is_empty() {
        parts.push(project);
    }

    parts.join("\n")
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
        )
        .unwrap();

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
    fn memory_dedupes_identical_entries() {
        // Phase 4.3: three identical Approved runs produce at most one entry.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        for _ in 0..3 {
            append_memory(
                dir,
                AgentRole::Coder,
                "fix null check",
                vec!["error-pattern".into()],
                "Always check Option before unwrap".to_string(),
                Some("main".into()),
            )
            .unwrap();
        }
        let mem = load_memory(dir, AgentRole::Coder);
        assert_eq!(
            mem.entries.len(),
            1,
            "three identical appends must dedupe to one entry"
        );
        assert_eq!(mem.entries[0].use_count, 0);
    }

    #[test]
    fn memory_distinct_entries_are_kept() {
        // Phase 4.3: different content must not dedupe.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        append_memory(
            dir,
            AgentRole::Coder,
            "task a",
            vec![],
            "content a".to_string(),
            None,
        )
        .unwrap();
        append_memory(
            dir,
            AgentRole::Coder,
            "task b",
            vec![],
            "content b".to_string(),
            None,
        )
        .unwrap();
        let mem = load_memory(dir, AgentRole::Coder);
        assert_eq!(mem.entries.len(), 2, "distinct entries must be kept");
    }

    #[test]
    fn memory_value_aware_retention_keeps_used_entries() {
        // Phase 4.3: a retrieval-used entry survives pressure that evicts
        // unused noise.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Fill memory to the cap with noise.
        for i in 0..MAX_MEMORY_ENTRIES {
            append_memory(
                dir,
                AgentRole::Tester,
                &format!("noise {}", i),
                vec![],
                format!("content {}", i),
                None,
            )
            .unwrap();
        }
        assert_eq!(
            load_memory(dir, AgentRole::Tester).entries.len(),
            MAX_MEMORY_ENTRIES
        );

        // Mark one entry as retrieval-used.
        let used_hash = memory_content_hash("noise 0", &[], "content 0");
        {
            let mut mem = load_memory(dir, AgentRole::Tester);
            for e in &mut mem.entries {
                if e.content_hash == used_hash {
                    e.use_count = 5;
                    break;
                }
            }
            save_memory(dir, &mem).unwrap();
        }

        // Append one more — eviction must keep the used entry.
        append_memory(
            dir,
            AgentRole::Tester,
            "new task",
            vec![],
            "new content".to_string(),
            None,
        )
        .unwrap();
        let mem = load_memory(dir, AgentRole::Tester);
        assert_eq!(mem.entries.len(), MAX_MEMORY_ENTRIES);
        let kept = mem.entries.iter().any(|e| e.content_hash == used_hash);
        assert!(
            kept,
            "retrieval-used entry must survive eviction over unused noise"
        );
    }

    #[test]
    fn memory_sparse_fixture_has_hash_default() {
        // Phase 4.3: a fixture missing content_hash must parse with empty hash.
        let entry: MemoryEntry = serde_json::from_str(r#"{"task":"t"}"#).unwrap();
        assert_eq!(entry.task, "t");
        assert!(entry.content_hash.is_empty());
        assert_eq!(entry.use_count, 0);
    }

    #[test]
    fn memory_content_hash_is_stable() {
        // Phase 4.3: same inputs → same hash; different inputs → different hash.
        let h1 = memory_content_hash("t", &["a".into()], "c");
        let h2 = memory_content_hash("t", &["a".into()], "c");
        assert_eq!(h1, h2, "hash must be deterministic");
        let h3 = memory_content_hash("t", &["b".into()], "c");
        assert_ne!(h1, h3, "different tags must produce different hashes");
    }

    #[test]
    fn retrieval_render_bumps_use_count_for_tagged_entries() {
        // Phase 4.3 regression: entries created WITH tags must still have
        // their use_count bumped when rendered (matches on task + content,
        // not on a hash computed with an empty tag list).
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        append_memory(
            dir,
            AgentRole::Coder,
            "fix null check",
            vec!["error-pattern".into()],
            "Always check Option before unwrap".to_string(),
            None,
        )
        .unwrap();
        assert_eq!(load_memory(dir, AgentRole::Coder).entries[0].use_count, 0);
        let rendered = render_memory_for_prompt(dir, AgentRole::Coder, 10);
        assert!(rendered.contains("fix null check"));
        assert_eq!(
            load_memory(dir, AgentRole::Coder).entries[0].use_count,
            1,
            "rendering must record retrieval use even for tagged entries"
        );
    }

    #[test]
    fn all_tags_collected() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        append_memory(
            dir,
            AgentRole::Planner,
            "t",
            vec!["convention".into()],
            "c".to_string(),
            None,
        )
        .unwrap();
        append_memory(
            dir,
            AgentRole::Coder,
            "t",
            vec!["error-pattern".into()],
            "c".to_string(),
            None,
        )
        .unwrap();
        let tags = get_all_tags(dir);
        assert_eq!(tags, vec!["convention", "error-pattern"]);
    }

    #[test]
    fn sparse_fixture_parses_with_defaults() {
        // Phase 2.1: a fixture missing every optional field must parse.
        let entry: MemoryEntry = serde_json::from_str(r#"{"task":"t"}"#).unwrap();
        assert_eq!(entry.task, "t");
        assert!(entry.tags.is_empty());
        assert!(entry.content.is_empty());
        let role_mem: RoleMemory = serde_json::from_str(r#"{"entries":[]}"#).unwrap();
        assert!(role_mem.entries.is_empty());
        assert_eq!(role_mem.schema_version, 1);
    }

    #[test]
    fn atomic_save_leaves_no_temp_file() {
        // Phase 2.1: atomic writes never leave temp files or corrupt JSON.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        append_memory(dir, AgentRole::Coder, "t", vec![], "c".to_string(), None).unwrap();
        let mem_dir = dir.join(".niki").join("memory");
        let strays: Vec<_> = std::fs::read_dir(&mem_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .contains("tmp-niki-atomic")
            })
            .collect();
        assert!(strays.is_empty(), "temp file left behind: {strays:?}");
        let loaded = load_memory(dir, AgentRole::Coder);
        assert_eq!(loaded.entries.len(), 1);
    }

    #[test]
    fn corrupt_file_warns_and_starts_empty() {
        // Phase 2.1: corrupt JSON warns loudly instead of failing the run.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let path = memory_path(dir, AgentRole::Coder);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        let loaded = load_memory(dir, AgentRole::Coder);
        assert!(loaded.entries.is_empty());
    }
}
