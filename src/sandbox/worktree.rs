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
                    // A block with an empty `search` and a named target is a
                    // *creation*: the file does not exist, so there is nothing
                    // to anchor to, and the `replace` is the whole file.
                    //
                    // `files_changed[].action == "create"` is the only thing in
                    // the artifact that can say so, and without this the
                    // contract could express a new file but not produce one —
                    // the `docs` task (write a README) was unexpressible, and
                    // the model was left guessing an anchor for a file that
                    // had no content to copy.
                    if block.search.trim().is_empty() {
                        if block.replace.trim().is_empty() {
                            return Err(anyhow!(
                                "edit block is empty on both sides; it changes nothing"
                            ));
                        }
                        if let Some(target) = &block.file {
                            // The target is model-authored: it comes verbatim
                            // from `CodeDiff.files_changed[].path`, which
                            // `schemas/code_diff.schema.json` types as a bare
                            // string with no pattern.
                            //
                            // `wt.join(target)` therefore honoured an absolute
                            // path (discarding the base entirely) and a `..`
                            // traversal, and this branch then created the
                            // parent directories and wrote the file. A
                            // CodeDiff with one edit whose `search` is empty
                            // and whose target is `/home/user/.bashrc` passed
                            // `validate_artifact` — the semantic layer only
                            // rejects a block empty on *both* sides — and
                            // wrote outside the project, as the user, with no
                            // permission check anywhere on this path.
                            //
                            // The same guard every read/write/edit/patch tool
                            // already uses: it rejects `..`, canonicalises
                            // through a symlinked parent, and refuses anything
                            // that does not resolve inside the root.
                            let path = match crate::runtime::tools::resolve_tool_path(&wt, target) {
                                Ok(p) => p,
                                Err(e) => {
                                    return Err(anyhow!("refusing to create {target:?}: {e}"));
                                }
                            };
                            if !contents.contains_key(&path) {
                                // The file does not exist, so this is a
                                // creation: there was nothing to anchor to and
                                // `replace` is the whole file.
                                if let Some(parent) = path.parent() {
                                    std::fs::create_dir_all(parent)?;
                                }
                                std::fs::write(&path, &block.replace)?;
                                changed_files.insert(path);
                                // `continue` skips the match bookkeeping below,
                                // so this block is retired here.
                                unmatched.retain(|&idx| idx != i);
                                continue;
                            }
                            // The file exists, so an empty anchor is an append
                            // — which is what a model means by "add this to the
                            // file". It used to be refused here as a would-be
                            // clobber, which refused the common case; and
                            // before *that* it fell through to the exact-match
                            // strategy, where the empty string matches at
                            // offset 0 and the new code landed at the top.
                            // Falling through now does the right thing, and
                            // loses nothing either way.
                        } else {
                            return Err(anyhow!(
                                "edit block has an empty `search` and no target file; it cannot \
                                 say which file to append to"
                            ));
                        }
                    }
                    // Bound blocks apply only to their target file; unbound blocks
                    // fall back to the cross-file search. See research report S4.
                    // `applied` is per block: a block that matched must not
                    // retire its siblings, or the all-or-nothing guarantee
                    // below stops meaning anything.
                    let mut applied = false;
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
                        "{}",
                        describe_unmatched(&edit_blocks, &unmatched, &contents, &paths)
                    ));
                }
                for file_path in changed_files {
                    // A file created above was written directly and is not in
                    // `contents`; re-writing it here would truncate it.
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
        //
        // The publishability rule lives in one place (`output::git::is_publishable_path`)
        // because this filter, the Docker one, and the host-side one were three
        // copies of the same logic — and they had already drifted. All three
        // refused any path starting with `.`, which silently dropped every
        // dotfile an agent legitimately creates (`.github/workflows/*.yml`,
        // `.gitignore`, `.env.example`). On this backend the file is written
        // into the worktree and then discarded, so the work was not just
        // unreported, it was gone.
        let wt = self.worktree_path.clone();
        let files: Vec<String> = agent_files
            .iter()
            .filter(|s| crate::output::git::is_publishable_path(s))
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
            // A failed intent-to-add, and a failed diff, were both discarded.
            // Both produce exactly the same observable as "the agent changed
            // nothing" — an empty string — and an empty diff is what the
            // pipeline reports to the user as a run that did no work. The
            // agent's edits are on disk the whole time; they are simply
            // invisible.
            //
            // Not fatal, because a stale index lock or a path git refuses to
            // stage should not destroy a run whose earlier work is already
            // saved. Visible, because the alternative is a confident wrong
            // answer.
            if let Ok(staged) = Command::new("git").args(&add_args).output()
                && !staged.status.success()
            {
                eprintln!(
                    "Warning: could not stage the agent's new files for diffing ({}). \
                     A brand-new file may be missing from the diff.",
                    String::from_utf8_lossy(&staged.stderr).trim()
                );
            }
            let mut diff_args = vec!["-C", wt_str, "diff", "--"];
            diff_args.extend(refs);
            let out = Command::new("git").args(&diff_args).output()?;
            if !out.status.success() {
                return Err(anyhow!(
                    "`git diff` in the worktree failed ({}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
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
                        // No TUI listening, and the run is not fail-closed, so
                        // this command is auto-approved.
                        //
                        // `tracing::warn!` was the only signal, and `tracing` is
                        // off unless RUST_LOG is set — so in the default
                        // headless `niki run` path, the single case where the
                        // permission system is not actually consulting anyone
                        // was completely silent. A user whose run auto-approved
                        // a destructive command had no way to find out.
                        //
                        // Stderr is the right channel: it survives regardless of
                        // log configuration, and `--output-format json` already
                        // reserves stdout for the envelope.
                        eprintln!(
                            "niki: auto-approved '{}' — the run is headless and \
                             [permissions] fail_closed_headless is off, so a command that \
                             needs approval was allowed without a prompt.",
                            full
                        );
                        tracing::warn!(
                            target: "niki::permissions",
                            command = full.as_str(),
                            "no TUI listening — Ask fell back to Allow (headless)"
                        );
                    } else {
                        let waited = self.permission_checker.prompt_timeout();
                        // A timeout and a refusal are **different events**, and
                        // they used to be the same value.
                        //
                        // `unwrap_or(PermissionAction::Deny)` turned "nobody
                        // answered" into "the user said no", and the message
                        // said so: `Command denied by user`. A user who read
                        // the command for six seconds was told they had denied
                        // it. Since `tools.bash` defaults to `Ask`
                        // (`permissions/mod.rs:83-93`), that was the outcome for
                        // *every* command in every interactive run, and the run
                        // then failed as though the user had blocked it.
                        let action =
                            tokio::task::block_in_place(|| response_rx.recv_timeout(waited));
                        match action {
                            Ok(PermissionAction::Deny) => {
                                return Err(anyhow!("Command denied by user: '{}'", full));
                            }
                            Ok(_) => {}
                            Err(_) => {
                                return Err(anyhow!(
                                    "No answer to the permission prompt for '{}' after {}s, so \
                                     the command was NOT run. That is a timeout, not a refusal — \
                                     raise it with [permissions] prompt_timeout_seconds, or \
                                     set fail_closed_headless = false to allow unattended runs.",
                                    full,
                                    waited.as_secs()
                                ));
                            }
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
        //
        // The deadline is enforced by `exec_with_timeout`, which signals the
        // whole process group. Wrapping `Command::output()` in
        // `spawn_blocking` and dropping the handle on timeout cancelled
        // nothing: a timed-out `cargo build` kept running in the user's
        // worktree, holding the tree and outliving the run that started it.
        match crate::sandbox::exec::exec_with_timeout(
            &cmd,
            &wt,
            timeout,
            self.policy.max_exec_seconds,
            crate::sandbox::truncate_head_tail,
        )
        .await?
        {
            Ok(out) => Ok(ExecOutput {
                exit_code: out.exit_code,
                stdout: out.stdout,
                stderr: out.stderr,
            }),
            Err(t) => {
                if !t.killed {
                    eprintln!(
                        "warning: {t} — the process group could not be signalled and a child may \
                         still be running in {}",
                        wt.display()
                    );
                }
                Err(anyhow!("{t}"))
            }
        }
    }

    fn work_root(&self) -> Option<&std::path::Path> {
        Some(&self.worktree_path)
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

/// Say what did not match, and what is actually in the file instead.
///
/// The old message was "No edit block matched its target file in the worktree
/// (N unmatched); nothing was written" — true, and useless. A model reading it
/// has no idea which of its anchors was wrong, what it sent, or what the file
/// really says, and it is about to guess.
///
/// Aider builds this report deliberately, and it is the difference between a
/// retry that converges and one that repeats: the failed SEARCH verbatim, a
/// "Did you mean to match some of these actual lines?" suggestion taken from
/// the real file, and — where it applies — a note that the other blocks
/// already applied and must not be re-sent. We are all-or-nothing per patch,
/// so nothing partially applied and that last part would be false here; the
/// first two carry the weight.
///
/// The near-miss is deliberately cheap and dependency-free, and capped: it is a
/// hint, not a resolver. The harness must not start choosing an anchor on the
/// model's behalf — that is how a change lands in the wrong place.
fn describe_unmatched(
    blocks: &[crate::sandbox::edit_format::EditBlock],
    unmatched: &[usize],
    contents: &std::collections::HashMap<std::path::PathBuf, String>,
    paths: &[std::path::PathBuf],
) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "{} of your {} edit blocks did not match anything in the worktree, so none of them \
         was written.\n",
        unmatched.len(),
        blocks.len()
    );

    for &i in unmatched {
        let block = &blocks[i];
        let _ = write!(out, "\n--- edit {} ---\n", i + 1);
        if let Some(target) = &block.file {
            let _ = writeln!(out, "you targeted: {target}");
        }
        let search = block.search.trim();
        if search.is_empty() {
            let _ = write!(out, "this edit has an empty anchor (an append).");
            if block.file.is_none() {
                let _ = write!(
                    out,
                    " An append needs a target file: put a `FILE: <path>` line before the block."
                );
            }
            let _ = writeln!(out);
            continue;
        }
        let _ = write!(out, "you searched for:\n{}\n", truncate_middle(search, 400));

        if let Some((score, text, where_)) =
            nearest_lines(&search.lines().collect::<Vec<_>>(), contents, paths)
        {
            let _ = write!(
                out,
                "the closest lines in the file ({:.0}% similar, in {}):\n{}\n",
                score * 100.0,
                where_,
                truncate_middle(&text, 400)
            );
        }
    }

    out.push_str(
        "\nRe-read each file and copy the surrounding lines EXACTLY — indentation included, no \
         line numbers, no ellipsis, enough of them to be unique. Then resubmit only the edits \
         that failed.",
    );
    out
}

/// The highest-scoring equal-length window in any file, with its path.
fn nearest_lines(
    needle: &[&str],
    contents: &std::collections::HashMap<std::path::PathBuf, String>,
    paths: &[std::path::PathBuf],
) -> Option<(f64, String, String)> {
    if needle.is_empty() {
        return None;
    }
    let mut best: Option<(f64, String, String)> = None;
    for path in paths {
        let Some(content) = contents.get(path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < needle.len() {
            continue;
        }
        for start in 0..=(lines.len() - needle.len()) {
            let window = &lines[start..start + needle.len()];
            let score = similarity(needle, window);
            if best.as_ref().is_none_or(|(b, _, _)| score > *b) {
                best = Some((score, window.join("\n"), path.display().to_string()));
            }
        }
    }
    // 0.6 is aider's floor (`find_similar_lines`). It is lower than it looks,
    // because character-level ratios over short, similar lines cluster: on the
    // fixture below, the true near miss scores 0.86 and a line two identifiers
    // away scores 0.77. That is fine here and would not be fine for choosing
    // an edit, which is why this is a suggestion shown to the model and never
    // applied by the harness.
    const SUGGESTION_FLOOR: f64 = 0.6;
    best.filter(|(score, _, _)| *score >= SUGGESTION_FLOOR)
}

/// How alike two equal-length line runs are, in `[0, 1]`.
///
/// Character-level, the way difflib's `SequenceMatcher` is, because a
/// whitespace-only comparison finds nothing useful. An anchor that differs from
/// a real line by one token is exactly the near miss worth showing, and an
/// exact/trimmed-line score gives it zero: `fn a() { 1 }` against `fn a() {}`
/// matches no line exactly and no line after trimming either.
///
/// The matching is a greedy longest-common-substring walk, which is O(n + m)
/// and needs no dependency. It over-estimates a little versus a true LCS — it
/// cannot go back and re-match an earlier character — and that is acceptable
/// for a suggestion shown to a model, which is not the same as resolving an
/// edit. Lines are capped so a minified file cannot make this expensive.
fn similarity(a: &[&str], b: &[&str]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let left: String = a.join("\n");
    let right: String = b.join("\n");
    let l: Vec<char> = left.chars().take(4000).collect();
    let r: Vec<char> = right.chars().take(4000).collect();
    if l.is_empty() || r.is_empty() {
        return 0.0;
    }

    // Greedy match, re-scanning forward from the last matched position, which
    // is the classic cheap approximation of a diff ratio.
    // Exact longest common subsequence for the sizes an anchor actually has.
    // The greedy walk below is O(n + m) but it cannot re-match an earlier
    // character, and on a two-line anchor that cost it most of the score: a
    // real near miss scored 0.41, under the 0.5 floor, so the suggestion never
    // appeared — the feature silently did nothing. An O(n·m) table over a few
    // hundred characters is nothing, and an edit anchor is rarely more.
    const EXACT_LIMIT: usize = 512;
    let matched = if l.len() <= EXACT_LIMIT && r.len() <= EXACT_LIMIT {
        lcs_len(&l, &r)
    } else {
        greedy_match_len(&l, &r)
    };
    // difflib's ratio, which handles unequal lengths: the near miss that
    // matters is usually a line with a token added or removed, so requiring
    // equal lengths scored exactly the case we care about as zero.
    2.0 * matched as f64 / (l.len() + r.len()) as f64
}

/// Length of the longest common subsequence of two character runs.
fn lcs_len(a: &[char], b: &[char]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for &ca in a {
        for (j, &cb) in b.iter().enumerate() {
            cur[j + 1] = if ca == cb {
                prev[j] + 1
            } else {
                cur[j].max(prev[j + 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
        cur.iter_mut().for_each(|v| *v = 0);
    }
    prev[b.len()]
}

/// Cheap O(n + m) fallback for runs too long for the table.
fn greedy_match_len(a: &[char], b: &[char]) -> usize {
    let mut matched = 0usize;
    let mut j = 0usize;
    for c in a {
        while j < b.len() && b[j] != *c {
            j += 1;
        }
        if j < b.len() {
            matched += 1;
            j += 1;
        }
    }
    matched
}

/// Keep both ends of a long string — the difference is usually at one of them.
fn truncate_middle(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max * 2 / 3).collect();
    let tail: String = s.chars().skip(len - max / 3).collect();
    format!("{head}\n… {len} characters omitted …\n{tail}")
}
