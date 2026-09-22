//! Converged query store (Phase 4.1/4.2).
//!
//! One local-first query surface answering "all learnings + memory + run
//! records for task X". The durable writers (learnings JSONL, role-memory
//! JSON, task records) remain the source of truth; `store/index.json` is a
//! rebuildable derived cache guarded by a versioned manifest
//! (`store/manifest.json`). Deleting `<output_dir>/store/` is always safe —
//! the next query rebuilds it from the sources.
//!
//! Design notes (see `docs/decisions/adr-002-converged-store-v1.md`):
//! - File-backed, zero new dependencies: no SQLite/native extension, so the
//!   `deny.toml` gate and `cargo dist` builds are untouched, and the default
//!   path stays offline with no network dependency.
//! - Retrieval ranks by hybrid score (keyword overlap + trigram-vector
//!   cosine + recency + authority tier) with a deterministic keyword
//!   fallback when the index is absent (live scan of the same sources).
//! - Index size honors `[repo_intel] disk_budget_mb` (previously unread): the
//!   serialized index is capped, oldest docs dropped first.
//! - Atomicity boundary: each source file write is atomic (temp + rename);
//!   the index itself is written atomically. There is no cross-file
//!   transaction — documented, not ACID.

use crate::config::NikiConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Store schema version. Bumped only with a migration that rebuilds the index.
pub const STORE_SCHEMA_VERSION: u32 = 1;

/// Root of the converged store, honoring `general.output_dir`.
pub fn store_root(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path.join(&config.general.output_dir).join("store")
}

/// Version/migration record for the store. This file is the version table:
/// a mismatch means the index was built by another schema and must be rebuilt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreManifest {
    #[serde(default = "store_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub built_at: String,
    #[serde(default)]
    pub doc_count: usize,
    /// Source files the index was built from (for debugging, not gating).
    #[serde(default)]
    pub sources: Vec<String>,
}

fn store_schema_version() -> u32 {
    STORE_SCHEMA_VERSION
}

/// What kind of record a stored document came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoredKind {
    Learning,
    Memory,
    Run,
}

/// One indexed record: a learning, a memory entry, or a run record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDoc {
    /// Stable id (`<kind>:<source>:<seq>`).
    pub id: String,
    pub kind: StoredKind,
    /// Owning task description (learnings/memory) or run description.
    #[serde(default)]
    pub task: String,
    /// Searchable content (learning details, memory content, run summary).
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Provenance tier: `authoritative`, `inferred`, or `advisory`.
    #[serde(default)]
    pub authority: String,
    /// RFC3339 timestamp, empty when unknown (sorts oldest).
    #[serde(default)]
    pub created_at: String,
    /// Originating file, relative to the project when possible.
    #[serde(default)]
    pub source: String,
}

/// The derived index. Rebuildable at any time via [`build_index`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreIndex {
    #[serde(default = "store_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub built_at: String,
    #[serde(default)]
    pub docs: Vec<StoredDoc>,
}

/// Hybrid retrieval score with per-signal components (all 0.0..=1.0).
#[derive(Debug, Clone)]
pub struct ScoredDoc {
    pub doc: StoredDoc,
    pub keyword: f64,
    pub vector: f64,
    pub recency: f64,
    pub authority: f64,
    pub total: f64,
}

/// Hybrid weights: keyword overlap leads, trigram-vector paraphrase second,
/// recency and authority break ties. Documented so evals can reason about it.
const W_KEYWORD: f64 = 0.45;
const W_VECTOR: f64 = 0.35;
const W_RECENCY: f64 = 0.10;
const W_AUTHORITY: f64 = 0.10;

/// Small stoplist so keyword matching keys on content words.
/// Mirrors `context_pack::STOPWORDS` without coupling the modules.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "into", "add", "new", "use", "using",
    "should", "could", "would", "when", "where", "what", "which", "have", "has", "are", "was",
    "were", "been", "also", "such", "than", "then", "them", "they", "our", "your", "its",
];

/// Split text into content keywords (lowercase, len ≥ 3, deduplicated).
pub fn tokenize(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(|c: char| !c.is_alphanumeric())
        .filter_map(|w| {
            let w = w.to_lowercase();
            if w.len() >= 3 && !STOPWORDS.contains(&w.as_str()) && seen.insert(w.clone()) {
                Some(w)
            } else {
                None
            }
        })
        .collect()
}

/// Character 3-gram TF vector over normalized text. Pure-Rust paraphrase
/// signal: related words share trigrams (`auth`/`authentication`) even with
/// zero shared keyword tokens.
fn trigrams(text: &str) -> HashMap<String, u32> {
    let norm: String = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let chars: Vec<char> = norm.chars().collect();
    let mut out = HashMap::new();
    if chars.len() < 3 {
        if !norm.trim().is_empty() {
            *out.entry(norm.trim().to_string()).or_insert(0) += 1;
        }
        return out;
    }
    for w in chars.windows(3) {
        let g: String = w.iter().collect();
        if g.trim().is_empty() {
            continue;
        }
        *out.entry(g).or_insert(0) += 1;
    }
    out
}

fn cosine(a: &HashMap<String, u32>, b: &HashMap<String, u32>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut dot = 0u64;
    for (k, va) in a {
        if let Some(vb) = b.get(k) {
            dot += (*va as u64) * (*vb as u64);
        }
    }
    let norm = |m: &HashMap<String, u32>| {
        m.values()
            .map(|v| (*v as f64) * (*v as f64))
            .sum::<f64>()
            .sqrt()
    };
    let denom = norm(a) * norm(b);
    if denom == 0.0 {
        0.0
    } else {
        (dot as f64 / denom).clamp(0.0, 1.0)
    }
}

fn authority_weight(authority: &str) -> f64 {
    match authority {
        "authoritative" => 1.0,
        "inferred" => 0.6,
        "advisory" => 0.3,
        _ => 0.2,
    }
}

/// Rank documents for a task with the hybrid score. Pure and deterministic:
/// the same inputs always produce the same order (ties broken by id).
pub fn hybrid_rank(docs: &[StoredDoc], task: &str) -> Vec<ScoredDoc> {
    let keywords = tokenize(task);
    let task_vec = trigrams(task);
    // Recency baseline: newest created_at first; unparsable/empty sorts oldest.
    let mut order: Vec<usize> = (0..docs.len()).collect();
    order.sort_by(|&a, &b| {
        docs[b]
            .created_at
            .cmp(&docs[a].created_at)
            .then_with(|| docs[a].id.cmp(&docs[b].id))
    });
    let recency_of = |idx: usize| {
        if docs.len() <= 1 {
            return 1.0;
        }
        let pos = order.iter().position(|&i| i == idx).unwrap_or(docs.len());
        1.0 - (pos as f64 / docs.len() as f64)
    };
    let mut scored: Vec<ScoredDoc> = docs
        .iter()
        .enumerate()
        .map(|(idx, doc)| {
            let hay = format!("{} {} {}", doc.task, doc.text, doc.tags.join(" "));
            let hay_tokens: HashSet<String> = tokenize(&hay).into_iter().collect();
            let keyword = if keywords.is_empty() {
                0.0
            } else {
                keywords.iter().filter(|k| hay_tokens.contains(*k)).count() as f64
                    / keywords.len() as f64
            };
            let vector = cosine(&task_vec, &trigrams(&hay));
            let recency = recency_of(idx);
            let authority = authority_weight(&doc.authority);
            let total = W_KEYWORD * keyword
                + W_VECTOR * vector
                + W_RECENCY * recency
                + W_AUTHORITY * authority;
            ScoredDoc {
                doc: doc.clone(),
                keyword,
                vector,
                recency,
                authority,
                total,
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.total
            .partial_cmp(&a.total)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.doc.id.cmp(&b.doc.id))
    });
    scored
}

/// Scan every source into documents. Best-effort per source: one unreadable
/// file never aborts the scan.
fn scan_all(project_path: &Path, config: &NikiConfig) -> (Vec<StoredDoc>, Vec<String>) {
    let mut docs = Vec::new();
    let mut sources = Vec::new();

    // 1. Learnings (JSONL, newest kept by readers; index keeps all).
    let learnings = crate::knowledge::learnings::tail_learnings(project_path, config, usize::MAX);
    if !learnings.is_empty() {
        sources.push(
            crate::knowledge::learnings::learnings_path(project_path, config)
                .display()
                .to_string(),
        );
    }
    for (i, l) in learnings.iter().enumerate() {
        docs.push(StoredDoc {
            id: format!("learning:{i}"),
            kind: StoredKind::Learning,
            task: l.task_id.clone().unwrap_or_default(),
            text: l.details.clone(),
            tags: vec![l.kind.clone(), l.author_role.clone()],
            authority: if l.authority.is_empty() {
                "advisory".to_string()
            } else {
                l.authority.clone()
            },
            created_at: l
                .created_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            source: "learnings.jsonl".to_string(),
        });
    }

    // 2. Role memory across all roles.
    for role in [
        crate::artifacts::types::AgentRole::Planner,
        crate::artifacts::types::AgentRole::Coder,
        crate::artifacts::types::AgentRole::Tester,
        crate::artifacts::types::AgentRole::Reviewer,
        crate::artifacts::types::AgentRole::SecurityAuditor,
        crate::artifacts::types::AgentRole::Red,
        crate::artifacts::types::AgentRole::Critic,
        crate::artifacts::types::AgentRole::Synthesizer,
    ] {
        let mem = crate::memory::load_memory(project_path, role);
        if mem.entries.is_empty() {
            continue;
        }
        sources.push(format!("memory/{role:?}.json"));
        for (i, e) in mem.entries.iter().enumerate() {
            docs.push(StoredDoc {
                id: format!("memory:{role:?}:{i}"),
                kind: StoredKind::Memory,
                task: e.task.clone(),
                text: e.content.clone(),
                tags: e.tags.clone(),
                authority: "advisory".to_string(),
                created_at: e.timestamp.clone(),
                source: format!("memory/{role:?}.json"),
            });
        }
    }

    // 3. Run records (`<output_dir>/tasks/*/task.json`).
    let tasks_dir = project_path.join(&config.general.output_dir).join("tasks");
    if let Ok(rd) = std::fs::read_dir(&tasks_dir) {
        for entry in rd.flatten() {
            let path = entry.path().join("task.json");
            if !path.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(rec) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let verdict = rec
                .get("verdict")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = rec.get("status").map(|s| s.to_string()).unwrap_or_default();
            let desc = rec
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            docs.push(StoredDoc {
                id: format!("run:{}", entry.file_name().to_string_lossy()),
                kind: StoredKind::Run,
                task: desc.clone(),
                text: format!("run {status} verdict {verdict}: {desc}"),
                tags: vec![verdict.clone()],
                authority: "authoritative".to_string(),
                created_at: rec
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                source: path.display().to_string(),
            });
            sources.push(path.display().to_string());
        }
    }

    // Newest first so size capping drops the oldest.
    docs.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    (docs, sources)
}

fn index_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    store_root(project_path, config).join("index.json")
}

fn manifest_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    store_root(project_path, config).join("manifest.json")
}

/// Build (or rebuild) the derived index from the current sources and persist
/// it atomically with its manifest. Honors `[repo_intel] disk_budget_mb`.
pub fn build_index(project_path: &Path, config: &NikiConfig) -> anyhow::Result<StoreIndex> {
    let budget_bytes = config.repo_intel.disk_budget_mb.max(1) * 1024 * 1024;
    let index = build_index_with_budget(project_path, config, budget_bytes as usize)?;
    let root = store_root(project_path, config);
    std::fs::create_dir_all(&root)?;
    let json = serde_json::to_string(&index)?;
    crate::knowledge::kb::write_atomic(&index_path(project_path, config), json.as_bytes())?;
    let manifest = StoreManifest {
        schema_version: STORE_SCHEMA_VERSION,
        built_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        doc_count: index.docs.len(),
        sources: scan_sources(project_path, config),
    };
    let mjson = serde_json::to_string_pretty(&manifest)?;
    crate::knowledge::kb::write_atomic(&manifest_path(project_path, config), mjson.as_bytes())?;
    Ok(index)
}

fn scan_sources(project_path: &Path, config: &NikiConfig) -> Vec<String> {
    scan_all(project_path, config).1
}

fn build_index_with_budget(
    project_path: &Path,
    config: &NikiConfig,
    budget_bytes: usize,
) -> anyhow::Result<StoreIndex> {
    let (mut docs, _sources) = scan_all(project_path, config);
    // Cap the serialized index: drop oldest docs until it fits (or one remains).
    while docs.len() > 1 {
        let probe = StoreIndex {
            schema_version: STORE_SCHEMA_VERSION,
            built_at: String::new(),
            docs: docs.clone(),
        };
        if serde_json::to_string(&probe)?.len() <= budget_bytes {
            break;
        }
        docs.pop();
    }
    Ok(StoreIndex {
        schema_version: STORE_SCHEMA_VERSION,
        built_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        docs,
    })
}

/// Load the persisted index when it exists and matches this schema version.
pub fn load_index(project_path: &Path, config: &NikiConfig) -> Option<StoreIndex> {
    let manifest: StoreManifest =
        serde_json::from_str(&std::fs::read_to_string(manifest_path(project_path, config)).ok()?)
            .ok()?;
    if manifest.schema_version != STORE_SCHEMA_VERSION {
        eprintln!(
            "Warning: store manifest schema_version={} (expected {}); ignoring cached index",
            manifest.schema_version, STORE_SCHEMA_VERSION
        );
        return None;
    }
    let index: StoreIndex =
        serde_json::from_str(&std::fs::read_to_string(index_path(project_path, config)).ok()?)
            .ok()?;
    if index.schema_version != STORE_SCHEMA_VERSION {
        return None;
    }
    Some(index)
}

/// One query API for "all learnings + memory + runs for task X".
/// Uses the persisted index when present; otherwise live-scans the same
/// sources with the same ranking (deterministic fallback, no behavior cliff).
pub fn query_store(
    project_path: &Path,
    config: &NikiConfig,
    task: &str,
    limit: usize,
) -> Vec<ScoredDoc> {
    let docs = match load_index(project_path, config) {
        Some(index) => index.docs,
        None => scan_all(project_path, config).0,
    };
    hybrid_rank(&docs, task).into_iter().take(limit).collect()
}

/// Ranked learnings for a task (the 4.5 retrieval path for `context_pack`).
/// Returns owned learning entries, most relevant first, capped at `limit`.
pub fn query_learnings(
    project_path: &Path,
    config: &NikiConfig,
    task: &str,
    limit: usize,
) -> Vec<crate::knowledge::learnings::LearningEntry> {
    let learnings = crate::knowledge::learnings::tail_learnings(project_path, config, usize::MAX);
    if learnings.is_empty() {
        return Vec::new();
    }
    let docs: Vec<StoredDoc> = learnings
        .iter()
        .enumerate()
        .map(|(i, l)| StoredDoc {
            id: format!("learning:{i}"),
            kind: StoredKind::Learning,
            task: l.task_id.clone().unwrap_or_default(),
            text: l.details.clone(),
            tags: vec![l.kind.clone(), l.author_role.clone()],
            authority: if l.authority.is_empty() {
                "advisory".to_string()
            } else {
                l.authority.clone()
            },
            created_at: l
                .created_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            source: "learnings.jsonl".to_string(),
        })
        .collect();
    hybrid_rank(&docs, task)
        .into_iter()
        .take(limit)
        .filter_map(|s| {
            s.doc
                .id
                .strip_prefix("learning:")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|i| learnings.get(i).cloned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::learnings::{LearningEntry, append_learning};

    fn test_config() -> NikiConfig {
        NikiConfig::default()
    }

    fn seed_learning(dir: &Path, details: &str) {
        append_learning(
            dir,
            &test_config(),
            &LearningEntry::new(
                "history",
                "niki-task-test",
                "history-miner",
                "inferred",
                details.to_string(),
            ),
        )
        .unwrap();
    }

    #[test]
    fn store_migrate_builds_index_from_existing_layout() {
        // Phase 4.1: existing `.niki/` fixtures migrate into one queryable index.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        seed_learning(dir, "login retry fix in auth");
        crate::memory::append_memory(
            dir,
            crate::artifacts::types::AgentRole::Coder,
            "fix null check",
            vec!["error-pattern".into()],
            "Always check Option before unwrap".to_string(),
            None,
        )
        .unwrap();

        let index = build_index(dir, &config).unwrap();
        assert!(index.docs.len() >= 2, "learnings + memory indexed");
        assert_eq!(index.schema_version, STORE_SCHEMA_VERSION);

        // Manifest is the version table; reload roundtrips.
        let loaded = load_index(dir, &config).expect("persisted index reloads");
        assert_eq!(loaded.docs.len(), index.docs.len());

        // One query API answers across kinds.
        let hits = query_store(dir, &config, "fix login retry", 5);
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.doc.kind == StoredKind::Learning));
    }

    #[test]
    fn retrieval_semantic_paraphrase_beats_keyword_only() {
        // Phase 4.2: a paraphrased task with NO shared keywords still surfaces
        // the relevant learning via the trigram-vector signal.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        seed_learning(dir, "login retry fix in auth module");
        seed_learning(dir, "unrelated deploy pipeline note for staging servers");

        let task = "signin authentication reattempt logic";
        // Prove the premise: zero keyword overlap with the relevant learning.
        let task_kw: HashSet<String> = tokenize(task).into_iter().collect();
        let rel_kw: HashSet<String> = tokenize("login retry fix in auth module")
            .into_iter()
            .collect();
        assert!(
            task_kw.intersection(&rel_kw).next().is_none(),
            "premise: no shared keywords, keyword score must be 0"
        );

        build_index(dir, &config).unwrap();
        let hits = query_store(dir, &config, task, 5);
        assert!(!hits.is_empty());
        let top = &hits[0];
        assert!(
            top.doc.text.contains("login retry"),
            "paraphrase must surface the relevant learning, got: {}",
            top.doc.text
        );
        assert_eq!(top.keyword, 0.0, "keyword signal is 0 by construction");
        assert!(top.vector > 0.0, "vector signal must carry the match");
    }

    #[test]
    fn retrieval_fallback_without_index_matches_live_scan() {
        // Phase 4.2: absent index degrades to the same ranking, not an error.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        seed_learning(dir, "login retry fix in auth");

        // No build_index call: store/ does not exist.
        assert!(!store_root(dir, &config).exists());
        let hits = query_store(dir, &config, "fix login retry", 3);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].doc.text.contains("login retry"));
    }

    #[test]
    fn disk_budget_caps_index_oldest_first() {
        // Phase 4.2 guard: `disk_budget_mb` semantics bound the index size.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        for i in 0..10 {
            seed_learning(
                dir,
                &format!("learning number {i} with padding content xxxxxxxxxx"),
            );
        }
        let tiny = build_index_with_budget(dir, &config, 300).unwrap();
        assert!(tiny.docs.len() < 10, "tiny budget must evict");
        assert!(!tiny.docs.is_empty(), "at least one doc always remains");
        // Newest-first order kept: remaining docs are the newest.
        let full = build_index_with_budget(dir, &config, usize::MAX).unwrap();
        assert_eq!(full.docs.len(), 10);
        assert_eq!(tiny.docs[0].id, full.docs[0].id);
    }

    #[test]
    fn version_mismatch_ignores_stale_index() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        seed_learning(dir, "some learning");
        build_index(dir, &config).unwrap();
        // Corrupt the manifest version: loader must ignore the cache...
        let bad = StoreManifest {
            schema_version: STORE_SCHEMA_VERSION + 1,
            built_at: String::new(),
            doc_count: 1,
            sources: vec![],
        };
        crate::knowledge::kb::write_atomic(
            &manifest_path(dir, &config),
            serde_json::to_string_pretty(&bad).unwrap().as_bytes(),
        )
        .unwrap();
        assert!(load_index(dir, &config).is_none());
        // ...while queries still work via live-scan fallback.
        assert_eq!(query_store(dir, &config, "learning", 3).len(), 1);
    }

    #[test]
    fn query_learnings_ranks_relevant_first() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let config = test_config();
        seed_learning(dir, "unrelated deploy pipeline note");
        seed_learning(dir, "login retry fix in auth");
        let ranked = query_learnings(dir, &config, "fix login retry", 3);
        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].details.contains("login retry"));
    }

    #[test]
    fn hybrid_rank_is_deterministic() {
        let docs = vec![
            StoredDoc {
                id: "b".into(),
                kind: StoredKind::Learning,
                task: String::new(),
                text: "alpha beta".into(),
                tags: vec![],
                authority: "advisory".into(),
                created_at: String::new(),
                source: String::new(),
            },
            StoredDoc {
                id: "a".into(),
                kind: StoredKind::Learning,
                task: String::new(),
                text: "alpha beta".into(),
                tags: vec![],
                authority: "advisory".into(),
                created_at: String::new(),
                source: String::new(),
            },
        ];
        let once: Vec<String> = hybrid_rank(&docs, "alpha")
            .iter()
            .map(|s| s.doc.id.clone())
            .collect();
        let twice: Vec<String> = hybrid_rank(&docs, "alpha")
            .iter()
            .map(|s| s.doc.id.clone())
            .collect();
        assert_eq!(once, twice, "identical inputs must rank identically");
    }
}
