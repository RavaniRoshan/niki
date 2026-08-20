//! AGENTS.md hierarchy loader.
//!
//! Mirrors the Claude Code / kimi convention: behavior instructions are read
//! from `AGENTS.md` files, with the most specific (deepest) entry winning.
//!
//! Load order (later overrides earlier on conflict):
//!   1. global  `~/.niki/AGENTS.md`
//!   2. project root `AGENTS.md`
//!   3. any nested `AGENTS.md` (sorted by path depth, shallow → deep)

use std::fs;
use std::path::Path;

use walkdir::WalkDir;

/// Combined AGENTS.md guidance for a project.
pub struct AgentsMd {
    /// Concatenated, sectioned instruction text.
    pub text: String,
    /// Set when the combined text exceeds a sane cap (surface to the footer as
    /// a `warningHint` so the user knows context is being truncated).
    pub size_warning: Option<String>,
}

const SIZE_CAP: usize = 16_000;

/// Load the AGENTS.md hierarchy for `project_path`.
pub fn load_agents_md_hierarchy(project_path: &Path) -> AgentsMd {
    let mut sections: Vec<(usize, String)> = Vec::new();

    // 1. Global ~/.niki/AGENTS.md
    if let Some(home) = dirs::home_dir() {
        let global = home.join(".niki").join("AGENTS.md");
        if let Ok(c) = fs::read_to_string(&global) {
            let c = c.trim().to_string();
            if !c.is_empty() {
                sections.push((0, format!("## Global (~/.niki/AGENTS.md)\n{}", c)));
            }
        }
    }

    // 2. Project root AGENTS.md
    let root = project_path.join("AGENTS.md");
    if let Ok(c) = fs::read_to_string(&root) {
        let c = c.trim().to_string();
        if !c.is_empty() {
            sections.push((1, format!("## Project ({})\n{}", project_path.display(), c)));
        }
    }

    // 3. Nested AGENTS.md, shallow → deep
    let mut nested: Vec<(usize, String)> = Vec::new();
    if let Ok(walker) = WalkDir::new(project_path)
        .min_depth(2)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    {
        for entry in walker {
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|n| n == "AGENTS.md") {
                if let Some(parent) = path.parent() {
                    if is_ignored(parent) {
                        continue;
                    }
                }
                if let Ok(c) = fs::read_to_string(path) {
                    let c = c.trim().to_string();
                    if !c.is_empty() {
                        let depth = path.components().count();
                        nested.push((depth, format!("## {}\n{}", path.display(), c)));
                    }
                }
            }
        }
    }
    nested.sort_by_key(|(d, _)| *d);
    sections.extend(nested);

    let combined = sections
        .into_iter()
        .map(|(_, s)| s)
        .collect::<Vec<_>>()
        .join("\n\n");

    let size_warning = if combined.len() > SIZE_CAP {
        Some(format!(
            "AGENTS.md guidance is {} chars (>{}); truncated for context",
            combined.len(),
            SIZE_CAP
        ))
    } else {
        None
    };

    let text = if combined.len() > SIZE_CAP {
        combined.chars().take(SIZE_CAP).collect()
    } else {
        combined
    };

    AgentsMd { text, size_warning }
}

/// Skip vendored / generated / VCS directories when walking for nested files.
fn is_ignored(dir: &Path) -> bool {
    dir.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(".git") | Some(".niki") | Some("target") | Some("node_modules")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_loads_global_then_project_then_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("AGENTS.md"), "PROJECT RULES").unwrap();
        let sub = root.join("src").join("mod");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("AGENTS.md"), "NESTED RULES").unwrap();

        // Global may or may not exist; just assert project + nested are found.
        let result = load_agents_md_hierarchy(root);
        assert!(result.text.contains("PROJECT RULES"));
        assert!(result.text.contains("NESTED RULES"));
        // Project (depth 1) appears before nested (depth 3).
        let p = result.text.find("PROJECT RULES").unwrap();
        let n = result.text.find("NESTED RULES").unwrap();
        assert!(p < n);
    }

    #[test]
    fn ignores_vcs_and_target_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("AGENTS.md"), "SHOULD BE IGNORED").unwrap();
        let result = load_agents_md_hierarchy(root);
        assert!(!result.text.contains("SHOULD BE IGNORED"));
    }
}
