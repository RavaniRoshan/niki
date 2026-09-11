//! Deterministic git-history miner with a cache-as-truth design.
//!
//! Every run rebuilds `<output_dir>/history/learnings.jsonl` from the
//! `<output_dir>/history/cache.jsonl` entries, so the derived file can never
//! desync from the cache. A rewrite detector (`_analyzed_head` in
//! `state.json`) busts the cache when history was rewritten (force-push,
//! rebase): the old head no longer resolves or is no longer an ancestor of
//! `HEAD`, so the cache is wiped and a `history` invalidation learning is
//! recorded. Huge diffs and non-code paths are skipped before analysis.

use crate::config::NikiConfig;
use crate::knowledge::learnings::LearningEntry;
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Max commits walked per run (newest first).
const MAX_COMMITS: usize = 300;
/// Diffs larger than this (insertions + deletions) are skipped as noise.
const MAX_DIFF_LOC: usize = 3000;
/// File extensions treated as code for history purposes.
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "py", "go", "java", "rb", "php", "c", "h", "cpp", "hpp",
    "cc", "cs", "swift", "kt", "sh", "toml", "json", "yaml", "yml",
];
/// Message keywords that mark a commit as worth learning from.
const BUG_KEYWORDS: &[&str] = &[
    "fix", "bug", "vuln", "cve-", "security", "panic", "leak", "unsafe", "overflow", "deadlock",
    "race", "exploit", "patch", "regress", "fail", "correct",
];

/// One cached per-commit analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryCacheEntry {
    pub commit_sha: String,
    pub analyzed_at: chrono::DateTime<Utc>,
    pub snapshot_id: String,
    /// Dominant stack of the diff (`Rust`, `TypeScript`, …), if determinable.
    #[serde(default)]
    pub stack: Option<String>,
    /// The derived learning, or `None` when the commit was noise.
    #[serde(default)]
    pub entry: Option<LearningEntry>,
}

/// Miner cache state (`state.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryState {
    /// HEAD at the time the cache was last written.
    #[serde(default)]
    pub analyzed_head: Option<String>,
    /// `ready`, `unsupported` (not a git repo), or `invalidated`.
    #[serde(default = "default_history_status")]
    pub status: String,
}

fn default_history_status() -> String {
    "ready".to_string()
}

/// Outcome of one [`mine_history`] call, for CLI display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryOutcome {
    pub analyzed: usize,
    pub learned: usize,
    pub skipped_large: usize,
    pub invalidated: bool,
    pub unsupported: bool,
}

fn history_dir(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path
        .join(&config.general.output_dir)
        .join("history")
}

fn cache_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    history_dir(project_path, config).join("cache.jsonl")
}

fn state_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    history_dir(project_path, config).join("state.json")
}

fn derived_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    history_dir(project_path, config).join("learnings.jsonl")
}

/// Mine history into the cache and rebuild the derived learnings file.
/// Never fails the caller: errors degrade to an `unsupported` outcome with a
/// best-effort state marker.
pub fn mine_history(project_path: &Path, config: &NikiConfig, snapshot_id: &str) -> HistoryOutcome {
    match mine_inner(project_path, config, snapshot_id) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("Warning: history mining failed ({e}); continuing without history");
            HistoryOutcome {
                analyzed: 0,
                learned: 0,
                skipped_large: 0,
                invalidated: false,
                unsupported: true,
            }
        }
    }
}

fn mine_inner(
    project_path: &Path,
    config: &NikiConfig,
    snapshot_id: &str,
) -> Result<HistoryOutcome> {
    let repo = match git2::Repository::open(project_path) {
        Ok(r) => r,
        Err(_) => {
            write_state(
                project_path,
                config,
                &HistoryState {
                    analyzed_head: None,
                    status: "unsupported".to_string(),
                },
            );
            return Ok(HistoryOutcome {
                analyzed: 0,
                learned: 0,
                skipped_large: 0,
                invalidated: false,
                unsupported: true,
            });
        }
    };
    let head_oid = match repo
        .revparse_single("HEAD")
        .ok()
        .and_then(|o| o.peel_to_commit().ok().map(|c| c.id()))
    {
        Some(oid) => oid,
        None => {
            // Empty repo (no commits yet): nothing to mine, but it IS a repo.
            write_state(
                project_path,
                config,
                &HistoryState {
                    analyzed_head: None,
                    status: "ready".to_string(),
                },
            );
            return Ok(HistoryOutcome {
                analyzed: 0,
                learned: 0,
                skipped_large: 0,
                invalidated: false,
                unsupported: false,
            });
        }
    };

    // Rewrite detector: the previously analyzed head must still resolve AND
    // be an ancestor of (or equal to) the current HEAD.
    let mut invalidated = false;
    let prior = read_state(project_path, config);
    if let Some(prev) = prior.analyzed_head.as_deref() {
        let prev_oid = repo
            .revparse_single(prev)
            .ok()
            .and_then(|o| o.peel_to_commit().ok().map(|c| c.id()));
        let still_valid = match prev_oid {
            Some(oid) if oid == head_oid => true,
            Some(oid) => repo
                .merge_base(oid, head_oid)
                .map(|base| base == oid)
                .unwrap_or(false),
            None => false,
        };
        if !still_valid {
            invalidated = true;
            // Wipe the cache: entries analyzed under rewritten history are
            // untrustworthy. Record the invalidation as a learning.
            let _ = std::fs::remove_file(cache_path(project_path, config));
            let _ = std::fs::remove_file(derived_path(project_path, config));
            crate::knowledge::learnings::append_learning(
                project_path,
                config,
                &LearningEntry::new(
                    "history",
                    snapshot_id,
                    "history-miner",
                    "authoritative",
                    format!(
                        "History rewritten: previously analyzed head {prev} is no longer reachable from HEAD; cache invalidated and rebuilt."
                    ),
                ),
            )?;
        }
    }

    let known: HashSet<String> = read_cache(project_path, config)
        .into_iter()
        .map(|e| e.commit_sha)
        .collect();

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    let mut analyzed = 0usize;
    let mut learned = 0usize;
    let mut skipped_large = 0usize;
    let mut new_entries = Vec::new();

    for oid in revwalk.take(MAX_COMMITS).flatten() {
        let sha = oid.to_string();
        if known.contains(&sha) {
            continue;
        }
        let commit = repo.find_commit(oid)?;
        let message = commit.summary().unwrap_or("").to_string();
        let stats = diff_stats(&repo, &commit);
        if stats.loc > MAX_DIFF_LOC {
            skipped_large += 1;
            continue;
        }
        analyzed += 1;
        let entry = if stats.code_files == 0 {
            // Test/README-only commits are noise: cached as analyzed, but they
            // never produce learnings.
            None
        } else {
            classify_commit(&message, &stats.stack, &sha, snapshot_id)
        };
        if entry.is_some() {
            learned += 1;
        }
        new_entries.push(HistoryCacheEntry {
            commit_sha: sha,
            analyzed_at: Utc::now(),
            snapshot_id: snapshot_id.to_string(),
            stack: stats.stack,
            entry,
        });
    }

    if !new_entries.is_empty() {
        append_cache(project_path, config, &new_entries)?;
    }
    // Cache as truth: rebuild the derived file from the FULL cache every run.
    rebuild_derived(project_path, config)?;
    write_state(
        project_path,
        config,
        &HistoryState {
            analyzed_head: Some(head_oid.to_string()),
            status: "ready".to_string(),
        },
    );

    Ok(HistoryOutcome {
        analyzed,
        learned,
        skipped_large,
        invalidated,
        unsupported: false,
    })
}

/// Per-commit diff summary: total churn, dominant stack, and how many
/// non-noise code files the diff touched.
struct CommitDiffStats {
    loc: usize,
    stack: Option<String>,
    code_files: usize,
}

/// (insertions + deletions, dominant stack) for a commit's diff to its first
/// parent. Root commits diff against the empty tree.
fn diff_stats(repo: &git2::Repository, commit: &git2::Commit) -> CommitDiffStats {
    let empty = CommitDiffStats {
        loc: 0,
        stack: None,
        code_files: 0,
    };
    let new_tree = commit.tree().ok();
    let old_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    let diff = match (old_tree, new_tree) {
        (old, Some(new)) => repo.diff_tree_to_tree(old.as_ref(), Some(&new), None).ok(),
        _ => None,
    };
    let diff = match diff {
        Some(d) => d,
        None => return empty,
    };
    let loc = diff
        .stats()
        .map(|s| s.insertions() + s.deletions())
        .unwrap_or(0);
    let mut stacks: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut code_files = 0usize;
    for delta in diff.deltas() {
        let path = delta.new_file().path().or_else(|| delta.old_file().path());
        let Some(path) = path else { continue };
        if is_noise_path(path) {
            continue;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && CODE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
        {
            code_files += 1;
            if let Some(stack) = stack_for(ext) {
                *stacks.entry(stack).or_insert(0) += 1;
            }
        }
    }
    let stack = stacks
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(s, _)| s.to_string());
    CommitDiffStats {
        loc,
        stack,
        code_files,
    }
}

fn stack_for(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "rs" => Some("Rust"),
        "ts" | "tsx" | "mts" => Some("TypeScript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "py" => Some("Python"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "rb" => Some("Ruby"),
        _ => None,
    }
}

fn is_noise_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    s.contains("node_modules")
        || s.contains("/target/")
        || s.starts_with("target/")
        || s.contains("/dist/")
        || s.contains("readme")
        || s.contains(".test.")
        || s.contains(".spec.")
        || s.ends_with("_test.rs")
        || s.ends_with("_test.go")
        || s.ends_with("_test.py")
}

/// Classify a commit message into an optional learning. Code-only signals:
/// test/README-only commits never produce entries.
fn classify_commit(
    message: &str,
    stack: &Option<String>,
    sha: &str,
    snapshot_id: &str,
) -> Option<LearningEntry> {
    let lower = message.to_lowercase();
    if !BUG_KEYWORDS.iter().any(|k| lower.contains(k)) {
        return None;
    }
    let stack_note = stack
        .as_deref()
        .map(|s| format!(" [{s}]"))
        .unwrap_or_default();
    let mut entry = LearningEntry::new(
        "history",
        snapshot_id,
        "history-miner",
        "inferred",
        format!("{message}{stack_note} ({})", &sha[..sha.len().min(7)]),
    );
    entry.task_id = None;
    Some(entry)
}

fn read_state(project_path: &Path, config: &NikiConfig) -> HistoryState {
    std::fs::read_to_string(state_path(project_path, config))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_state(project_path: &Path, config: &NikiConfig, state: &HistoryState) {
    let path = state_path(project_path, config);
    if path
        .parent()
        .is_some_and(|p| std::fs::create_dir_all(p).is_err())
    {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = crate::knowledge::kb::write_atomic(&path, json.as_bytes());
    }
}

fn read_cache(project_path: &Path, config: &NikiConfig) -> Vec<HistoryCacheEntry> {
    std::fs::read_to_string(cache_path(project_path, config))
        .map(|content| {
            content
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn append_cache(
    project_path: &Path,
    config: &NikiConfig,
    entries: &[HistoryCacheEntry],
) -> Result<()> {
    let path = cache_path(project_path, config);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    for entry in entries {
        writeln!(file, "{}", serde_json::to_string(entry)?)?;
    }
    let _ = file.sync_all();
    Ok(())
}

fn rebuild_derived(project_path: &Path, config: &NikiConfig) -> Result<()> {
    let entries = read_cache(project_path, config);
    let mut out = String::new();
    for cache_entry in &entries {
        if let Some(entry) = &cache_entry.entry {
            out.push_str(&serde_json::to_string(entry)?);
            out.push('\n');
        }
    }
    let path = derived_path(project_path, config);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::knowledge::kb::write_atomic(&path, out.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(repo: &git2::Repository, path: &str, content: &str, message: &str) {
        let workdir = repo.workdir().unwrap().to_path_buf();
        std::fs::create_dir_all(workdir.join(path).parent().unwrap()).unwrap();
        std::fs::write(workdir.join(path), content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("t", "t@t").unwrap();
        let parent = repo
            .revparse_single("HEAD")
            .ok()
            .and_then(|o| o.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }

    #[test]
    fn mines_keywords_and_skips_noise() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        commit(&repo, "src/a.rs", "fn a() {}\n", "initial");
        commit(&repo, "src/a.rs", "fn a() { panic!() }\n", "fix panic in a");
        commit(&repo, "README.md", "# docs\n", "fix typo in readme");

        let outcome = mine_history(tmp.path(), &NikiConfig::default(), "niki-task-test");
        assert!(!outcome.unsupported);
        assert_eq!(outcome.learned, 1);
        let derived =
            std::fs::read_to_string(derived_path(tmp.path(), &NikiConfig::default())).unwrap();
        assert!(derived.contains("fix panic in a"));
        assert!(!derived.contains("readme"));
    }

    #[test]
    fn skips_huge_diffs() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        commit(&repo, "src/a.rs", "fn a() {}\n", "initial");
        let big = "x\n".repeat(4000);
        commit(&repo, "src/big.rs", &big, "fix everything large");
        let outcome = mine_history(tmp.path(), &NikiConfig::default(), "niki-task-test");
        assert_eq!(outcome.skipped_large, 1);
        assert_eq!(outcome.learned, 0);
    }

    #[test]
    fn rewrite_busts_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        commit(&repo, "src/a.rs", "fn a() {}\n", "initial");
        commit(&repo, "src/a.rs", "fn a() { 1 }\n", "fix return value");
        let config = NikiConfig::default();
        let first = mine_history(tmp.path(), &config, "niki-task-test");
        assert_eq!(first.learned, 1);

        // Simulate a force-push: reset HEAD back, orphaning the analyzed head.
        let parent = repo
            .revparse_single("HEAD~1")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        repo.reset(parent.as_object(), git2::ResetType::Hard, None)
            .unwrap();

        let second = mine_history(tmp.path(), &config, "niki-task-test2");
        assert!(second.invalidated);
        // Cache rebuilt from scratch: the orphaned fix commit is unreachable.
        let derived = std::fs::read_to_string(derived_path(tmp.path(), &config)).unwrap();
        assert!(!derived.contains("fix return value"));
    }

    #[test]
    fn rerun_is_idempotent_cache_as_truth() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(tmp.path()).unwrap();
        commit(&repo, "src/a.rs", "fn a() {}\n", "fix initial shape");
        let config = NikiConfig::default();
        let first = mine_history(tmp.path(), &config, "niki-task-test");
        let second = mine_history(tmp.path(), &config, "niki-task-test");
        assert_eq!(first.learned, 1);
        assert_eq!(second.analyzed, 0, "second run skips known commits");
        let derived = std::fs::read_to_string(derived_path(tmp.path(), &config)).unwrap();
        assert_eq!(
            derived.lines().count(),
            1,
            "derived rebuilt, never duplicated"
        );
    }

    #[test]
    fn unsupported_outside_git() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = mine_history(tmp.path(), &NikiConfig::default(), "niki-task-test");
        assert!(outcome.unsupported);
        let state = read_state(tmp.path(), &NikiConfig::default());
        assert_eq!(state.status, "unsupported");
    }
}
