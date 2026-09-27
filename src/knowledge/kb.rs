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
    #[serde(default)]
    pub generated_by: String,
    #[serde(default)]
    pub generated_at: DateTime<Utc>,
    /// Snapshot anchor (`niki-task-<8 hex>`) this data was derived under.
    #[serde(default)]
    pub state_ref: String,
    #[serde(default = "default_authority")]
    pub authority: Authority,
    pub payload: T,
}

fn default_authority() -> Authority {
    Authority::Advisory
}

/// One dependency inventory row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRow {
    #[serde(default)]
    pub manager: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// KB-level manifest (`kb/manifest.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbManifest {
    #[serde(default)]
    pub snapshot_id: String,
    #[serde(default)]
    pub commit_sha: Option<String>,
    #[serde(default)]
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub generated_by: String,
    #[serde(default)]
    pub files: Vec<String>,
    /// Store schema version; mismatches warn loudly instead of emptying silently.
    #[serde(default = "kb_schema_version")]
    pub schema_version: u32,
}

fn kb_schema_version() -> u32 {
    1
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
///
/// The temp name keeps the *whole* file name and adds a pid + counter. It used
/// to be `with_extension("tmp-niki-atomic")`, which replaced the extension
/// rather than appending to it — so `task.json` became `task.tmp-niki-atomic`
/// and a sibling `task` became the same path. Two writers, or two files that
/// happened to share a stem, would clobber each other's partial write and then
/// rename a truncated file into place. That is the exact failure an atomic
/// write exists to prevent.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "niki".to_string());
    let tmp = parent.join(format!(
        ".{stem}.{}.{}.tmp-niki-atomic",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    // A failed write must not leave the temp file behind for the next run to
    // trip over, and it must not be renamed into place either.
    let result = (|| -> std::io::Result<()> {
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(result?)
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
