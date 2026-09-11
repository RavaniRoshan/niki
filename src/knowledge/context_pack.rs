//! Bounded Planner context: a token-budgeted pack assembled from the repo
//! manifest, the project KB, the structural index, and recent learnings.
//!
//! Sections are appended in priority order (boundary table → learnings →
//! architecture → entities → symbol excerpts) and assembly stops at
//! `[general] max_context_chars`, so the Planner always sees the most
//! load-bearing context first and the cut is explicit, never silent.

use crate::config::NikiConfig;
use crate::knowledge::kb::kb_root;
use crate::repo_intel::RepoManifest;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Max symbol-matched files excerpted into the pack.
const MAX_PACK_FILES: usize = 8;
/// Chars kept per excerpted file.
const CHARS_PER_FILE: usize = 3000;
/// Recent learnings considered for the task.
const MAX_LEARNINGS: usize = 3;

/// Small stoplist so keyword matching keys on content words.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "into", "add", "new", "use", "using",
    "should", "could", "would", "when", "where", "what", "which", "have", "has", "are", "was",
    "were", "been", "also", "such", "than", "then", "them", "they", "our", "your", "its",
];

/// Split a task description into content keywords (lowercase, len ≥ 3,
/// deduplicated, order-preserving).
pub fn task_keywords(task: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    task.split(|c: char| !c.is_alphanumeric())
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

/// Assemble the Planner context. Never fails: missing KB/index/learnings
/// simply omit their sections.
pub fn build_context_pack(
    project_path: &Path,
    config: &NikiConfig,
    task_description: &str,
    manifest: &RepoManifest,
) -> String {
    let budget = config.general.max_context_chars.max(1024);
    let mut out = String::new();
    let mut truncated = false;

    // 1. Repo boundary table (always included: entry points, build, tests).
    let mut boundary = String::from("## Repo boundary (source of truth for where code lives)\n");
    boundary.push_str(&format!(
        "Languages: {}\nSize: {} files, ~{} LOC\n",
        if manifest.languages.is_empty() {
            "(none detected)".to_string()
        } else {
            manifest.languages.join(", ")
        },
        manifest.files,
        manifest.loc
    ));
    if !manifest.entry_points.is_empty() {
        boundary.push_str(&format!(
            "Entry points: {}\n",
            manifest.entry_points.join(", ")
        ));
    }
    if !manifest.build_files.is_empty() {
        boundary.push_str(&format!(
            "Build files: {}\n",
            manifest.build_files.join(", ")
        ));
    }
    if !manifest.test_paths.is_empty() {
        let tests: Vec<String> = manifest.test_paths.iter().take(12).cloned().collect();
        boundary.push_str(&format!("Test paths: {}\n", tests.join(", ")));
    }
    push_section(&mut out, &boundary, budget, &mut truncated);

    // 2. Recent learnings relevant to the task (K=3, keyword overlap first).
    let keywords = task_keywords(task_description);
    let learnings = crate::knowledge::learnings::tail_learnings(project_path, config, 10);
    let picked = pick_learnings(&learnings, &keywords, MAX_LEARNINGS);
    if !picked.is_empty() {
        let mut section = String::from("## Learnings from past runs (advisory)\n");
        for learning in picked {
            section.push_str(&format!(
                "- [{}|{}] {}\n",
                learning.kind,
                learning.author_role,
                single_line(&learning.details, 400)
            ));
        }
        push_section(&mut out, &section, budget, &mut truncated);
    }

    // 3. architecture.md (truncated to its share of the budget).
    let arch_path = kb_root(project_path, config).join("architecture.md");
    if let Ok(content) = std::fs::read_to_string(&arch_path) {
        let body: String = content
            .lines()
            .skip_while(|l| l.starts_with("<!-- KB_SNAPSHOT:"))
            .collect::<Vec<_>>()
            .join("\n");
        let excerpt: String = body.chars().take(budget / 4).collect();
        if !excerpt.trim().is_empty() {
            push_section(
                &mut out,
                &format!("## Architecture (from project KB)\n{excerpt}\n"),
                budget,
                &mut truncated,
            );
        }
    }

    // 4. Entities whose path matches task keywords.
    if !keywords.is_empty() {
        let entities_dir = kb_root(project_path, config).join("entities");
        let mut matched = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&entities_dir) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let slug = name.strip_suffix(".md").unwrap_or(&name).to_string();
                if keywords
                    .iter()
                    .any(|k| slug.contains(k) || k.contains(&slug))
                {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        matched.push((slug, content));
                    }
                }
            }
        }
        matched.sort_by(|a, b| a.0.cmp(&b.0));
        for (slug, content) in matched.iter().take(4) {
            let excerpt: String = content.chars().take(1500).collect();
            push_section(
                &mut out,
                &format!("## Entity: {slug}\n{excerpt}\n"),
                budget,
                &mut truncated,
            );
            if truncated {
                break;
            }
        }
    }

    // 5. Symbol-matched file excerpts from the structural index.
    if !keywords.is_empty() {
        let excerpts = symbol_excerpts(project_path, config, &keywords);
        if !excerpts.is_empty() {
            let mut section =
                String::from("## Relevant code (symbol-index matched, verify with search)\n");
            for (path, preview) in excerpts.iter().take(MAX_PACK_FILES) {
                section.push_str(&format!("### {path}\n```\n{preview}\n```\n"));
            }
            push_section(&mut out, &section, budget, &mut truncated);
        }
    }

    if truncated {
        out.push_str("\n[context truncated at max_context_chars budget]\n");
    }
    out.push_str(
        "\n(Context sources: repo manifest + project KB + symbol index — advisory. \
         Verify file contents with search before planning edits.)\n",
    );
    out
}

fn push_section(out: &mut String, section: &str, budget: usize, truncated: &mut bool) {
    if out.len() + section.len() <= budget {
        out.push_str(section);
        out.push('\n');
    } else {
        *truncated = true;
    }
}

fn pick_learnings(
    learnings: &[crate::knowledge::learnings::LearningEntry],
    keywords: &[String],
    n: usize,
) -> Vec<crate::knowledge::learnings::LearningEntry> {
    // Newest first, then stable-sort by score: keyword hits lead, and ties
    // stay newest-first. `sort_by` is stable, so this composes correctly.
    let mut scored: Vec<(usize, &crate::knowledge::learnings::LearningEntry)> = learnings
        .iter()
        .rev()
        .map(|l| {
            let details = l.details.to_lowercase();
            let score = keywords
                .iter()
                .filter(|k| details.contains(k.as_str()))
                .count();
            (score, l)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(n).map(|(_, l)| l.clone()).collect()
}

/// Rank indexed files by symbol-name overlap with task keywords and excerpt
/// the top hits. Returns `(path, preview)` pairs.
fn symbol_excerpts(
    project_path: &Path,
    config: &NikiConfig,
    keywords: &[String],
) -> Vec<(String, String)> {
    let index = match crate::knowledge::structural::open_index(project_path, config) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let mut scores: HashMap<&str, usize> = HashMap::new();
    for unit in &index.units {
        if unit.backend == "coverage" {
            continue;
        }
        for sym in &unit.symbols {
            let name = sym.name.to_lowercase();
            for kw in keywords {
                if name == *kw || name.contains(kw) || kw.contains(&name) {
                    *scores.entry(unit.path.as_str()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut ranked: Vec<(&str, usize)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = Vec::new();
    for (path, _) in ranked.into_iter().take(MAX_PACK_FILES) {
        let full = project_path.join(path);
        if let Ok(content) = std::fs::read_to_string(&full) {
            let preview: String = content.chars().take(CHARS_PER_FILE).collect();
            out.push((path.to_string(), preview));
        }
    }
    out
}

fn single_line(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        format!("{}…", flat.chars().take(max).collect::<String>())
    } else {
        flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_intel::build_manifest;
    use std::fs;

    fn tiny_project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/auth.rs"), "pub fn login() {}\n").unwrap();
        fs::write(tmp.path().join("src/main.rs"), "mod auth;\nfn main() {}\n").unwrap();
        tmp
    }

    #[test]
    fn pack_contains_boundary_and_respects_budget() {
        let tmp = tiny_project();
        // A large matched file forces the budget cut.
        fs::write(
            tmp.path().join("src/big.rs"),
            format!("pub fn login_big() {{}}\n// {}\n", "padding ".repeat(1000)),
        )
        .unwrap();
        crate::knowledge::structural::build_index(tmp.path(), &NikiConfig::default(), "t").unwrap();
        let mut config = NikiConfig::default();
        config.general.max_context_chars = 1100;
        let manifest = build_manifest(tmp.path(), &config);
        let pack = build_context_pack(tmp.path(), &config, "fix login flow", &manifest);
        assert!(pack.contains("## Repo boundary"));
        assert!(pack.contains("src/main.rs"));
        assert!(
            pack.contains("[context truncated"),
            "budget cut must be explicit"
        );
        assert!(pack.len() <= 1100 + 512, "bounded with marker slack");
    }

    #[test]
    fn pack_without_kb_or_index_still_packs() {
        let tmp = tiny_project();
        let config = NikiConfig::default();
        let manifest = build_manifest(tmp.path(), &config);
        let pack = build_context_pack(tmp.path(), &config, "fix login flow", &manifest);
        assert!(pack.contains("## Repo boundary"));
        assert!(pack.contains("advisory"));
    }

    #[test]
    fn pack_includes_symbol_excerpts_and_learnings() {
        let tmp = tiny_project();
        let config = NikiConfig::default();
        crate::knowledge::structural::build_index(tmp.path(), &config, "niki-task-test").unwrap();
        crate::knowledge::learnings::append_learning(
            tmp.path(),
            &config,
            &crate::knowledge::learnings::LearningEntry::new(
                "history",
                "niki-task-test",
                "history-miner",
                "inferred",
                "login retry fix in auth".to_string(),
            ),
        )
        .unwrap();
        let manifest = build_manifest(tmp.path(), &config);
        let pack = build_context_pack(tmp.path(), &config, "fix login retry", &manifest);
        assert!(
            pack.contains("src/auth.rs"),
            "symbol-matched file excerpted"
        );
        assert!(
            pack.contains("login retry fix"),
            "relevant learning included"
        );
    }

    #[test]
    fn keywords_drop_stopwords_and_dupes() {
        let kw = task_keywords("Fix the login flow for the login page");
        assert!(kw.contains(&"login".to_string()));
        assert!(kw.contains(&"flow".to_string()));
        assert!(!kw.contains(&"the".to_string()));
        assert_eq!(kw.iter().filter(|k| *k == "login").count(), 1);
    }
}
