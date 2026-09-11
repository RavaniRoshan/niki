//! Deterministic repository manifest: a first-class, fail-soft summary of
//! what a repo contains, with no LLM involved.
//!
//! [`build_manifest`] never fails: unreadable files, missing dirs, and unit
//! overruns degrade to empty/truncated fields rather than errors, so `niki run`
//! can always proceed.

use crate::config::NikiConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Cap on characters read per file when estimating LOC (huge generated files
/// must not blow up memory or the walk budget).
const MAX_CHARS_PER_FILE: usize = 1_000_000;

/// Directory names that are never indexed (vendored, generated, or VCS state).
pub(crate) const VENDOR_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".git",
    ".hg",
    ".svn",
    ".niki-worktrees",
];

/// Files that mark a build system / project root.
const BUILD_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "go.mod",
    "Makefile",
    "CMakeLists.txt",
    "build.gradle",
    "pom.xml",
    "Dockerfile",
    "docker-compose.yml",
    "compose.yml",
];

/// Files that carry project configuration (superset of build files).
const CONFIG_FILES: &[&str] = &[
    "niki.toml",
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "package-lock.json",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "go.mod",
    "go.sum",
    "Makefile",
    "CMakeLists.txt",
    "Dockerfile",
    "docker-compose.yml",
    "compose.yml",
    ".editorconfig",
    "rust-toolchain.toml",
    "tsconfig.json",
    "vite.config.ts",
    "webpack.config.js",
];

/// Candidate entry points checked by relative path (cheap existence probes).
const ENTRY_CANDIDATES: &[&str] = &[
    "src/main.rs",
    "src/lib.rs",
    "src/main.py",
    "src/index.ts",
    "src/index.js",
    "src/main.ts",
    "src/main.js",
    "main.rs",
    "main.py",
    "main.go",
    "app.py",
    "index.js",
    "index.ts",
];

/// A deterministic risk cue: a config-pattern hit against the manifest.
/// Advisory only — the risk classifier (Phase 6) decides what it means.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskSignal {
    /// Which config list produced this signal (`denylist` or `severity`).
    pub rule: String,
    /// The matched pattern.
    pub pattern: String,
    /// Where it matched (file path or spec field).
    pub detail: String,
}

/// First-class repository understanding: what the repo is made of.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoManifest {
    /// Detected code languages (e.g. `["Rust", "TypeScript"]`), sorted.
    pub languages: Vec<String>,
    /// Probable program entry points (relative paths), sorted.
    pub entry_points: Vec<String>,
    /// Test dirs/files (relative paths), sorted.
    pub test_paths: Vec<String>,
    /// Config files (relative paths), sorted.
    pub config_paths: Vec<String>,
    /// Vendor/generated dirs present at the root (e.g. `["target"]`), sorted.
    pub vendor_dirs: Vec<String>,
    /// Build-system files (relative paths), sorted.
    pub build_files: Vec<String>,
    /// Number of files considered (excluding vendor/hidden).
    pub files: usize,
    /// Estimated lines of code across readable text files.
    pub loc: usize,
    /// `(manager, file, dependencies)` triples for recognized manifests.
    pub package_info: Vec<PackageSummary>,
    /// Config-driven risk cues (path matches only — no content scan).
    pub risk_signals: Vec<RiskSignal>,
    /// True when the walk stopped early at `max_units`.
    pub truncated: bool,
}

/// One dependency manifest found in the repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSummary {
    pub manager: String,
    pub file_path: String,
    pub dependencies: Vec<String>,
}

/// Build a [`RepoManifest`] for `project_path`. Never fails: every I/O error
/// degrades to an empty/truncated field.
pub fn build_manifest(project_path: &Path, config: &NikiConfig) -> RepoManifest {
    let max_units = config.repo_intel.max_units.max(1);
    let mut languages: HashSet<String> = HashSet::new();
    let mut test_paths: HashSet<String> = HashSet::new();
    let mut config_paths: HashSet<String> = HashSet::new();
    let mut build_files: HashSet<String> = HashSet::new();
    let mut package_info = Vec::new();
    let mut files = 0usize;
    let mut loc = 0usize;
    let mut truncated = false;

    let walker = WalkDir::new(project_path)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            if VENDOR_DIRS.contains(&name.as_ref()) {
                return false;
            }
            !name.starts_with('.')
        });

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if files >= max_units {
            truncated = true;
            break;
        }
        files += 1;

        let rel = entry
            .path()
            .strip_prefix(project_path)
            .unwrap_or(entry.path());
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let name = entry.file_name().to_string_lossy().to_string();

        if let Some(lang) = language_for(rel) {
            languages.insert(lang.to_string());
        }
        if is_test_path(&rel_str) {
            test_paths.insert(rel_str.clone());
        }
        if CONFIG_FILES.contains(&name.as_str()) {
            config_paths.insert(rel_str.clone());
        }
        if BUILD_FILES.contains(&name.as_str()) {
            build_files.insert(rel_str.clone());
        }
        if name == "Cargo.toml" {
            package_info.push(parse_cargo_deps(entry.path(), &rel_str));
        } else if name == "package.json" {
            package_info.push(parse_npm_deps(entry.path(), &rel_str));
        }

        // LOC estimate: text files only, per-file char cap.
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            let capped: String = content.chars().take(MAX_CHARS_PER_FILE).collect();
            loc += capped.lines().count();
        }
    }

    let mut languages: Vec<String> = languages.into_iter().collect();
    languages.sort();
    let mut test_paths: Vec<PathBuf> = test_paths.into_iter().map(PathBuf::from).collect();
    test_paths.sort();
    let mut config_paths: Vec<PathBuf> = config_paths.into_iter().map(PathBuf::from).collect();
    config_paths.sort();
    let mut build_files: Vec<PathBuf> = build_files.into_iter().map(PathBuf::from).collect();
    build_files.sort();

    let entry_points = detect_entry_points(project_path);
    let vendor_dirs = detect_vendor_dirs(project_path);
    let risk_signals = collect_risk_signals(config, &test_paths, &config_paths, &build_files);

    RepoManifest {
        languages,
        entry_points,
        test_paths: rel_strings(&test_paths),
        config_paths: rel_strings(&config_paths),
        vendor_dirs,
        build_files: rel_strings(&build_files),
        files,
        loc,
        package_info,
        risk_signals,
        truncated,
    }
}

fn rel_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect()
}

pub(crate) fn language_for(rel: &Path) -> Option<&'static str> {
    match rel.extension().and_then(|e| e.to_str()) {
        Some("rs") => Some("Rust"),
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => Some("JavaScript"),
        Some("ts") | Some("tsx") | Some("mts") => Some("TypeScript"),
        Some("py") => Some("Python"),
        Some("go") => Some("Go"),
        Some("java") => Some("Java"),
        Some("rb") => Some("Ruby"),
        Some("php") => Some("PHP"),
        Some("c") | Some("h") => Some("C"),
        Some("cpp") | Some("hpp") | Some("cc") => Some("C++"),
        Some("cs") => Some("C#"),
        Some("swift") => Some("Swift"),
        Some("kt") | Some("kts") => Some("Kotlin"),
        Some("sh") | Some("bash") => Some("Shell"),
        _ => None,
    }
}

fn is_test_path(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    lower.starts_with("tests/")
        || lower.starts_with("test/")
        || lower.starts_with("__tests__/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.contains("/__tests__/")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.py")
        || lower.ends_with("_test.go")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.starts_with("tests.")
        || lower.starts_with("test_")
}

fn detect_entry_points(project_path: &Path) -> Vec<String> {
    let mut out: Vec<String> = ENTRY_CANDIDATES
        .iter()
        .filter(|c| project_path.join(c).is_file())
        .map(|c| c.to_string())
        .collect();
    // Any top-level `main.*` / `app.*` / `lib.*` at depth <= 2.
    let walker = WalkDir::new(project_path)
        .max_depth(2)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && !VENDOR_DIRS.contains(&name.as_ref())
        });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let stem = name.split('.').next().unwrap_or("");
        if matches!(stem, "main" | "app" | "lib" | "index" | "server") {
            let rel = entry
                .path()
                .strip_prefix(project_path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if !out.contains(&rel) {
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

fn detect_vendor_dirs(project_path: &Path) -> Vec<String> {
    let mut out: Vec<String> = VENDOR_DIRS
        .iter()
        .filter(|d| *d != &".git" && project_path.join(d).is_dir())
        .map(|d| d.to_string())
        .collect();
    out.sort();
    out
}

fn collect_risk_signals(
    config: &NikiConfig,
    test_paths: &[PathBuf],
    config_paths: &[PathBuf],
    build_files: &[PathBuf],
) -> Vec<RiskSignal> {
    let mut out = Vec::new();
    let paths: Vec<String> = test_paths
        .iter()
        .chain(config_paths)
        .chain(build_files)
        .map(|p| p.to_string_lossy().to_lowercase())
        .collect();
    for pattern in &config.risk.denylist_patterns {
        let needle = pattern.to_lowercase();
        for path in &paths {
            if path.contains(&needle) {
                out.push(RiskSignal {
                    rule: "denylist".to_string(),
                    pattern: pattern.clone(),
                    detail: path.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.pattern, &a.detail).cmp(&(&b.pattern, &b.detail)));
    out.dedup_by(|a, b| a.pattern == b.pattern && a.detail == b.detail);
    out
}

fn parse_cargo_deps(path: &Path, rel: &str) -> PackageSummary {
    let mut dependencies = Vec::new();
    if let Ok(content) = std::fs::read_to_string(path)
        && let Ok(value) = content.parse::<toml::Value>()
        && let Some(table) = value.get("dependencies").and_then(|v| v.as_table())
    {
        dependencies.extend(table.keys().cloned());
    }
    dependencies.sort();
    PackageSummary {
        manager: "Cargo.toml".to_string(),
        file_path: rel.to_string(),
        dependencies,
    }
}

fn parse_npm_deps(path: &Path, rel: &str) -> PackageSummary {
    let mut dependencies = Vec::new();
    if let Ok(content) = std::fs::read_to_string(path)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(table) = value.get("dependencies").and_then(|v| v.as_object())
    {
        dependencies.extend(table.keys().cloned());
    }
    dependencies.sort();
    PackageSummary {
        manager: "package.json".to_string(),
        file_path: rel.to_string(),
        dependencies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("node_modules/dep")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/auth.rs"), "pub fn login() {}\n").unwrap();
        fs::write(root.join("tests/basic.rs"), "#[test]\nfn t() {}\n").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        fs::write(root.join("node_modules/dep/index.js"), "x".repeat(10000)).unwrap();
        tmp
    }

    #[test]
    fn manifest_detects_languages_entries_and_vendor() {
        let tmp = fixture();
        let manifest = build_manifest(tmp.path(), &NikiConfig::default());
        assert!(manifest.languages.contains(&"Rust".to_string()));
        assert!(manifest.entry_points.contains(&"src/main.rs".to_string()));
        assert!(manifest.test_paths.iter().any(|p| p.contains("tests/")));
        assert!(manifest.build_files.contains(&"Cargo.toml".to_string()));
        assert!(manifest.vendor_dirs.contains(&"node_modules".to_string()));
        // Vendor contents never counted.
        assert_eq!(manifest.files, 4);
        assert!(manifest.loc >= 4);
        assert!(!manifest.truncated);
    }

    #[test]
    fn manifest_package_info_and_risk_signals() {
        let tmp = fixture();
        let manifest = build_manifest(tmp.path(), &NikiConfig::default());
        let cargo = manifest
            .package_info
            .iter()
            .find(|p| p.manager == "Cargo.toml")
            .unwrap();
        assert!(cargo.dependencies.contains(&"serde".to_string()));
        // Default denylist contains "auth" and src/auth.rs is an entry-adjacent
        // path... risk signals only cover test/config/build paths, so none here.
        assert!(manifest.risk_signals.is_empty());
    }

    #[test]
    fn manifest_truncates_at_max_units() {
        let tmp = fixture();
        let mut config = NikiConfig::default();
        config.repo_intel.max_units = 2;
        let manifest = build_manifest(tmp.path(), &config);
        assert!(manifest.truncated);
        assert_eq!(manifest.files, 2);
    }

    #[test]
    fn manifest_on_missing_dir_is_empty_not_error() {
        let manifest = build_manifest(
            Path::new("/nonexistent/niki-manifest-probe"),
            &NikiConfig::default(),
        );
        assert_eq!(manifest.files, 0);
        assert!(manifest.languages.is_empty());
        assert!(!manifest.truncated);
    }
}
