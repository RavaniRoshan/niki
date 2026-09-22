use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

use crate::artifacts::types::AgentRole;
use crate::config::{DockerConfig, SecurityPolicyConfig};
use crate::permissions::PermissionAction;
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
    /// Owning repo, for Drop-time `worktree remove` without path surgery.
    source_repo: PathBuf,
    /// Set by `destroy()`; Drop only cleans up when explicit destroy was
    /// skipped (panic/error paths).
    destroyed: AtomicBool,
    policy: SecurityPolicyConfig,
    permission_checker: crate::permissions::PermissionChecker,
    event_tx: std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>,
}

impl WorktreeSandbox {
    pub async fn create(
        agent_role: AgentRole,
        source_repo: &Path,
        task_id: &Uuid,
        _config: &DockerConfig,
        niki_config: &crate::config::NikiConfig,
        policy: SecurityPolicyConfig,
        event_tx: std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>,
    ) -> Result<Self> {
        let base = source_repo.join(".niki-worktrees");
        std::fs::create_dir_all(&base)?;

        // Prune stale worktrees from crashed or interrupted prior runs (>24h old)
        let _ = cleanup_stale_worktrees(source_repo, std::time::Duration::from_secs(86400));

        // Phase 5.2: same-task-id collision fails loudly instead of deleting
        // the other run's dir. A registered (live) worktree at the exact path
        // means a concurrent run owns this id. An unregistered leftover is
        // crash debris and is safe to clear.
        let first = base.join(task_id.to_string());
        if first.exists() {
            if is_registered_worktree(source_repo, &first) {
                anyhow::bail!(
                    "worktree for task {task_id} already exists (concurrent run with the same task id?)"
                );
            }
            let _ = std::fs::remove_dir_all(&first);
        }

        // Blocking git operation — run off the async runtime.
        // `git worktree add` can still lose a race with a parallel coder
        // sharing this task id — suffix and retry instead of clobbering.
        let repo = source_repo.to_path_buf();
        let tid = task_id.to_string();
        let mut attempt = 0u32;
        let wt = loop {
            let candidate = if attempt == 0 {
                first.clone()
            } else {
                base.join(format!("{tid}-{attempt}"))
            };
            // Capture (don't inherit) child output: `git worktree add` prints
            // informational lines ("Preparing worktree...") that would otherwise
            // leak onto our stdout and break `--output-format json` pipe-purity.
            let repo_clone = repo.clone();
            let cand_clone = candidate.clone();
            let status = tokio::task::spawn_blocking(move || {
                Command::new("git")
                    .arg("-C")
                    .arg(&repo_clone)
                    .arg("worktree")
                    .arg("add")
                    .arg("--force")
                    .arg(&cand_clone)
                    .arg("HEAD")
                    .output()
                    .map(|o| o.status)
            })
            .await
            .map_err(|e| anyhow!("worktree spawn failed: {e}"))?;

            match status {
                Ok(s) if s.success() => break candidate,
                _ => {
                    // Lost a race (path taken concurrently) → suffix, retry.
                    // A genuinely broken repo fails on a free path → bail.
                    if is_registered_worktree(&repo, &candidate) || candidate.exists() {
                        attempt += 1;
                        if attempt > 100 {
                            anyhow::bail!(
                                "worktree for task {tid} is contended after 100 attempts"
                            );
                        }
                        continue;
                    }
                    anyhow::bail!(
                        "Failed to create git worktree at {} (is the project a git repo?)",
                        candidate.display()
                    );
                }
            }
        };

        Ok(Self {
            worktree_path: wt,
            agent_role,
            task_id: task_id.to_string(),
            source_repo: source_repo.to_path_buf(),
            destroyed: AtomicBool::new(false),
            policy: policy.clone(),
            permission_checker: crate::sandbox::build_permission_checker(&policy, niki_config),
            event_tx,
        })
    }
}

/// True when `path` is a registered worktree of `repo` (live or leaked
/// registration — either way, owned by someone).
fn is_registered_worktree(repo: &Path, path: &Path) -> bool {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "list", "--porcelain"])
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let needle = format!("worktree {}", path.display());
            text.lines().any(|l| l == needle)
        }
        Err(_) => false,
    }
}

/// Remove every worktree dir belonging to `task_id`: the exact
/// `.niki-worktrees/<id>` dir plus suffixed parallel-coder siblings
/// (`<id>-1`, …). Used by the Ctrl+C/SIGTERM handlers, which cannot track
/// per-sandbox paths. Returns the number of dirs removed.
pub fn cleanup_worktrees_for_task(source_repo: &Path, task_id: &str) -> usize {
    let base = source_repo.join(".niki-worktrees");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return 0;
    };
    let suffixed = format!("{task_id}-");
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name != task_id && !name.starts_with(&suffixed) {
            continue;
        }
        let path = entry.path();
        let _ = Command::new("git")
            .arg("-C")
            .arg(source_repo)
            .args(["worktree", "remove", "--force"])
            .arg(&path)
            .output();
        if std::fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
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
                // Phase 5.1 partial-apply semantics: all-or-nothing per stage.
                // A stage with ANY unmatched block writes NOTHING, so a failed
                // stage never leaves a half-applied worktree behind.
                if !unmatched.is_empty() {
                    return Err(anyhow!(
                        "No edit block matched its target file in the worktree ({} unmatched); nothing was written",
                        unmatched.len()
                    ));
                }
                for file_path in changed_files {
                    if let Some(content) = contents.get(&file_path) {
                        std::fs::write(&file_path, content)?;
                    }
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
        let normalized = crate::output::git::normalize_patch(patch);
        let wt = self.worktree_path.clone();
        let patch_text = normalized.clone();
        tokio::task::spawn_blocking(move || Self::apply_in_worktree(&wt, &patch_text)).await?
    }

    async fn get_diff(&self, agent_files: &[String]) -> Result<String> {
        // Phase 5.1: scope to agent-reported files (intent-to-add just those),
        // so brand-new agent files appear in the diff while worktree-local
        // byproducts (caches, tool output) stay out.
        let wt = self.worktree_path.clone();
        let files: Vec<String> = agent_files
            .iter()
            .filter(|s| {
                !s.is_empty()
                    && !s.starts_with('.')
                    && !s.starts_with('/')
                    && !s.contains("..")
                    && *s != "niki.toml"
            })
            .cloned()
            .collect();
        tokio::task::spawn_blocking(move || -> Result<String> {
            if files.is_empty() {
                return Ok(String::new());
            }
            let wt_str = wt
                .to_str()
                .ok_or_else(|| anyhow!("worktree path is not valid UTF-8"))?;
            let mut add_args = vec!["-C", wt_str, "add", "-N", "--"];
            let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
            add_args.extend(refs.iter().copied());
            let _ = Command::new("git").args(&add_args).output();
            let mut diff_args = vec!["-C", wt_str, "diff", "--"];
            diff_args.extend(refs);
            let out = Command::new("git").args(&diff_args).output()?;
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
            // F1b: Enforce granular permission policy (dead-island PermissionChecker).
            let full = cmd.join(" ");
            match self.permission_checker.check_command(&full) {
                crate::permissions::Permission::Deny => {
                    return Err(anyhow!("Command denied by permission policy: '{}'", full));
                }
                crate::permissions::Permission::Ask => {
                    let (response_tx, response_rx) = std::sync::mpsc::channel();
                    let request = crate::display::tui::DisplayEvent::PermissionRequest {
                        command: full.clone(),
                        response_tx,
                    };
                    if self.event_tx.send(request).is_err() {
                        if self.permission_checker.fail_closed_headless() {
                            return Err(anyhow::anyhow!(
                                "Command denied by policy (headless Ask with fail_closed_headless): '{}'",
                                full
                            ));
                        }
                        // No TUI listening — fall back to Allow (headless mode).
                        // Loud by design: silent auto-approval is how agents end
                        // up running `curl | sh` in CI. Use --permission-mode to
                        // make the posture explicit, or run attached to review.
                        tracing::warn!(
                            target: "niki::permissions",
                            command = full.as_str(),
                            "no TUI listening — Ask fell back to Allow (headless). Pass --permission-mode explicitly to silence this per-run posture."
                        );
                    } else {
                        let action = tokio::task::block_in_place(|| {
                            response_rx.recv_timeout(std::time::Duration::from_secs(5))
                        })
                        .unwrap_or(PermissionAction::Deny);
                        if matches!(action, PermissionAction::Deny) {
                            return Err(anyhow!("Command denied by user: '{}'", full));
                        }
                    }
                }
                crate::permissions::Permission::Allow => {}
            }
        }
        let wt = self.worktree_path.clone();
        let cmd: Vec<String> = cmd.iter().map(|s| s.to_string()).collect();
        let timeout = std::time::Duration::from_secs(self.policy.max_exec_seconds);
        // F3: Enforce exec timeout and process group isolation.
        tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || -> Result<ExecOutput> {
                let mut c = Command::new(&cmd[0]);
                c.args(&cmd[1..]).current_dir(&wt);
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    c.process_group(0);
                }
                let output = c.output()?;
                let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let raw_stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = crate::sandbox::truncate_head_tail(&raw_stdout, 1500, 65536);
                let stderr = crate::sandbox::truncate_head_tail(&raw_stderr, 1500, 65536);
                Ok(ExecOutput {
                    exit_code: output.status.code().unwrap_or(0) as i64,
                    stdout,
                    stderr,
                })
            }),
        )
        .await
        .map_err(|_| anyhow!("exec timed out after {}s", self.policy.max_exec_seconds))?
        .map_err(|e| anyhow!("exec spawn failed: {e}"))?
    }

    async fn destroy(&self) -> Result<()> {
        // Mark first: Drop must not repeat an explicit teardown even when the
        // removal below partially fails (the 24h prune is the backstop).
        self.destroyed.store(true, Ordering::SeqCst);
        let wt = self.worktree_path.clone();
        let task_id = self.task_id.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            // `git worktree remove` deletes the worktree dir; `--force` allows
            // removal even with uncommitted changes (the merged diff is already
            // captured via get_diff before destroy is called).
            // Output is captured, not inherited: a half-removed worktree
            // prints `fatal: ... is not a working tree`, which is expected
            // noise during teardown, not a user-facing error.
            let _ = Command::new("git")
                .arg("worktree")
                .arg("remove")
                .arg("--force")
                .arg(&wt)
                .output();
            let _ = Command::new("git").arg("worktree").arg("prune").output();
            let _ = std::fs::remove_dir_all(&wt);
            let _ = task_id;
            Ok(())
        })
        .await
        .map_err(|e| anyhow!("destroy spawn failed: {e}"))?
    }
}

impl Drop for WorktreeSandbox {
    /// Best-effort teardown for panic/error paths that skip `destroy()`.
    /// Synchronous by necessity; failures are ignored (the 24h prune is the
    /// backstop). Skipped entirely after an explicit `destroy()`.
    fn drop(&mut self) {
        if self.destroyed.load(Ordering::SeqCst) {
            return;
        }
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.source_repo)
            .args(["worktree", "remove", "--force"])
            .arg(&self.worktree_path)
            .output();
        let _ = std::fs::remove_dir_all(&self.worktree_path);
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

/// True when any file under `path` was modified within `max_age` — i.e. the
/// worktree is active and must not be pruned, however old its top-level dir.
fn is_active_worktree(
    path: &Path,
    now: std::time::SystemTime,
    max_age: std::time::Duration,
) -> bool {
    walkdir::WalkDir::new(path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .any(|t| now.duration_since(t).is_ok_and(|d| d <= max_age))
}

/// Scan the `.niki-worktrees` directory for stale worktrees older than `max_age` and prune them.
pub fn cleanup_stale_worktrees(source_repo: &Path, max_age: std::time::Duration) -> usize {
    let base = source_repo.join(".niki-worktrees");
    if !base.is_dir() {
        return 0;
    }

    let Ok(entries) = std::fs::read_dir(&base) else {
        return 0;
    };

    let mut cleaned = 0;
    let now = std::time::SystemTime::now();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let top_stale = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| now.duration_since(t).ok())
                .map(|dur| dur > max_age)
                .unwrap_or(false);
            // Phase 5.2: top-level dir mtime lies for old-but-active
            // worktrees (the dir entry itself rarely changes) — confirm with
            // the recursive newest mtime before pruning, so another run's
            // active worktree is never deleted out from under it.
            let is_stale = top_stale && !is_active_worktree(&path, now, max_age);

            if is_stale {
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(source_repo)
                    .arg("worktree")
                    .arg("remove")
                    .arg("--force")
                    .arg(&path)
                    .status();
                let _ = std::fs::remove_dir_all(&path);
                cleaned += 1;
            }
        }
    }

    if cleaned > 0 {
        let _ = Command::new("git")
            .arg("-C")
            .arg(source_repo)
            .arg("worktree")
            .arg("prune")
            .status();
    }

    cleaned
}
