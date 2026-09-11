//! Run provenance: answers "what exact repo state did this run reason about?"
//!
//! A [`RunManifest`] is written to `.niki/tasks/<id>/manifest.json` after
//! indexing and before the Planner, updated on completion with the result
//! branch, and surfaced via `niki status --with-provenance`. Every step is
//! best-effort: provenance bookkeeping never fails a run.

use crate::artifacts::types::AgentRole;
use crate::config::NikiConfig;
use crate::config::types::PipelineStageConfig;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// The on-disk provenance record for one run (`manifest.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: Uuid,
    /// Stage roles in execution order (Planner first).
    pub agent_roles: Vec<AgentRole>,
    pub created_at: DateTime<Utc>,
    pub repo_identity: RepoIdentity,
    pub active_snapshot: SnapshotRef,
    pub config_fingerprint: ConfigFingerprint,
    pub toolchain: ToolchainVersions,
    /// Result branch (`niki/<id>`), filled on completion.
    #[serde(default)]
    pub branch: Option<String>,
    /// Commit the result branch points at, filled on completion.
    #[serde(default)]
    pub commit_sha: Option<String>,
    /// Artifact roles in execution order, filled on completion.
    #[serde(default)]
    pub artifact_roles: Vec<AgentRole>,
    /// Summed stage cost estimate in USD, filled on completion.
    #[serde(default)]
    pub total_cost_usd: f64,
    /// True when the run stopped after the Planner (no branch by design).
    #[serde(default)]
    pub dry_run: bool,
}

/// What repo state the run observed before executing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIdentity {
    /// HEAD commit at capture time (`None` outside a git repo).
    pub commit_sha: Option<String>,
    /// Current branch name, if any.
    pub branch: Option<String>,
    /// `origin` remote URL, if configured.
    pub remote_url: Option<String>,
    /// True when the working tree had uncommitted changes.
    pub dirty: bool,
    /// Stable fingerprint of the workdir when not in a git repo.
    pub workdir_fingerprint: Option<String>,
}

/// The snapshot anchor for this run. The `niki/<id>` branch is the
/// authoritative anchor; this record is the advisory pointer to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRef {
    /// HEAD commit captured read-only (`None` outside a git repo).
    pub commit_sha: Option<String>,
    /// `"branch"` (clean tree), `"dirty"` (uncommitted changes), or `"nongit"`.
    pub kind: String,
    /// Stable id of the form `niki-task-<8 hex>`.
    pub snapshot_id: String,
}

/// Which config the run loaded, content-hashed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFingerprint {
    /// Local `niki.toml` path that was loaded, if present.
    pub path: Option<String>,
    /// Stable non-crypto (FNV-1a) hash of its bytes, if present.
    pub content_hash: Option<String>,
}

/// Toolchain versions pinned to the run for later reproduction queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainVersions {
    pub niki: String,
    pub rustc: Option<String>,
}

/// Capture the current repo/config/toolchain state. Never fails: git absence,
/// unreadable config, and missing toolchains degrade to `None` fields.
pub fn capture(
    _config: &NikiConfig,
    project_path: &Path,
    task_id: &Uuid,
    stages: &[PipelineStageConfig],
) -> RunManifest {
    let repo_identity = read_repo_identity(project_path);
    let snapshot_id = format!("niki-task-{}", &task_id.to_string()[..8]);
    let kind = match (&repo_identity.commit_sha, repo_identity.dirty) {
        (None, _) => "nongit",
        (Some(_), true) => "dirty",
        (Some(_), false) => "branch",
    }
    .to_string();
    RunManifest {
        run_id: *task_id,
        agent_roles: stages.iter().map(|s| s.role).collect(),
        created_at: Utc::now(),
        active_snapshot: SnapshotRef {
            commit_sha: repo_identity.commit_sha.clone(),
            kind,
            snapshot_id,
        },
        repo_identity,
        config_fingerprint: fingerprint_config(project_path),
        toolchain: ToolchainVersions {
            niki: env!("CARGO_PKG_VERSION").to_string(),
            rustc: rustc_version(),
        },
        branch: None,
        commit_sha: None,
        artifact_roles: Vec::new(),
        total_cost_usd: 0.0,
        dry_run: false,
    }
}

/// Write `manifest.json` into `task_dir` (created if missing).
pub fn write_manifest(task_dir: &Path, manifest: &RunManifest) -> Result<()> {
    std::fs::create_dir_all(task_dir)?;
    let path = task_dir.join("manifest.json");
    std::fs::write(path, serde_json::to_string_pretty(manifest)?)?;
    Ok(())
}

/// Load the manifest previously written by [`write_manifest`].
pub fn read_manifest(task_dir: &Path) -> Result<RunManifest> {
    let content = std::fs::read_to_string(task_dir.join("manifest.json"))?;
    Ok(serde_json::from_str(&content)?)
}

/// Best-effort completion update: stamps the result branch, its commit, the
/// artifact roles, and the summed cost. Never fails — warnings go to stderr.
pub fn record_completion(
    task_dir: &Path,
    project_path: &Path,
    branch: Option<&str>,
    artifacts: &[(AgentRole, String)],
    total_cost_usd: f64,
) {
    let mut manifest = match read_manifest(task_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Warning: could not load run manifest for completion update: {e}");
            return;
        }
    };
    manifest.branch = branch.map(|b| b.to_string());
    manifest.commit_sha = branch
        .and_then(|b| resolve_branch_sha(project_path, b))
        .or_else(|| manifest.active_snapshot.commit_sha.clone());
    manifest.artifact_roles = artifacts.iter().map(|(role, _)| *role).collect();
    manifest.total_cost_usd = total_cost_usd;
    if let Err(e) = write_manifest(task_dir, &manifest) {
        eprintln!("Warning: could not update run manifest: {e}");
    }
}

fn read_repo_identity(project_path: &Path) -> RepoIdentity {
    let repo = match git2::Repository::open(project_path) {
        Ok(r) => r,
        Err(_) => {
            return RepoIdentity {
                commit_sha: None,
                branch: None,
                remote_url: None,
                dirty: false,
                workdir_fingerprint: Some(workdir_fingerprint(project_path)),
            };
        }
    };
    let commit_sha = repo
        .revparse_single("HEAD")
        .ok()
        .and_then(|o| o.peel_to_commit().ok())
        .map(|c| c.id().to_string());
    let branch = repo
        .head()
        .ok()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().map(|s| s.to_string()));
    let remote_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|r| r.url().map(|s| s.to_string()));
    let dirty = repo
        .statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .include_ignored(false),
        ))
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    RepoIdentity {
        commit_sha,
        branch,
        remote_url,
        dirty,
        workdir_fingerprint: None,
    }
}

fn fingerprint_config(project_path: &Path) -> ConfigFingerprint {
    let path = project_path.join("niki.toml");
    match std::fs::read(&path) {
        Ok(bytes) => ConfigFingerprint {
            path: Some(path.display().to_string()),
            content_hash: Some(crate::util::fnv1a64_hex(&bytes)),
        },
        Err(_) => ConfigFingerprint {
            path: None,
            content_hash: None,
        },
    }
}

fn workdir_fingerprint(project_path: &Path) -> String {
    let canonical = project_path
        .canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf());
    crate::util::fnv1a64_hex(canonical.to_string_lossy().as_bytes())
}

fn rustc_version() -> Option<String> {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn resolve_branch_sha(project_path: &Path, branch: &str) -> Option<String> {
    git2::Repository::open(project_path)
        .ok()?
        .revparse_single(branch)
        .ok()?
        .peel_to_commit()
        .ok()
        .map(|c| c.id().to_string())
}

/// Snapshot id for manual (non-run) index/KB builds: `niki-task-manual-<8>`
/// anchored on the current HEAD, or `nongit` outside a repo.
pub fn manual_snapshot_id(project_path: &Path) -> String {
    let full: Option<String> = (|| {
        let repo = git2::Repository::open(project_path).ok()?;
        let commit = repo.revparse_single("HEAD").ok()?.peel_to_commit().ok()?;
        Some(commit.id().to_string())
    })();
    format!(
        "niki-task-manual-{}",
        full.as_deref().map(|s| &s[..8]).unwrap_or("nongit")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit_fixture(repo_path: &Path) {
        let repo = git2::Repository::init(repo_path).unwrap();
        std::fs::write(repo_path.join("a.txt"), "hello\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("niki-test", "niki@test").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
    }

    #[test]
    fn capture_on_git_repo_records_head() {
        let tmp = tempfile::tempdir().unwrap();
        commit_fixture(tmp.path());
        let id = Uuid::new_v4();
        let manifest = capture(&NikiConfig::default(), tmp.path(), &id, &[]);
        assert!(manifest.repo_identity.commit_sha.is_some());
        assert!(!manifest.repo_identity.dirty);
        assert_eq!(manifest.active_snapshot.kind, "branch");
        assert_eq!(
            manifest.active_snapshot.snapshot_id,
            format!("niki-task-{}", &id.to_string()[..8])
        );
        assert_eq!(manifest.toolchain.niki, env!("CARGO_PKG_VERSION"));
        assert!(manifest.repo_identity.workdir_fingerprint.is_none());
    }

    #[test]
    fn capture_dirty_tree_marks_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        commit_fixture(tmp.path());
        std::fs::write(tmp.path().join("uncommitted.txt"), "x\n").unwrap();
        let manifest = capture(&NikiConfig::default(), tmp.path(), &Uuid::new_v4(), &[]);
        assert!(manifest.repo_identity.dirty);
        assert_eq!(manifest.active_snapshot.kind, "dirty");
    }

    #[test]
    fn capture_outside_git_falls_back_to_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = capture(&NikiConfig::default(), tmp.path(), &Uuid::new_v4(), &[]);
        assert!(manifest.repo_identity.commit_sha.is_none());
        assert!(manifest.repo_identity.workdir_fingerprint.is_some());
        assert_eq!(manifest.active_snapshot.kind, "nongit");
    }

    #[test]
    fn manifest_roundtrip_and_completion_update() {
        let tmp = tempfile::tempdir().unwrap();
        commit_fixture(tmp.path());
        let task_dir = tmp.path().join("task");
        let id = Uuid::new_v4();
        let manifest = capture(&NikiConfig::default(), tmp.path(), &id, &[]);
        write_manifest(&task_dir, &manifest).unwrap();

        // Simulate the result branch the CLI creates on success.
        let repo = git2::Repository::open(tmp.path()).unwrap();
        let head = repo
            .revparse_single("HEAD")
            .unwrap()
            .peel_to_commit()
            .unwrap();
        repo.branch("niki/abcd1234", &head, false).unwrap();

        record_completion(&task_dir, tmp.path(), Some("niki/abcd1234"), &[], 1.25);
        let updated = read_manifest(&task_dir).unwrap();
        assert_eq!(updated.branch.as_deref(), Some("niki/abcd1234"));
        assert_eq!(
            updated.commit_sha, manifest.repo_identity.commit_sha,
            "branch points at HEAD in this fixture"
        );
        assert_eq!(updated.total_cost_usd, 1.25);
    }

    #[test]
    fn completion_without_manifest_warns_but_never_panics() {
        let tmp = tempfile::tempdir().unwrap();
        record_completion(&tmp.path().join("missing"), tmp.path(), None, &[], 0.0);
    }

    #[test]
    fn workdir_fingerprint_is_stable() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            workdir_fingerprint(tmp.path()),
            workdir_fingerprint(tmp.path())
        );
    }
}
