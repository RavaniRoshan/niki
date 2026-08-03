use anyhow::Result;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// A concise text summary of a project's structure and contents.
/// This is injected into the Planner context so it understands the repo layout.
#[derive(Default)]
pub struct ProjectSummary {
    /// List of top-level directories and their entry-point files.
    pub top_level: Vec<String>,
    /// List of source file paths (relative, sorted, depth-limited).
    pub source_files: Vec<String>,
    /// Entry points for build/test/run.
    pub entry_points: Vec<String>,
    /// Total lines of code across indexed source files.
    pub total_loc: usize,
}

impl ProjectSummary {
    /// Build a project summary by scanning the directory structure.
    pub fn build(project_dir: &Path) -> Result<Self> {
        let mut top_level = Vec::new();
        let mut source_files = Vec::new();
        let mut entry_points = Vec::new();
        let mut total_loc = 0usize;

        // Top-level entries
        if let Ok(entries) = fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    top_level.push(format!("{} (dir)", name));
                } else if path.is_file() {
                    let _ext = path.extension().and_then(|e| e.to_str());
                    let size = entry
                        .metadata()
                        .ok()
                        .map(|m| m.len())
                        .unwrap_or(0);
                    top_level.push(format!("{} ({} bytes)", name, size));
                }
            }
        }

        top_level.sort();

        // Source file extensions
        let source_exts: Vec<&str> = vec![
            "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "cpp", "h", "hpp", "cs", "rb",
            "php", "sh", "toml", "json", "yaml", "yml", "md", "txt", "cfg", "ini",
        ];

        // Walk the tree, skipping common noise directories
        for entry in WalkDir::new(project_dir)
            .max_depth(5)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.')
                    && name != "node_modules"
                    && name != "target"
                    && name != "__pycache__"
                    && name != "venv"
                    && name != ".venv"
                    && name != "dist"
                    && name != "build"
                    && name != ".git"
                    && name != ".next"
                    && name != ".astro"
                    && name != ".svelte-kit"
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str());
            let Some(ext) = ext else {
                continue;
            };

            if source_exts.contains(&ext) {
                if let Ok(rel) = path.strip_prefix(project_dir) {
                    if let Some(rel_str) = rel.to_str() {
                        source_files.push(rel_str.to_string());
                    }
                }

                // Entry point detection
                let name = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                let is_entry = matches!(
                    name,
                    "main"
                        | "index"
                        | "lib"
                        | "mod"
                        | "start"
                        | "app"
                        | "server"
                        | "cli"
                        | "run"
                        | "setup"
                        | "build"
                        | "test"
                        | "tests"
                );
                if is_entry {
                    if let Ok(rel) = path.strip_prefix(project_dir) {
                        let rel_depth = rel.parent().map(|p| p.components().count()).unwrap_or(0);
                        if rel_depth <= 2 {
                            if let Some(rel_str) = rel.to_str() {
                                entry_points.push(rel_str.to_string());
                            }
                        }
                    }
                }

                // Count lines for .rs, .ts, .js, .py, .go, .java, .c, .cpp
                if matches!(
                    ext,
                    "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs" | "rb" | "php"
                ) {
                    if let Ok(content) = fs::read_to_string(path) {
                        total_loc += content.lines().count();
                    }
                }
            }
        }

        source_files.sort();
        entry_points.sort();
        entry_points.dedup();

        Ok(ProjectSummary {
            top_level,
            source_files,
            entry_points,
            total_loc,
        })
    }

    /// Render the summary as a concise text block for LLM context injection.
    pub fn render(&self, max_files: usize) -> String {
        let mut out = String::new();

        out.push_str("## Project Structure\n\n");
        out.push_str("### Top-level\n");
        for entry in self.top_level.iter().take(20) {
            out.push_str(&format!("- {}\n", entry));
        }
        out.push('\n');

        out.push_str("### Entry Points\n");
        if self.entry_points.is_empty() {
            out.push_str("(none found)\n");
        } else {
            for ep in &self.entry_points {
                out.push_str(&format!("- {}\n", ep));
            }
        }
        out.push('\n');

        out.push_str(&format!(
            "### Source Files ({} total, showing top {})\n",
            self.source_files.len(),
            max_files
        ));
        for f in self.source_files.iter().take(max_files) {
            out.push_str(&format!("- {}\n", f));
        }
        if self.source_files.len() > max_files {
            out.push_str(&format!(
                "... and {} more\n",
                self.source_files.len() - max_files
            ));
        }
        out.push('\n');

        out.push_str(&format!(
            "### Total Lines of Code: {}\n",
            self.total_loc
        ));

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_summary() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src").join("main.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();
        fs::write(dir.join("src").join("lib.rs"), "// lib\n").unwrap();
        fs::write(dir.join("README.md"), "# Test\n").unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        let summary = ProjectSummary::build(dir).unwrap();
        assert!(summary.top_level.iter().any(|e| e.starts_with("src")));
        assert!(summary.source_files.iter().any(|f| f == "src/main.rs"));
        assert!(summary.source_files.iter().any(|f| f == "src/lib.rs"));
        assert!(summary.source_files.iter().any(|f| f == "README.md"));
        assert!(summary.entry_points.iter().any(|e| e == "src/main.rs"));
    }

    #[test]
    fn test_render_output() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src").join("main.rs"), "fn main() {}\n").unwrap();

        let summary = ProjectSummary::build(dir).unwrap();
        let rendered = summary.render(10);
        assert!(rendered.contains("Project Structure"));
        assert!(rendered.contains("src/main.rs"));
        assert!(rendered.contains("Total Lines of Code"));
    }

    #[test]
    fn test_build_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let summary = ProjectSummary::build(tmp.path()).unwrap();
        assert!(summary.top_level.is_empty());
        assert!(summary.source_files.is_empty());
        assert_eq!(summary.total_loc, 0);
    }
}
