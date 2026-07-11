use anyhow::Result;
use bollard::{Docker, container::{CreateContainerOptions, Config, RemoveContainerOptions}, exec::{CreateExecOptions, StartExecResults}};
use futures::StreamExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::artifacts::types::AgentRole;
use crate::config::DockerConfig;

/// Shared registry of containers currently owned by an in-flight pipeline.
/// The Ctrl+C handler drains this list to clean up dangling containers.
pub type ActiveContainers = Arc<Mutex<Vec<String>>>;

pub struct DockerSandbox {
    pub container_id: String,
    pub agent_role: AgentRole,
    pub workspace_path: PathBuf,
    docker: Docker,
    containers: ActiveContainers,
}

pub struct ExecOutput {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

impl DockerSandbox {
    pub async fn create(
        docker: &Docker,
        agent_role: AgentRole,
        source_repo: &Path,
        task_id: &Uuid,
        config: &DockerConfig,
        containers: ActiveContainers,
    ) -> Result<Self> {
        let container_name = format!("niki-{}-{}-{:?}", task_id.to_string()[..8].to_string(), "sandbox", agent_role).to_lowercase();
        let workspace_path = PathBuf::from("/workspace");

        let binds = vec![format!("{}:{}", source_repo.display(), workspace_path.display())];

        // Run the container as the host user's uid:gid so files it writes into the
        // bind-mounted project directory keep the host owner. Otherwise the container
        // (root) rewrites the files as root and the host-side git operations later fail
        // with "Permission denied".
        let meta = std::fs::metadata(&source_repo).ok();
        let uid = meta.as_ref().map(|m| m.uid()).unwrap_or(0);
        let gid = meta.as_ref().map(|m| m.gid()).unwrap_or(0);
        let user = format!("{}:{}", uid, gid);

        let create_opts = CreateContainerOptions {
            name: container_name.as_str(),
            platform: None,
        };
        let host_config = bollard::models::HostConfig {
            binds: Some(binds),
            ..Default::default()
        };

        let container_config = Config {
            image: Some(config.base_image.clone()),
            user: Some(user),
            tty: Some(true),
            cmd: Some(vec!["tail".to_string(), "-f".to_string(), "/dev/null".to_string()]),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Ensure the base image exists locally before creating the container.
        Self::pull_image(docker, &config.base_image).await?;

        let res = docker.create_container(Some(create_opts), container_config).await?;
        docker.start_container::<String>(&res.id, None).await?;

        // Register so a Ctrl+C handler can tear the container down.
        containers.lock().await.push(res.id.clone());

        Ok(Self {
            container_id: res.id,
            agent_role,
            workspace_path,
            docker: docker.clone(),
            containers,
        })
    }

    pub async fn create_from(
        docker: &Docker,
        agent_role: AgentRole,
        _source_sandbox: &DockerSandbox,
        task_id: &Uuid,
        config: &DockerConfig,
        containers: ActiveContainers,
    ) -> Result<Self> {
        // Fallback to simple create for now. In reality, you'd use docker commit + create.
        Self::create(docker, agent_role, Path::new("."), task_id, config, containers).await
    }

    /// Pull the base image if it is not already present locally.
    /// Errors are surfaced to the caller (e.g. no network, auth required).
    async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
        use bollard::image::CreateImageOptions;
        use futures::StreamExt as _;

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

        let exec = self.docker.create_exec(&self.container_id, exec_opts).await?;
        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = self.docker.start_exec(&exec.id, None).await? {
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
        // The host workspace is bind-mounted at /workspace inside the container, so the
        // patch we write to `host_workspace` is visible there as /workspace/.niki-tmp.patch.
        // Run git from /workspace (the repo root) or it won't find the repo or the patch.
        let patch_path = host_workspace.join(".niki-tmp.patch");
        // Normalize: unify line endings and guarantee a trailing newline. LLM-generated
        // diffs often lack a final newline, which makes `git apply` reject the last
        // context line as "corrupt patch".
        let normalized = Self::normalize_patch(patch);
        std::fs::write(&patch_path, normalized)?;

        let res = self.exec(&["sh", "-c", "cd /workspace && git apply .niki-tmp.patch"]).await;

        let _ = std::fs::remove_file(&patch_path);

        match res {
            Ok(output) if output.exit_code == 0 => Ok(()),
            Ok(output) => {
                // If git apply fails, try patch -p1 as a fallback.
                let patch_res = self
                    .exec(&["sh", "-c", "cd /workspace && patch -p1 -i .niki-tmp.patch"])
                    .await;
                if let Ok(p_out) = patch_res {
                    if p_out.exit_code == 0 {
                        return Ok(());
                    }
                }
                Err(anyhow::anyhow!("Failed to apply patch. git exit code: {}\nstdout: {}\nstderr: {}", output.exit_code, output.stdout, output.stderr))
            }
            Err(e) => Err(e),
        }
    }

    pub async fn get_diff(&self) -> Result<String> {
        // Run from /workspace so `git diff` sees the repository.
        let output = self.exec(&["sh", "-c", "cd /workspace && git diff"]).await?;
        Ok(output.stdout)
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
        self.docker.remove_container(&self.container_id, Some(opts)).await?;
        Ok(())
    }
}
