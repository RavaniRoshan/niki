use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::NikiConfig;
use crate::knowledge::kb::{
    Authority, KbManifest, Sidecar, ensure_layout, kb_root, write_atomic, write_kb_markdown,
};
use crate::repo_intel::build_manifest;

#[derive(Subcommand)]
pub enum ArchitectureCommands {
    /// Build (or rebuild from scratch) the project knowledge base under
    /// `<output_dir>/kb/`. Deterministic and manual — normal `niki run`
    /// never synthesizes KB content automatically.
    Build {
        /// Project directory (defaults to the current directory).
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

/// Summary of one architecture build, for CLI display and tests.
#[derive(Debug)]
pub struct ArchitectureBuildSummary {
    pub files_written: Vec<String>,
    pub entities: usize,
    pub learnings_included: usize,
    pub history_entries: usize,
}

pub async fn handle(command: &ArchitectureCommands) -> Result<()> {
    match command {
        ArchitectureCommands::Build { project } => {
            let project_dir = project
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let summary = build_architecture(&project_dir)?;
            println!(
                "Knowledge base built under {}",
                kb_root_display(&project_dir)
            );
            println!("  {} files written", summary.files_written.len());
            println!("  {} entities", summary.entities);
            println!("  {} recent learnings included", summary.learnings_included);
            println!("  {} history entries included", summary.history_entries);
            Ok(())
        }
    }
}

fn kb_root_display(project_dir: &Path) -> String {
    match NikiConfig::load(project_dir) {
        Ok(config) => kb_root(project_dir, &config).display().to_string(),
        Err(_) => project_dir.join(".niki/kb").display().to_string(),
    }
}

/// Build the KB from scratch: RepoManifest + recent learnings + recent
/// security verdicts + mined history. Deterministic, no LLM.
pub fn build_architecture(project_dir: &Path) -> Result<ArchitectureBuildSummary> {
    const GENERATED_BY: &str = "architecture-build";
    let config = NikiConfig::load(project_dir).unwrap_or_default();
    let manifest = build_manifest(project_dir, &config);
    let commit_sha = current_commit_sha(project_dir);
    let snapshot_id = crate::orchestrator::provenance::manual_snapshot_id(project_dir);

    let root = ensure_layout(project_dir, &config, commit_sha.as_deref(), GENERATED_BY)?;
    let mut files_written = Vec::new();

    // Refresh the history cache first so history.md reflects this repo state.
    // Best-effort: a mining failure leaves the previous cache in place.
    if config.repo_intel.history {
        let _ = crate::knowledge::history::mine_history(project_dir, &config, &snapshot_id);
    }

    let learnings = crate::knowledge::learnings::tail_learnings(project_dir, &config, 5);
    let history_entries = read_history_learnings(project_dir, &config, 5);
    let security_notes = recent_security_notes(project_dir, &config, 5);

    // architecture.md: repo shape + learnings + security posture.
    let mut arch = String::from("# Architecture\n\n");
    arch.push_str("## Repository shape\n\n");
    arch.push_str(&format!(
        "- Languages: {}\n- Size: {} files, ~{} LOC{}\n",
        if manifest.languages.is_empty() {
            "(none detected)".to_string()
        } else {
            manifest.languages.join(", ")
        },
        manifest.files,
        manifest.loc,
        if manifest.truncated {
            " (truncated)"
        } else {
            ""
        },
    ));
    if !manifest.entry_points.is_empty() {
        arch.push_str(&format!(
            "- Entry points: {}\n",
            manifest.entry_points.join(", ")
        ));
    }
    if !manifest.build_files.is_empty() {
        arch.push_str(&format!(
            "- Build files: {}\n",
            manifest.build_files.join(", ")
        ));
    }
    if !manifest.test_paths.is_empty() {
        arch.push_str(&format!(
            "- Test roots: {}\n",
            first_n(&manifest.test_paths, 8).join(", ")
        ));
    }
    arch.push('\n');
    if !learnings.is_empty() {
        arch.push_str("## Recent learnings\n\n");
        for learning in &learnings {
            arch.push_str(&format!(
                "- [{}|{}] {}\n",
                learning.kind,
                learning.author_role,
                single_line(&learning.details, 300)
            ));
        }
        arch.push('\n');
    }
    if !security_notes.is_empty() {
        arch.push_str("## Recent security posture\n\n");
        for note in &security_notes {
            arch.push_str(&format!("- {}\n", single_line(note, 300)));
        }
        arch.push('\n');
    }
    if !manifest.risk_signals.is_empty() {
        arch.push_str("## Risk cues (advisory)\n\n");
        for signal in &manifest.risk_signals {
            arch.push_str(&format!(
                "- [{}] `{}` in {}\n",
                signal.rule, signal.pattern, signal.detail
            ));
        }
        arch.push('\n');
    }
    let arch_path = root.join("architecture.md");
    write_kb_markdown(&arch_path, commit_sha.as_deref(), GENERATED_BY, &arch)?;
    files_written.push("architecture.md".to_string());

    // history.md: mined-history summary.
    let mut hist = String::from("# History\n\n");
    if history_entries.is_empty() {
        hist.push_str("(no history mined yet)\n");
    } else {
        hist.push_str("Mined from git history (keyword-classified, cached as truth):\n\n");
        for entry in &history_entries {
            hist.push_str(&format!("- {}\n", single_line(entry, 300)));
        }
    }
    let hist_path = root.join("history.md");
    write_kb_markdown(&hist_path, commit_sha.as_deref(), GENERATED_BY, &hist)?;
    files_written.push("history.md".to_string());

    // entities/<slug>.md: one per top-level code area.
    let entities = collect_entities(project_dir);
    for (slug, info) in &entities {
        let body = format!(
            "# Entity: {slug}\n\n- Directory: `{}`\n- Files: {}\n- Languages: {}\n",
            info.dir,
            info.files,
            if info.languages.is_empty() {
                "(none)".to_string()
            } else {
                info.languages.join(", ")
            }
        );
        let path = root.join("entities").join(format!("{slug}.md"));
        write_kb_markdown(&path, commit_sha.as_deref(), GENERATED_BY, &body)?;
        files_written.push(format!("entities/{slug}.md"));
    }

    // dependencies.json: provenance-wrapped inventory.
    let rows: Vec<crate::knowledge::kb::DependencyRow> = manifest
        .package_info
        .iter()
        .map(|p| crate::knowledge::kb::DependencyRow {
            manager: p.manager.clone(),
            file_path: p.file_path.clone(),
            dependencies: p.dependencies.clone(),
        })
        .collect();
    let sidecar = Sidecar {
        generated_by: GENERATED_BY.to_string(),
        generated_at: chrono::Utc::now(),
        state_ref: snapshot_id.clone(),
        authority: Authority::Inferred,
        payload: rows,
    };
    write_atomic(
        &root.join("dependencies.json"),
        serde_json::to_string_pretty(&sidecar)?.as_bytes(),
    )?;
    files_written.push("dependencies.json".to_string());

    // manifest.json last: the commit point listing everything written.
    files_written.push("manifest.json".to_string());
    let kb_manifest = KbManifest {
        snapshot_id,
        commit_sha,
        generated_at: chrono::Utc::now(),
        generated_by: GENERATED_BY.to_string(),
        files: files_written.clone(),
    };
    write_atomic(
        &root.join("manifest.json"),
        serde_json::to_string_pretty(&kb_manifest)?.as_bytes(),
    )?;

    Ok(ArchitectureBuildSummary {
        files_written,
        entities: entities.len(),
        learnings_included: learnings.len(),
        history_entries: history_entries.len(),
    })
}

struct EntityInfo {
    dir: String,
    files: usize,
    languages: Vec<String>,
}

fn collect_entities(project_dir: &Path) -> Vec<(String, EntityInfo)> {
    let mut map: HashMap<String, (String, usize, std::collections::HashSet<String>)> =
        HashMap::new();
    let walker = walkdir::WalkDir::new(project_dir)
        .max_depth(3)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            if crate::repo_intel::manifest_vendor_dirs().contains(&name.as_ref()) {
                return false;
            }
            !name.starts_with('.')
        });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(project_dir)
            .unwrap_or(entry.path());
        let Some(lang) = crate::repo_intel::manifest_language_for(rel) else {
            continue;
        };
        let top = rel
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        // Files at the repo root group under "root".
        let (slug, dir) = if rel.components().count() == 1 {
            ("root".to_string(), ".".to_string())
        } else {
            (slugify(&top), top.clone())
        };
        let slot = map
            .entry(slug)
            .or_insert_with(|| (dir, 0, std::collections::HashSet::new()));
        slot.1 += 1;
        slot.2.insert(lang.to_string());
    }
    let mut out: Vec<(String, EntityInfo)> = map
        .into_iter()
        .map(|(slug, (dir, files, langs))| {
            let mut languages: Vec<String> = langs.into_iter().collect();
            languages.sort();
            (
                slug,
                EntityInfo {
                    dir,
                    files,
                    languages,
                },
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "area".to_string()
    } else {
        slug
    }
}

fn current_commit_sha(project_path: &Path) -> Option<String> {
    git2::Repository::open(project_path)
        .ok()?
        .revparse_single("HEAD")
        .ok()?
        .peel_to_commit()
        .ok()
        .map(|c| c.id().to_string())
}

fn read_history_learnings(project_path: &Path, config: &NikiConfig, n: usize) -> Vec<String> {
    let path = project_path
        .join(&config.general.output_dir)
        .join("history")
        .join("learnings.jsonl");
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut entries: Vec<String> = content
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| {
            v.get("details")
                .and_then(|d| d.as_str())
                .map(|d| d.to_string())
        })
        .collect();
    if entries.len() > n {
        entries.drain(..entries.len() - n);
    }
    entries
}

/// Last `n` security verdicts across tasks (newest task dirs first).
/// Best-effort: unreadable or unparsable artifacts are skipped.
fn recent_security_notes(project_path: &Path, config: &NikiConfig, n: usize) -> Vec<String> {
    let tasks_dir = project_path.join(&config.general.output_dir).join("tasks");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&tasks_dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.sort();
    dirs.reverse();
    let mut notes = Vec::new();
    for dir in dirs {
        if notes.len() >= n {
            break;
        }
        let path = dir.join("artifacts").join("security_auditor.json");
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let verdict = value.get("verdict").and_then(|v| v.as_str()).unwrap_or("?");
        let findings = value
            .get("findings")
            .and_then(|f| f.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let task = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        notes.push(format!(
            "task {task}: verdict {verdict}, {findings} finding(s)"
        ));
    }
    notes
}

fn first_n(items: &[String], n: usize) -> Vec<String> {
    items.iter().take(n).cloned().collect()
}

fn single_line(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        format!("{}…", flat.chars().take(max).collect::<String>())
    } else {
        flat
    }
}
