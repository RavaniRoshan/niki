use anyhow::Result;
use std::path::Path;
use std::fs;
use walkdir::WalkDir;
use git2::Repository;
use std::collections::HashSet;

pub struct ProjectKnowledge {
    pub file_tree: String,
    pub detected_languages: Vec<String>,
    pub package_info: Vec<PackageInfo>,
    pub git_recent_commits: Vec<CommitSummary>,
    pub skills_files: Vec<SkillsFile>,
    pub project_size: ProjectSize,
}

pub struct PackageInfo {
    pub manager: String,
    pub file_path: String,
    pub dependencies: Vec<String>,
}

pub struct CommitSummary {
    pub hash: String,
    pub message: String,
}

pub struct SkillsFile {
    pub path: String,
    pub content: String,
}

pub enum ProjectSize {
    Small,
    Medium,
    Large,
}

impl ProjectKnowledge {
    pub fn render(&self) -> String {
        let mut output = String::new();

        output.push_str("## Project Structure\n");
        output.push_str(&self.file_tree);
        output.push('\n');

        output.push_str("## Languages\n");
        output.push_str(&self.detected_languages.join(", "));
        output.push_str("\n\n");

        if !self.package_info.is_empty() {
            output.push_str("## Dependencies\n");
            for pkg in &self.package_info {
                output.push_str(&format!("{}: {}\n", pkg.manager, pkg.dependencies.join(", ")));
            }
            output.push('\n');
        }

        if !self.git_recent_commits.is_empty() {
            output.push_str("## Recent Git History\n");
            for commit in &self.git_recent_commits {
                output.push_str(&format!("- {}: {}\n", commit.hash, commit.message));
            }
            output.push('\n');
        }

        if !self.skills_files.is_empty() {
            output.push_str("## Project Conventions\n");
            for skill in &self.skills_files {
                output.push_str(&format!("### {}\n{}\n\n", skill.path, skill.content));
            }
        }

        output
    }
}

pub fn index_project(path: &Path, _config: &crate::config::NikiConfig) -> Result<ProjectKnowledge> {
    let mut file_count = 0;
    let mut languages = HashSet::new();
    let mut tree_lines = Vec::new();
    let mut package_info = Vec::new();
    let mut skills_files = Vec::new();

    for entry in WalkDir::new(path)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            // Always include the project root, even if its name is dot-prefixed
            // (e.g. a temp dir). Only skip hidden entries *within* the project.
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') || name == ".cursorrules"
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let rel_path = entry.path().strip_prefix(path).unwrap_or(entry.path());
        if rel_path.as_os_str().is_empty() {
            continue;
        }

        let depth = rel_path.components().count();
        let indent = "  ".repeat(depth.saturating_sub(1));
        let is_dir = entry.file_type().is_dir();
        
        let name = entry.file_name().to_string_lossy().to_string();
        if is_dir {
            tree_lines.push(format!("{}{}/", indent, name));
        } else {
            tree_lines.push(format!("{}{}", indent, name));
            file_count += 1;

            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                match ext {
                    "rs" => { languages.insert("Rust"); }
                    "js" | "jsx" => { languages.insert("JavaScript"); }
                    "ts" | "tsx" => { languages.insert("TypeScript"); }
                    "py" => { languages.insert("Python"); }
                    "go" => { languages.insert("Go"); }
                    _ => {}
                }
            }

            if name == "Cargo.toml" {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let mut deps = Vec::new();
                    if let Ok(value) = content.parse::<toml::Value>() {
                        if let Some(d) = value.get("dependencies").and_then(|v| v.as_table()) {
                            deps.extend(d.keys().cloned());
                        }
                    }
                    package_info.push(PackageInfo {
                        manager: "Cargo.toml".to_string(),
                        file_path: rel_path.to_string_lossy().to_string(),
                        dependencies: deps,
                    });
                }
            } else if name == "package.json" {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let mut deps = Vec::new();
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(d) = value.get("dependencies").and_then(|v| v.as_object()) {
                            deps.extend(d.keys().cloned());
                        }
                    }
                    package_info.push(PackageInfo {
                        manager: "package.json".to_string(),
                        file_path: rel_path.to_string_lossy().to_string(),
                        dependencies: deps,
                    });
                }
            }

            if ["AGENTS.md", "CLAUDE.md", ".cursorrules", ".editorconfig"].contains(&name.as_str()) {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    skills_files.push(SkillsFile {
                        path: rel_path.to_string_lossy().to_string(),
                        content,
                    });
                }
            }
        }
    }

    let mut git_recent_commits = Vec::new();
    if let Ok(repo) = Repository::open(path) {
        if let Ok(mut revwalk) = repo.revwalk() {
            if revwalk.push_head().is_ok() {
                for oid in revwalk.take(10) {
                    if let Ok(oid) = oid {
                        if let Ok(commit) = repo.find_commit(oid) {
                            git_recent_commits.push(CommitSummary {
                                hash: commit.id().to_string()[..7].to_string(),
                                message: commit.summary().unwrap_or("").to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    let project_size = if file_count < 50 {
        ProjectSize::Small
    } else if file_count < 500 {
        ProjectSize::Medium
    } else {
        ProjectSize::Large
    };

    Ok(ProjectKnowledge {
        file_tree: tree_lines.join("\n"),
        detected_languages: languages.into_iter().map(|s| s.to_string()).collect(),
        package_info,
        git_recent_commits,
        skills_files,
        project_size,
    })
}
