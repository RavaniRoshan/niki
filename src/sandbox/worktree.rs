use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::config::{DockerConfig, SecurityPolicyConfig};
use crate::sandbox::{ExecOutput, Sandbox, check_command_policy};

/// Lightweight sandbox using a `git worktree` of the project plus local
/// `std::process::Command` execution. No Docker daemon required.
///
/// Each stage gets its own worktree (a separate working directory checked out at
/// the current HEAD), so concurrent stages (e.g. parallel Coders) never touch
/// the same files. The change lives only in the worktree; the host project is
/// left untouched until the run's end, when the merged diff is applied back.
pub struct WorktreeSandbox {
    pub worktree_path: PathBuf,
    pub agent_role: AgentRole,
    task_id: String,
    policy: SecurityPolicyConfig,
}

impl WorktreeSandbox {
    pub async fn create(
        agent_role: AgentRole,
        source_repo: &Path,
        task_id: &Uuid,
        _config: &DockerConfig,
        policy: SecurityPolicyConfig,
    ) -> Result<Self> {
        let base = source_repo.join(".niki-worktrees");
        std::fs::create_dir_all(&base)?;
        let wt = base.join(task_id.to_string());
        if wt.exists() {
            let _ = std::fs::remove_dir_all(&wt);
        }

        // Blocking git operation — run off the async runtime.
        let repo = source_repo.to_path_buf();
        let wt_clone = wt.clone();
        let status = tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .arg("worktree")
                .arg("add")
                .arg("--force")
                .arg(&wt_clone)
                .arg("HEAD")
                .status()
        })
        .await
        .map_err(|e| anyhow!("worktree spawn failed: {e}"))?;

        match status {
            Ok(s) if s.success() => Ok(Self {
                worktree_path: wt,
                agent_role,
                task_id: task_id.to_string(),
                policy,
            }),
            _ => Err(anyhow!(
                "Failed to create git worktree at {} (is the project a git repo?)",
                wt.display()
            )),
        }
    }

    /// Normalize a diff: unify CRLF→LF and guarantee a trailing newline, matching
    /// the Docker sandbox's `normalize_patch` so `git apply` never chokes on the
    /// last context line.
    fn normalize_patch(patch: &str) -> String {
        let mut s = patch.replace("\r\n", "\n");
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s
    }
}

#[async_trait]
impl Sandbox for WorktreeSandbox {
    async fn ensure_tools(&self, tools: &[String]) -> Result<()> {
        let mut missing = Vec::new();
        for t in tools {
            let tool = t.clone();
            let ok = tokio::task::spawn_blocking(move || {
                Command::new("sh")
                    .arg("-c")
                    .arg(format!("command -v \"{}\" >/dev/null 2>&1", tool))
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .await
            .map_err(|e| anyhow!("tool check spawn failed: {e}"))?;
            if !ok {
                missing.push(t.clone());
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "Sandbox (worktree) is missing required tools: {}",
                missing.join(", ")
            ))
        }
    }

    async fn apply_patch(&self, patch: &str, _host_workspace: &Path) -> Result<()> {
        // Try edit format (SEARCH/REPLACE blocks) first
        let edit_blocks = crate::sandbox::edit_format::parse_edit_blocks(patch);
        if !edit_blocks.is_empty() {
            let wt = self.worktree_path.clone();
            return tokio::task::spawn_blocking(move || -> Result<()> {
                let files = find_files_in_worktree(&wt)?;
                let paths: Vec<std::path::PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
                let mut unmatched: Vec<usize> = (0..edit_blocks.len()).collect();
                let mut changed_files: std::collections::HashSet<std::path::PathBuf> =
                    std::collections::HashSet::new();
                let mut contents: std::collections::HashMap<std::path::PathBuf, String> =
                    std::collections::HashMap::new();
                for (path, content) in &files {
                    contents.insert(path.clone(), content.clone());
                }

                for (i, block) in edit_blocks.iter().enumerate() {
                    // Bound blocks apply only to their target file; unbound blocks
                    // fall back to the cross-file search. See research report S4.
                    let targets: Vec<std::path::PathBuf> = match &block.file {
                        Some(target) => paths
                            .iter()
                            .filter(|f| {
                                let s = f.to_string_lossy();
                                &s == target
                                    || s.ends_with(target)
                                    || s.ends_with(&format!("/{target}"))
                            })
                            .cloned()
                            .collect(),
                        None => paths.clone(),
                    };
                    let mut applied = false;
                    for file_path in targets {
                        if let Some(content) = contents.get(&file_path) {
                            if let Some(new_content) =
                                crate::sandbox::edit_format::apply_single_edit_block(
                                    content,
                                    block.search.as_str(),
                                    block.replace.as_str(),
                                )?
                            {
                                contents.insert(file_path.clone(), new_content);
                                applied = true;
                                changed_files.insert(file_path);
                            }
                        }
                    }
                    if applied {
                        unmatched.retain(|&idx| idx != i);
                    }
                }
                for file_path in changed_files {
                    if let Some(content) = contents.get(&file_path) {
                        std::fs::write(&file_path, content)?;
                    }
                }
                if !unmatched.is_empty() {
                    return Err(anyhow!(
                        "No edit block matched its target file in the worktree ({} unmatched)",
                        unmatched.len()
                    ));
                }
                Ok(())
            })
            .await
            .map_err(|e| anyhow!("edit apply spawn failed: {e}"))?;
        }

        // Fall back to unified diff format. If the text isn't a diff at all
        // (e.g. an empty edits array), treat it as a no-op rather than failing.
        let looks_like_diff =
            patch.contains("diff --git") || patch.contains("--- a/") || patch.contains("+++ b/");
        if !looks_like_diff {
            return Ok(());
        }
        let normalized = Self::normalize_patch(patch);
        let wt = self.worktree_path.clone();
        let patch_text = normalized.clone();
        tokio::task::spawn_blocking(move || Self::apply_in_worktree(&wt, &patch_text)).await?
    }

    async fn get_diff(&self) -> Result<String> {
        let wt = self.worktree_path.clone();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let out = Command::new("git")
                .arg("-C")
                .arg(&wt)
                .args(["diff", "--", ".", ":(exclude).niki", ":(exclude)niki.toml"])
                .output()?;
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        })
        .await
        .map_err(|e| anyhow!("diff spawn failed: {e}"))?
    }

    async fn exec(&self, cmd: &[&str], role: Option<&AgentRole>) -> Result<ExecOutput> {
        if cmd.is_empty() {
            return Err(anyhow!("empty command"));
        }
        // F1: Enforce security policy when a role is supplied.
        if role.is_some() {
            check_command_policy(cmd, &self.policy)?;
        }
        let wt = self.worktree_path.clone();
        let cmd: Vec<String> = cmd.iter().map(|s| s.to_string()).collect();
        let timeout = std::time::Duration::from_secs(self.policy.max_exec_seconds);
        // F3: Enforce exec timeout.
        tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || -> Result<ExecOutput> {
                let output = Command::new(&cmd[0])
                    .args(&cmd[1..])
                    .current_dir(&wt)
                    .output()?;
                Ok(ExecOutput {
                    exit_code: output.status.code().unwrap_or(0) as i64,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            }),
        )
        .await
        .map_err(|_| anyhow!("exec timed out after {}s", self.policy.max_exec_seconds))?
        .map_err(|e| anyhow!("exec spawn failed: {e}"))?
    }

    async fn destroy(&self) -> Result<()> {
        let wt = self.worktree_path.clone();
        let task_id = self.task_id.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            // `git worktree remove` deletes the worktree dir; `--force` allows
            // removal even with uncommitted changes (the merged diff is already
            // captured via get_diff before destroy is called).
            let _ = Command::new("git")
                .arg("worktree")
                .arg("remove")
                .arg("--force")
                .arg(&wt)
                .status();
            let _ = Command::new("git").arg("worktree").arg("prune").status();
            let _ = std::fs::remove_dir_all(&wt);
            let _ = task_id;
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("destroy spawn failed: {e}"))?
    }
}

impl WorktreeSandbox {
    /// Apply a unified diff inside the worktree, with a `patch -p1` fallback
    /// (mirrors the Docker sandbox's apply_patch). Runs on a blocking thread.
    fn apply_in_worktree(wt: &Path, patch: &str) -> Result<()> {
        let p = wt.join(".niki-tmp.patch");
        std::fs::write(&p, patch)?;
        let res = Command::new("git")
            .arg("-C")
            .arg(wt)
            .arg("apply")
            .arg(&p)
            .status();
        let _ = std::fs::remove_file(&p);

        match res {
            Ok(s) if s.success() => Ok(()),
            // No `patch -p1` fallback: it does not guard against `../` or absolute
            // paths in the diff, which is a path-traversal risk. `git apply` is the
            // only accepted method. See report S3.
            _ => Err(anyhow!("Failed to apply patch (git apply only)")),
        }
    }
}

/// Find all tracked files in the worktree and return their contents.
fn find_files_in_worktree(wt: &Path) -> Result<Vec<(std::path::PathBuf, String)>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(["ls-files", "--cached", "--exclude-standard"])
        .output()?;

    let files = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();

    for file in files.lines() {
        let path = wt.join(file);
        if path.is_file()
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            result.push((path, content));
        }
    }

    Ok(result)
}
