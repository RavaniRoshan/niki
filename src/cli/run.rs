use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::orchestrator::pipeline::{Task, execute_pipeline};
use crate::orchestrator::state::{TaskRecord, TaskStatus};
use crate::sandbox::SandboxBackend;
use crate::sandbox::docker::ActiveContainers;
use anyhow::{Result, anyhow};
use bollard::Docker;
use clap::Args;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Probe container runtime sockets: Podman (rootless, then rootful) → Docker.
/// Returns the first connection that pings successfully, or an error if none work.
#[cfg(unix)]
#[allow(clippy::collapsible_if)]
pub(crate) async fn connect_container_runtime() -> Result<Docker> {
    // 1. Respect explicit DOCKER_HOST override if set.
    if let Ok(host) = env::var("DOCKER_HOST") {
        if !host.is_empty() {
            if let Ok(d) = Docker::connect_with_local_defaults() {
                if d.ping().await.is_ok() {
                    tracing::info!("Connected via DOCKER_HOST={host}");
                    return Ok(d);
                }
            }
        }
    }

    // 2. Probe known Podman and Docker socket paths in priority order.
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(unix)]
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", uid));
    #[cfg(not(unix))]
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_default();

    let candidates = [
        PathBuf::from(&runtime_dir).join("podman/podman.sock"), // rootless podman
        PathBuf::from("/run/podman/podman.sock"),               // rootful podman
        PathBuf::from("/var/run/docker.sock"),                  // docker
    ];

    for socket in &candidates {
        if !socket.exists() {
            continue;
        }
        let addr = format!("unix://{}", socket.display());
        if let Ok(d) = Docker::connect_with_local(addr.as_str(), 120, bollard::API_DEFAULT_VERSION)
        {
            if d.ping().await.is_ok() {
                tracing::info!("Connected via {}", socket.display());
                return Ok(d);
            }
        }
    }

    Err(anyhow!(
        "No container runtime found. Install and start Podman \
         (systemctl --user enable --now podman.socket) or Docker."
    ))
}
use crate::artifacts::types::AgentRole;

#[derive(Args)]
pub struct RunArgs {
    /// Natural language description of the task
    pub description: String,

    /// Path to the project (default: current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,

    /// Name for the output branch (default: niki/{task_id_short})
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Override max revision rounds (default: from config)
    #[arg(long)]
    pub max_rounds: Option<u32>,

    /// Override planner model
    #[arg(long)]
    pub planner_model: Option<String>,

    /// Override coder model
    #[arg(long)]
    pub coder_model: Option<String>,

    /// Override tester model
    #[arg(long)]
    pub tester_model: Option<String>,

    /// Override reviewer model
    #[arg(long)]
    pub reviewer_model: Option<String>,

    /// Sandbox backend: docker (container) or worktree (git worktree + local
    /// process, no Docker). Overrides [docker] backend in config.
    #[arg(long, value_enum)]
    pub backend: Option<BackendArg>,

    /// Run the Planner only and show the spec without executing
    #[arg(long)]
    pub dry_run: bool,

    /// Minimal output — no streaming, just final report
    #[arg(long)]
    pub quiet: bool,

    /// Show full agent reasoning (not just summaries)
    #[arg(long)]
    pub verbose: bool,

    /// Render a rich terminal TUI (panels per agent stage) instead of the
    /// inline streaming view. Requires a TTY; ignored when piped.
    #[arg(long)]
    pub tui: bool,
}

/// CLI spelling of the sandbox backend; maps onto [`crate::sandbox::SandboxBackend`].
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum BackendArg {
    Docker,
    Worktree,
}

impl From<BackendArg> for crate::sandbox::SandboxBackend {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Docker => crate::sandbox::SandboxBackend::Docker,
            BackendArg::Worktree => crate::sandbox::SandboxBackend::Worktree,
        }
    }
}

fn role_filename(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
    }
}

pub async fn handle(args: &RunArgs) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.canonicalize()?,
        None => env::current_dir()?,
    };

    let mut config = NikiConfig::load(&project_dir)?;

    if let Some(r) = args.max_rounds {
        config.general.max_revision_rounds = r;
    }
    if let Some(ref m) = args.planner_model {
        config.agents.planner.model = m.clone();
    }
    if let Some(ref m) = args.coder_model {
        config.agents.coder.model = m.clone();
    }
    if let Some(ref m) = args.tester_model {
        config.agents.tester.model = m.clone();
    }
    if let Some(ref m) = args.reviewer_model {
        config.agents.reviewer.model = m.clone();
    }

    // Resolve the sandbox backend: explicit --backend wins, otherwise fall
    // back to [docker] backend in config (default: docker).
    let backend = if let Some(b) = args.backend {
        b.into()
    } else {
        config.docker.backend
    };
    config.docker.backend = backend;

    let uses_docker = matches!(backend, SandboxBackend::Docker);

    // Trust & cost notices (launch-plan B3 / S6 / G9).
    if matches!(backend, SandboxBackend::Worktree) {
        eprintln!(
            "warning: worktree backend runs agent commands as local processes on YOUR host \
             with your privileges — there is no VM/container isolation. Prefer the default \
             container backend for untrusted tasks."
        );
    }
    if config.general.spend_cap_usd > 0.0 {
        eprintln!(
            "note: spend cap active — this run will abort before a branch is created if estimated cost exceeds ${:.2}",
            config.general.spend_cap_usd
        );
    }

    let task = Task {
        id: Uuid::new_v4(),
        description: args.description.clone(),
        project_path: project_dir.clone(),
    };

    let mut display = AgenticDisplay::new();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Opt-in rich TUI. Must be enabled before any display call so the banner
    // and subsequent events are routed to the render thread.
    if args.tui {
        display.enable_tui(task.description.clone(), task.project_path.clone(), cancel.clone());
    }

    if !args.quiet {
        display.show_banner(&task, &config);
    }

    // Resolve output locations up front so the Ctrl+C handler can persist state.
    let task_dir = project_dir
        .join(&config.general.output_dir)
        .join("tasks")
        .join(task.id.to_string());

    // Capture values the shutdown handlers need BEFORE any handler closure
    // moves them. (String/PathBuf don't impl Copy, so an `async move` would
    // otherwise leave nothing for the second handler.) See research report S13.
    let output_dir = config.general.output_dir.clone();
    let project_dir_for_signal = project_dir.clone();
    let project_dir_for_ctrlc = project_dir.clone();
    let output_dir_for_ctrlc = output_dir.clone();

    // Track containers so the shutdown handlers can tear them down cleanly.
    let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));

    {
        let containers = containers.clone();
        let task_dir = task_dir.clone();
        let task_id_str = task.id.to_string();
        let output_dir = output_dir_for_ctrlc;
        let worktree_dir = project_dir_for_ctrlc
            .join(&output_dir)
            .join(format!(".niki-worktrees/{}", task_id_str));
        tokio::spawn(async move {
            if signal::ctrl_c().await.is_ok() {
                eprintln!("\n Shutting down — cleaning up...");

                let ids = containers.lock().await.clone();
                #[cfg(unix)]
                if !ids.is_empty()
                    && let Ok(docker) = connect_container_runtime().await
                {
                    for id in ids {
                        // force:true stops the container if still running, then removes it.
                        let _ = docker
                            .remove_container(
                                &id,
                                Some(bollard::container::RemoveContainerOptions {
                                    force: true,
                                    ..Default::default()
                                }),
                            )
                            .await;
                    }
                }

                // Persist a cancelled task record so status commands reflect reality.
                let mut rec =
                    TaskRecord::new(uuid::Uuid::parse_str(&task_id_str).unwrap_or_default(), "");
                rec.status = TaskStatus::Cancelled;
                let _ = rec.save_to_disk(&task_dir);

                // Clean up any leftover .niki-worktrees/<task_id> dirs that were
                // created for parallel coder agents. Left behind after a cancelled
                // or killed run, they can be large and are never reused. See
                // research report S13.
                if worktree_dir.exists() {
                    let _ = std::fs::remove_dir_all(&worktree_dir);
                }

                eprintln!(" Partial results saved under ./{}/tasks/", output_dir);
                // 130 = 128 + SIGINT(2), the conventional exit code for Ctrl+C.
                // Lets CI/scripts distinguish an interrupt from a generic failure.
                std::process::exit(130);
            }
        });
    }

    // SIGTERM handler (kill, systemd stop, container engine timeout, CI cancel).
    // Mirrors the Ctrl+C path but exits 143 (128 + SIGTERM(15)) so callers can
    // distinguish the two signals. See research report S13.
    #[cfg(unix)]
    {
        use signal::unix::{SignalKind, signal};
        let containers = containers.clone();
        let task_dir = task_dir.clone();
        let task_id_str = task.id.to_string();
        let worktree_cleanup = project_dir_for_signal
            .join(&output_dir)
            .join(format!(".niki-worktrees/{}", task.id));
        tokio::spawn(async move {
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(_) => return,
            };
            if sigterm.recv().await.is_none() {
                return;
            }
            eprintln!("\n SIGTERM received — cleaning up...");
            let ids = containers.lock().await.clone();
            if !ids.is_empty() {
                if let Ok(docker) = connect_container_runtime().await {
                    for id in ids {
                        let _ = docker
                            .remove_container(
                                &id,
                                Some(bollard::container::RemoveContainerOptions {
                                    force: true,
                                    ..Default::default()
                                }),
                            )
                            .await;
                    }
                }
            }
            if worktree_cleanup.exists() {
                let _ = std::fs::remove_dir_all(&worktree_cleanup);
            }
            let mut rec =
                TaskRecord::new(uuid::Uuid::parse_str(&task_id_str).unwrap_or_default(), "");
            rec.status = TaskStatus::Cancelled;
            let _ = rec.save_to_disk(&task_dir);
            std::process::exit(143);
        });
    }

    // Only connect to a container runtime when the Docker backend is in use. The
    // worktree backend never touches Podman/Docker, so it runs without a daemon.
    // The dry-run path also skips the daemon ping (it never creates a sandbox).
    #[cfg(unix)]
    let docker = if uses_docker && !args.dry_run {
        let d = connect_container_runtime()
            .await
            .map_err(|e| anyhow!("Container runtime error: {}", e))?;
        Some(d)
    } else {
        None
    };
    #[cfg(not(unix))]
    let docker: Option<Docker> = None;

    // Borrow the connection for the pipeline; None for non-Docker backends.
    let docker_ref = docker.as_ref();

    // Persist an initial "running" record.
    let mut record = TaskRecord::new(task.id, &task.description);
    if let Err(e) = record.save_to_disk(&task_dir) {
        eprintln!("Warning: could not save task state: {}", e);
    }

    // Hermetic safety: fingerprint the repo before the pipeline mutates anything,
    // so we can prove afterwards that only the new `niki/<id>` branch was added.
    let pre_snapshot = match crate::safety::snapshot(&project_dir) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Warning: could not snapshot repo for safety proof: {}", e);
            None
        }
    };

    let mut result = match execute_pipeline(
        &task,
        &config,
        docker_ref,
        &mut display,
        containers.clone(),
        args.dry_run,
        cancel.clone(),
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            let mut rec = TaskRecord::new(task.id, &task.description);
            rec.status = TaskStatus::Failed {
                error: e.to_string(),
            };
            let _ = rec.save_to_disk(&task_dir);
            display.finish_tui();
            return Err(e);
        }
    };

    let branch_name = args
        .branch
        .clone()
        .unwrap_or_else(|| format!("niki/{}", &task.id.to_string()[..8]));

    // Send branch name to TUI for status line display
    display.set_branch_name(&branch_name);

    // Save raw agent artifacts.
    let artifacts_dir = task_dir.join("artifacts");
    if let Err(e) = std::fs::create_dir_all(&artifacts_dir) {
        eprintln!("Warning: could not create artifacts dir: {}", e);
    } else {
        for (role, json) in &result.artifacts {
            let path = artifacts_dir.join(format!("{}.json", role_filename(*role)));
            if let Err(e) = crate::util::write_restricted(&path, json) {
                eprintln!("Warning: could not save artifact {:?}: {}", role, e);
            }
        }
        if let Some(te) = &result.test_execution {
            let path = artifacts_dir.join("test_execution.json");
            if let Err(e) = crate::util::write_restricted(&path, serde_json::to_string_pretty(te)?)
            {
                eprintln!("Warning: could not save test_execution artifact: {}", e);
            }
        }
    }

    // Generate the static HTML dashboard (diff viewer + annotations).
    {
        let find_artifact = |role: AgentRole| -> Option<String> {
            result
                .artifacts
                .iter()
                .find(|(r, _)| *r == role)
                .map(|(_, j)| j.clone())
        };
        let review_json = find_artifact(AgentRole::Reviewer);
        let security_json = find_artifact(AgentRole::SecurityAuditor);

        let total_in: u32 = result.metrics.iter().map(|m| m.input_tokens).sum();
        let total_out: u32 = result.metrics.iter().map(|m| m.output_tokens).sum();
        let total_cost: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
        let total_ms: u64 = result.metrics.iter().map(|m| m.latency_ms).sum();
        if config.general.spend_cap_usd > 0.0 && total_cost > config.general.spend_cap_usd {
            eprintln!(
                "\nwarning: spend cap exceeded — estimated ${:.4} > cap ${:.2}. \
                 Lower the task scope or raise [general] spend_cap_usd.",
                total_cost, config.general.spend_cap_usd
            );
        }
        let metrics_rows = vec![
            ("Agents run".to_string(), result.metrics.len().to_string()),
            ("Input tokens".to_string(), total_in.to_string()),
            ("Output tokens".to_string(), total_out.to_string()),
            (
                "Latency".to_string(),
                format!("{:.1}s", total_ms as f64 / 1000.0),
            ),
            (
                "Est. cost".to_string(),
                if total_cost > 0.0 {
                    format!("${:.4}", total_cost)
                } else {
                    "n/a".to_string()
                },
            ),
        ];

        let input = crate::output::dashboard::DashboardInput {
            task_id: &task.id.to_string(),
            description: &task.description,
            verdict: &format!("{:?}", result.verdict),
            revision_rounds: result.revision_rounds,
            final_diff: &result.final_diff,
            review_json: review_json.as_deref(),
            security_json: security_json.as_deref(),
            metrics_rows,
        };
        if let Err(e) = crate::output::dashboard::write_dashboard(&task_dir, &input) {
            eprintln!("Warning: could not generate dashboard: {}", e);
        }
    }

    // Generate the patch file.
    if let Err(e) =
        crate::output::patch::generate_patch(&result.final_diff, &task_dir.join("changes.patch"))
    {
        eprintln!("Failed to generate patch: {}", e);
    }

    // For the worktree backend the change still lives inside the sandbox copy (a
    // separate git worktree), so `working_tree_diff` on the host would be empty.
    // Apply the sandbox's diff to the host working tree first; the Docker backend
    // already wrote through the bind mount and skips this step.
    if !uses_docker
        && !result.final_diff.trim().is_empty()
        && let Err(e) =
            crate::output::git::apply_diff_to_working_tree(&project_dir, &result.final_diff)
    {
        eprintln!("Warning: could not apply sandbox diff to host: {}", e);
    }

    // Create the git branch + commit (no-op when there is no diff).
    if let Err(e) = crate::output::git::create_branch_and_commit(
        &project_dir,
        &branch_name,
        &result.final_diff,
        &task.id.to_string(),
    ) {
        eprintln!("Warning: git branch/commit failed: {}", e);
    }

    // Hermetic safety proof (BUILD_PLAN 1.1): with the branch now committed,
    // verify the committed repo state is unchanged except for that one branch.
    // Emit `safety_proof.json` next to the report and attach it to the result.
    // Skip when there was no diff (no branch was created), so a no-op run isn't
    // misreported as NON-HERMETIC.
    if !result.final_diff.trim().is_empty()
        && let Some(pre) = &pre_snapshot
    {
        // Enforce the hermetic guarantee (research report S9). Previously this used
        // strict=false and only printed a warning on a committed-state breach, so a
        // non-hermetic run could silently complete. strict=true makes prove() return
        // an Err when existing branches are repointed, history is rewritten, or the
        // new branch is missing — which we propagate to abort the run rather than
        // present a completed task. The working-tree cleanliness flags remain
        // informational (NIKI intentionally applies the diff to the host working tree).
        let proof =
            crate::safety::prove(pre, &project_dir, &branch_name, &task.id.to_string(), true)?;
        if let Err(e) = crate::util::write_restricted(
            &task_dir.join("safety_proof.json"),
            serde_json::to_string_pretty(&proof)?,
        ) {
            eprintln!("Warning: could not write safety_proof.json: {}", e);
        }
        result.safety_proof = Some(proof);
    }

    // Generate the markdown report (now includes the hermetic safety proof).
    if let Err(e) = crate::output::report::generate_report(&task, &config, &result) {
        eprintln!("Warning: could not generate report: {}", e);
    }

    // Persist final task record.
    record.status = TaskStatus::Completed;
    record.branch = Some(branch_name.clone());
    record.verdict = Some(format!("{:?}", result.verdict));
    record.revision_rounds = result.revision_rounds;
    record.add_metrics(&result.metrics);
    if let Err(e) = record.save_to_disk(&task_dir) {
        eprintln!("Warning: could not save final task state: {}", e);
    }

    if !args.quiet {
        display.show_completion(&result, &branch_name, &task_dir);
    }

    // Tear down the TUI (if active): this joins the render thread, which
    // restores the terminal before any further output.
    display.finish_tui();

    Ok(())
}
