//! Project knowledge base: a Markdown + JSON sidecar store with explicit
//! provenance stamped into storage.
//!
//! Layout under `<output_dir>/kb/`:
//!
//! ```text
//! kb/
//!   manifest.json      provenance for the whole KB (snapshot, generator)
//!   architecture.md    repo shape, entry points, recent learnings
//!   history.md         mined history summary
//!   dependencies.json  provenance-wrapped dependency inventory
//!   entities/<slug>.md one file per top-level code area
//! ```
//!
//! Every Markdown file starts with a `<!-- KB_SNAPSHOT: … -->` first line so
//! future runs can tell what repo state produced it. All writes are atomic
//! (temp file + rename), so a crashed build never leaves half a KB.

use crate::config::NikiConfig;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Provenance tier for stored knowledge.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Authority {
    Authoritative,
    Inferred,
    Advisory,
}

/// Provenance wrapper for JSON sidecars.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sidecar<T: Serialize> {
    pub generated_by: String,
    pub generated_at: DateTime<Utc>,
    /// Snapshot anchor (`niki-task-<8 hex>`) this data was derived under.
    pub state_ref: String,
    pub authority: Authority,
    pub payload: T,
}

/// One dependency inventory row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRow {
    pub manager: String,
    pub file_path: String,
    pub dependencies: Vec<String>,
}

/// KB-level manifest (`kb/manifest.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbManifest {
    pub snapshot_id: String,
    pub commit_sha: Option<String>,
    pub generated_at: DateTime<Utc>,
    pub generated_by: String,
    pub files: Vec<String>,
}

/// Root of the KB store, honoring `general.output_dir`.
pub fn kb_root(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path.join(&config.general.output_dir).join("kb")
}

/// First-line provenance stamp for every KB Markdown file.
pub fn snapshot_header(commit_sha: Option<&str>, generated_by: &str) -> String {
    format!(
        "<!-- KB_SNAPSHOT: commit={} generated={} by={} -->",
        commit_sha.unwrap_or("nongit"),
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        generated_by
    )
}

/// Atomic write: temp file in the same directory + rename, so readers never
/// see a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp-niki-atomic");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Write a Markdown KB file with the snapshot stamp as its first line.
pub fn write_kb_markdown(
    path: &Path,
    commit_sha: Option<&str>,
    generated_by: &str,
    body: &str,
) -> Result<()> {
    let mut out = snapshot_header(commit_sha, generated_by);
    out.push('\n');
    out.push_str(body);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    write_atomic(path, out.as_bytes())
}

/// Read the snapshot stamp (first line) of a KB Markdown file, if present.
pub fn read_snapshot_stamp(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .next()
        .filter(|line| line.starts_with("<!-- KB_SNAPSHOT:"))
        .map(|line| line.to_string())
}

/// Ensure the KB directory layout exists (empty placeholders with stamps when
/// files are missing — a missed run gets an empty KB, never an error).
pub fn ensure_layout(
    project_path: &Path,
    config: &NikiConfig,
    commit_sha: Option<&str>,
    generated_by: &str,
) -> Result<PathBuf> {
    let root = kb_root(project_path, config);
    std::fs::create_dir_all(root.join("entities"))?;
    for (name, placeholder) in [
        (
            "architecture.md",
            "# Architecture\n\n(empty — run `niki architecture build`)\n",
        ),
        ("history.md", "# History\n\n(no history mined yet)\n"),
    ] {
        let path = root.join(name);
        if !path.exists() {
            write_kb_markdown(&path, commit_sha, generated_by, placeholder)?;
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_roundtrip_carries_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("architecture.md");
        write_kb_markdown(&path, Some("abc123"), "test", "# A\n").unwrap();
        let stamp = read_snapshot_stamp(&path).unwrap();
        assert!(stamp.starts_with("<!-- KB_SNAPSHOT:"));
        assert!(stamp.contains("commit=abc123"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content
                .lines()
                .next()
                .unwrap()
                .starts_with("<!-- KB_SNAPSHOT:")
        );
    }

    #[test]
    fn atomic_write_never_leaves_partial() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sub").join("f.json");
        write_atomic(&path, b"{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}");
        assert!(
            std::fs::read_dir(tmp.path().join("sub")).unwrap().count() == 1,
            "no temp file left behind"
        );
    }

    #[test]
    fn sidecar_serializes_provenance() {
        let sidecar = Sidecar {
            generated_by: "architecture-build".to_string(),
            generated_at: Utc::now(),
            state_ref: "niki-task-abc123".to_string(),
            authority: Authority::Inferred,
            payload: vec![DependencyRow {
                manager: "Cargo.toml".to_string(),
                file_path: "Cargo.toml".to_string(),
                dependencies: vec!["serde".to_string()],
            }],
        };
        let json = serde_json::to_string(&sidecar).unwrap();
        assert!(json.contains("\"authority\":\"inferred\""));
        assert!(json.contains("niki-task-abc123"));
    }

    #[test]
    fn ensure_layout_creates_stamped_placeholders() {
        let tmp = tempfile::tempdir().unwrap();
        let root = ensure_layout(tmp.path(), &NikiConfig::default(), None, "test").unwrap();
        assert!(root.join("entities").is_dir());
        for name in ["architecture.md", "history.md"] {
            let stamp = read_snapshot_stamp(&root.join(name)).unwrap();
            assert!(stamp.contains("commit=nongit"));
        }
    }
}
