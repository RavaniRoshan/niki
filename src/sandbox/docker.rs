use crate::artifacts::types::AgentRole;
use crate::config::DockerConfig;
use crate::permissions::PermissionAction;
use crate::sandbox::Sandbox;
use anyhow::Result;
use async_trait::async_trait;
use bollard::{
    Docker,
    container::{Config, CreateContainerOptions, RemoveContainerOptions},
    exec::{CreateExecOptions, StartExecResults},
};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Shared registry of containers currently owned by an in-flight pipeline.
/// The Ctrl+C handler drains this list to clean up dangling containers.
pub type ActiveContainers = Arc<Mutex<Vec<String>>>;

pub struct DockerSandbox {
    pub container_id: String,
    pub agent_role: AgentRole,
    pub workspace_path: PathBuf,
    docker: Docker,
    containers: ActiveContainers,
    policy: crate::config::SecurityPolicyConfig,
    permission_checker: crate::permissions::PermissionChecker,
    event_tx: std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>,
}

#[derive(Debug)]
pub struct ExecOutput {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

/// Returns true when the network allowlist is a wildcard (`"*"` or `"all"`),
/// which means the operator explicitly wants full egress despite
/// `network_disabled = true`.
fn allowlist_allows_all(config: &DockerConfig) -> bool {
    config
        .network_allowlist
        .iter()
        .any(|s| s == "*" || s == "all")
}

impl DockerSandbox {
    pub async fn create(
        docker: &Docker,
        agent_role: AgentRole,
        source_repo: &Path,
        task_id: &Uuid,
        config: &DockerConfig,
        niki_config: &crate::config::NikiConfig,
        policy: crate::config::SecurityPolicyConfig,
        containers: ActiveContainers,
        event_tx: std::sync::mpsc::Sender<crate::display::tui::DisplayEvent>,
    ) -> Result<Self> {
        let container_name = format!(
            "niki-{}-{}-{:?}",
            &task_id.to_string()[..8],
            "sandbox",
            agent_role
        )
        .to_lowercase();
        let workspace_path = PathBuf::from("/workspace");

        let binds = vec![format!(
            "{}:{}",
            source_repo.display(),
            workspace_path.display()
        )];

        // Run the container as the host user's uid:gid so files it writes into the
        // bind-mounted project directory keep the host owner. Otherwise the container
        // (root) rewrites the files as root and the host-side git operations later fail
        // with "Permission denied". On non-Unix platforms this is a no-op (Docker is
        // Unix-only anyway).
        #[cfg(unix)]
        let user = {
            use std::os::unix::fs::MetadataExt;
            let meta = std::fs::metadata(source_repo).ok();
            let uid = meta.as_ref().map(|m| m.uid()).unwrap_or(0);
            let gid = meta.as_ref().map(|m| m.gid()).unwrap_or(0);
            format!("{}:{}", uid, gid)
        };
        #[cfg(not(unix))]
        let user = "0:0".to_string();

        let create_opts = CreateContainerOptions {
            name: container_name.as_str(),
            platform: None,
        };

        // Parse resource limits from config (F2). Memory is a human string like
        // "2g"; `bollard` expects bytes. CPU is a float like 2.0; Docker wants
        // microcgroup-style NanoCPUs (e.g. 2.0 → 2_000_000).
        let memory_bytes = Self::parse_memory_limit(&config.memory_limit);
        let nanocpus = (config.cpu_limit * 1_000_000.0) as i64;

        let host_config = bollard::models::HostConfig {
            binds: Some(binds),
            memory: if memory_bytes > 0 {
                Some(memory_bytes)
            } else {
                None
            },
            nano_cpus: if nanocpus > 0 { Some(nanocpus) } else { None },
            // Defense-in-depth for the sandbox (research report S2):
            // - Drop ALL Linux capabilities: the agent runtime needs none of them.
            cap_drop: if config.cap_drop_all {
                Some(vec!["ALL".to_string()])
            } else {
                None
            },
            // Bound the process count to contain fork-bombs / runaway recursion.
            pids_limit: if config.pids_limit > 0 {
                Some(config.pids_limit as i64)
            } else {
                None
            },
            // Egress is blocked by default (network_disabled defaults to true) — the
            // container gets network_mode "none". Open it with `network_disabled =
            // false` (or `network_allowlist = ["*"]`) when a task must fetch
            // dependencies such as `cargo fetch` / `npm install` / `pip install`.
            network_mode: if config.network_disabled && !allowlist_allows_all(config) {
                Some("none".to_string())
            } else {
                None
            },
            // Read-only rootfs; the bind-mounted /workspace stays writable.
            readonly_rootfs: if config.readonly_rootfs {
                Some(true)
            } else {
                None
            },
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.base_image.clone()),
            user: Some(user),
            tty: Some(true),
            cmd: Some(vec![
                "tail".to_string(),
                "-f".to_string(),
                "/dev/null".to_string(),
            ]),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Ensure the base image exists locally before creating the container.
        Self::pull_image(docker, &config.base_image).await?;

        let res = docker
            .create_container(Some(create_opts), container_config)
            .await?;
        docker.start_container::<String>(&res.id, None).await?;

        // Register so a Ctrl+C handler can tear the container down.
        containers.lock().await.push(res.id.clone());

        Ok(Self {
            container_id: res.id,
            agent_role,
            workspace_path,
            docker: docker.clone(),
            containers,
            policy: policy.clone(),
            permission_checker: crate::sandbox::build_permission_checker(&policy, niki_config),
            event_tx,
        })
    }

    /// Parse a human-friendly memory limit string (e.g. "2g", "512m", "1gb")
    /// into bytes. Returns 0 if parsing fails.
    pub fn parse_memory_limit(s: &str) -> i64 {
        let s = s.trim().to_lowercase();
        let (num_str, unit) = if s.ends_with("gb") {
            (&s[..s.len() - 2], "gb")
        } else if s.ends_with('g') {
            (&s[..s.len() - 1], "g")
        } else if s.ends_with("mb") {
            (&s[..s.len() - 2], "mb")
        } else if s.ends_with('m') {
            (&s[..s.len() - 1], "m")
        } else if s.ends_with("kb") {
            (&s[..s.len() - 2], "kb")
        } else if s.ends_with('k') {
            (&s[..s.len() - 1], "k")
        } else {
            (&s[..], "")
        };
        let n: f64 = num_str.parse().unwrap_or(0.0);
        let multiplier: f64 = match unit {
            "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
            "m" | "mb" => 1024.0 * 1024.0,
            "k" | "kb" => 1024.0,
            _ => 1.0,
        };
        (n * multiplier) as i64
    }

    pub async fn create_from(
        docker: &Docker,
        agent_role: AgentRole,
        _source_sandbox: &DockerSandbox,
        task_id: &Uuid,
        config: &DockerConfig,
        niki_config: &crate::config::NikiConfig,
        policy: crate::config::SecurityPolicyConfig,
        containers: ActiveContainers,
    ) -> Result<Self> {
        // Fallback to simple create for now. In reality, you'd use docker commit + create.
        Self::create(
            docker,
            agent_role,
            Path::new("."),
            task_id,
            config,
            niki_config,
            policy,
            containers,
            std::sync::mpsc::channel().0,
        )
        .await
    }

    /// Pull the base image if it is not already present locally.
    /// Errors are surfaced to the caller (e.g. no network, auth required).
    async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
        use bollard::image::CreateImageOptions;
        use futures::StreamExt as _;

        // Supply-chain hardening (research report S2): a mutable tag (e.g.
        // `:24.04`) can be repointed by a compromised registry, silently swapping
        // the sandbox image under us. Prefer an immutable `@sha256:` digest. This
        // is a warning, not a hard failure, so local dev images still work.
        if !image.contains("@sha256:") && image.contains(':') {
            tracing::warn!(
                "Sandbox base image {:?} uses a mutable tag, not an immutable \
                 digest. Pin it to @sha256:<digest> to prevent registry repointing.",
                image
            );
        }

        // Locally-built images (e.g. our pre-baked `niki-sandbox:24.04`) are not on any
        // registry. `create_image` always contacts Docker Hub, so pulling them 404s. Skip
        // the pull when the image already exists locally.
        if docker.inspect_image(image).await.is_ok() {
            tracing::debug!("Image {image} present locally, skipping pull");
            return Ok(());
        }

        tracing::debug!("Pulling image {image}");
        let mut stream = docker.create_image(
            Some(CreateImageOptions {
                from_image: image,
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(update) = stream.next().await {
            match update {
                Ok(_) => {}
                Err(e) => return Err(anyhow::anyhow!("Failed to pull image {image}: {e}")),
            }
        }
        Ok(())
    }

    pub async fn exec(&self, cmd: &[&str]) -> Result<ExecOutput> {
        let exec_opts = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self
            .docker
            .create_exec(&self.container_id, exec_opts)
            .await?;
        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(&exec.id, None).await?
        {
            while let Some(Ok(msg)) = output.next().await {
                match msg {
                    bollard::container::LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        let inspect = self.docker.inspect_exec(&exec.id).await?;
        let exit_code = inspect.exit_code.unwrap_or(0);

        Ok(ExecOutput {
            exit_code,
            stdout,
            stderr,
        })
    }

    /// Verify that every required tool is present in the sandbox image.
    /// Fails fast with a clear message instead of hanging on a runtime install.
    pub async fn ensure_tools(&self, tools: &[String]) -> Result<()> {
        let check = format!(
            "missing=0; for t in {}; do command -v \"$t\" >/dev/null 2>&1 || {{ echo \"missing:$t\"; missing=1; }}; done; exit $missing",
            tools.join(" ")
        );
        match self.exec(&["sh", "-c", &check]).await {
            Ok(out) if out.exit_code == 0 => Ok(()),
            Ok(out) => Err(anyhow::anyhow!(
                "Sandbox image is missing required tools. Expected a pre-baked image with all tooling present. Missing: {}",
                out.stdout.trim().replace('\n', " ")
            )),
            Err(e) => Err(anyhow::anyhow!("Tool check failed: {e}")),
        }
    }

    pub async fn copy_in(&self, _host_path: &Path, _container_path: &str) -> Result<()> {
        // Stub
        Ok(())
    }

    pub async fn copy_out(&self, _container_path: &str, _host_path: &Path) -> Result<()> {
        // Stub
        Ok(())
    }

    /// Normalize an LLM-generated diff before writing it to disk: unify CRLF→LF
    /// line endings and guarantee a trailing newline. `git apply` treats a patch
    /// that ends mid-line (no final newline) as a "corrupt patch" at the last
    /// context line, which silently breaks the Coder's output.
    fn normalize_patch(patch: &str) -> String {
        let mut s = patch.replace("\r\n", "\n");
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s
    }

    pub async fn apply_patch(&self, patch: &str, host_workspace: &Path) -> Result<()> {
        // Try edit format (SEARCH/REPLACE blocks) first
        let edit_blocks = crate::sandbox::edit_format::parse_edit_blocks(patch);
        if !edit_blocks.is_empty() {
            use std::collections::{HashMap, HashSet};
            // Read all workspace files once.
            let files = self.list_files().await?;
            let mut contents: HashMap<String, String> = HashMap::new();
            for file_path in &files {
                if let Ok(content) = std::fs::read_to_string(host_workspace.join(file_path)) {
                    contents.insert(file_path.clone(), content);
                }
            }

            let mut unmatched: Vec<usize> = (0..edit_blocks.len()).collect();
            let mut changed_files: HashSet<String> = HashSet::new();

            for (i, block) in edit_blocks.iter().enumerate() {
                // When a block is bound to a file, only consider that file (or any
                // workspace path ending in it). Unbound blocks fall back to a
                // cross-file search (the previous behavior). See research report S4.
                let targets: Vec<String> = match &block.file {
                    Some(target) => files
                        .iter()
                        .filter(|f| {
                            *f == target
                                || f.ends_with(target)
                                || f.ends_with(&format!("/{target}"))
                        })
                        .cloned()
                        .collect(),
                    None => files.clone(),
                };
                let mut applied = false;
                for file_path in targets {
                    if let Some(content) = contents.get(&file_path) {
                        if let Some(new_content) =
                            crate::sandbox::edit_format::apply_single_edit_block(
                                content,
                                &block.search,
                                &block.replace,
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

            // Write back only the files that actually changed.
            for file_path in changed_files {
                if let Some(content) = contents.get(&file_path) {
                    std::fs::write(host_workspace.join(&file_path), content)?;
                }
            }

            if !unmatched.is_empty() {
                return Err(anyhow::anyhow!(
                    "No edit block matched its target file in the workspace ({} unmatched)",
                    unmatched.len()
                ));
            }
            return Ok(());
        }

        // Fall back to unified diff format. If the text isn't a diff at all
        // (e.g. an empty edits array), treat it as a no-op rather than failing.
        let looks_like_diff =
            patch.contains("diff --git") || patch.contains("--- a/") || patch.contains("+++ b/");
        if !looks_like_diff {
            return Ok(());
        }
        // The host workspace is bind-mounted at /workspace inside the container, so the
        // patch we write to `host_workspace` is visible there as /workspace/.niki-tmp.patch.
        // Run git from /workspace (the repo root) or it won't find the repo or the patch.
        let patch_path = host_workspace.join(".niki-tmp.patch");
        // Normalize: unify line endings and guarantee a trailing newline. LLM-generated
        // diffs often lack a final newline, which makes `git apply` reject the last
        // context line as "corrupt patch".
        let normalized = Self::normalize_patch(patch);
        std::fs::write(&patch_path, normalized)?;

        let res = self
            .exec(&["sh", "-c", "cd /workspace && git apply .niki-tmp.patch"])
            .await;

        let _ = std::fs::remove_file(&patch_path);

        match res {
            Ok(output) if output.exit_code == 0 => Ok(()),
            Ok(output) => Err(anyhow::anyhow!(
                // `git apply` is the only accepted path application method. We
                // deliberately do NOT fall back to `patch -p1`: unlike `git apply`
                // it does not reject paths escaping the repository, which is a
                // path-traversal risk for attacker-influenced diffs. See report S3.
                "Failed to apply patch (git apply only; patch -p1 fallback disabled for safety). git exit code: {}\nstdout: {}\nstderr: {}",
                output.exit_code,
                output.stdout,
                output.stderr
            )),
            Err(e) => Err(e),
        }
    }

    pub async fn get_diff(&self) -> Result<String> {
        // Run from /workspace so `git diff` sees the repository.
        let output = self
            .exec(&["sh", "-c", "cd /workspace && git diff"])
            .await?;
        Ok(output.stdout)
    }

    /// List all tracked files in the workspace.
    async fn list_files(&self) -> Result<Vec<String>> {
        let output = self
            .exec(&[
                "sh",
                "-c",
                "cd /workspace && git ls-files --cached --exclude-standard",
            ])
            .await?;
        Ok(output.stdout.lines().map(|s| s.to_string()).collect())
    }

    pub async fn destroy(&self) -> Result<()> {
        // Unregister first so a concurrent Ctrl+C handler doesn't double-remove.
        {
            let mut list = self.containers.lock().await;
            list.retain(|id| id != &self.container_id);
        }

        let opts = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        self.docker
            .remove_container(&self.container_id, Some(opts))
            .await?;
        Ok(())
    }
}

#[async_trait]
impl Sandbox for DockerSandbox {
    async fn ensure_tools(&self, tools: &[String]) -> Result<()> {
        DockerSandbox::ensure_tools(self, tools).await
    }
    async fn apply_patch(&self, patch: &str, host_workspace: &Path) -> Result<()> {
        DockerSandbox::apply_patch(self, patch, host_workspace).await
    }
    async fn get_diff(&self) -> Result<String> {
        DockerSandbox::get_diff(self).await
    }
    async fn exec(&self, cmd: &[&str], role: Option<&AgentRole>) -> Result<ExecOutput> {
        // F1: Enforce security policy when a role is supplied.
        if role.is_some() {
            crate::sandbox::check_command_policy(cmd, &self.policy)?;
            // F1b: Enforce granular permission policy (dead-island PermissionChecker).
            let full = cmd.join(" ");
            match self.permission_checker.check_command(&full) {
                crate::permissions::Permission::Deny => {
                    return Err(anyhow::anyhow!(
                        "Command denied by permission policy: '{}'",
                        full
                    ));
                }
                crate::permissions::Permission::Ask => {
                    let (response_tx, response_rx) = std::sync::mpsc::channel();
                    let request = crate::display::tui::DisplayEvent::PermissionRequest {
                        command: full.clone(),
                        response_tx,
                    };
                    if self.event_tx.send(request).is_err() {
                        // No TUI listening — fall back to Allow (headless mode).
                    } else {
                        let action = tokio::task::block_in_place(|| {
                            response_rx.recv_timeout(std::time::Duration::from_secs(5))
                        })
                        .unwrap_or(PermissionAction::Deny);
                        if matches!(action, PermissionAction::Deny) {
                            return Err(anyhow::anyhow!("Command denied by user: '{}'", full));
                        }
                    }
                }
                crate::permissions::Permission::Allow => {}
            }
        }
        // F3: Enforce exec timeout.
        let timeout = std::time::Duration::from_secs(self.policy.max_exec_seconds);
        tokio::time::timeout(timeout, DockerSandbox::exec(self, cmd))
            .await
            .map_err(|_| {
                anyhow::anyhow!("exec timed out after {}s", self.policy.max_exec_seconds)
            })?
    }
    async fn destroy(&self) -> Result<()> {
        DockerSandbox::destroy(self).await
    }
}
