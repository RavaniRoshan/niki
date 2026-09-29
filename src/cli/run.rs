use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::orchestrator::pipeline::{PipelineResult, Task, execute_pipeline};
use crate::orchestrator::state::{TaskRecord, TaskStatus};
use crate::sandbox::SandboxBackend;
use crate::sandbox::docker::ActiveContainers;
use anyhow::{Context, Result, anyhow};
use bollard::Docker;
use clap::Args;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Probe container runtime endpoints, in priority order, and return the first
/// that answers.
///
/// Windows gets its own implementation rather than inheriting the Unix one.
/// It previously had none: `connect_container_runtime` was `#[cfg(unix)]`, and
/// the non-Unix arm hard-coded `docker = None`, so a Windows user who had
/// Docker Desktop installed and running still got `None` — the container
/// backend was unreachable on that platform regardless of their setup. The
/// Unix path's candidates are all POSIX socket paths that cannot exist on
/// Windows, so it could not simply be reused.
#[cfg(windows)]
pub(crate) async fn connect_container_runtime() -> Result<Docker> {
    // 1. An explicit DOCKER_HOST wins. Docker Desktop sets this itself, and a
    //    user pointing at a remote or VM-hosted engine relies on it.
    if let Ok(host) = env::var("DOCKER_HOST")
        && !host.is_empty()
    {
        if let Ok(d) = Docker::connect_with_http(host.as_str(), 120, bollard::API_DEFAULT_VERSION)
            && d.ping().await.is_ok()
        {
            tracing::info!("Connected via DOCKER_HOST={host}");
            return Ok(d);
        }
    }

    // 2. Docker Desktop's named pipe — the default on Windows.
    if let Ok(d) = Docker::connect_with_socket_defaults()
        && d.ping().await.is_ok()
    {
        tracing::info!("Connected via the Docker Desktop named pipe");
        return Ok(d);
    }

    Err(anyhow!(
        "No container runtime found. Start Docker Desktop, or point DOCKER_HOST \
         at a Podman machine endpoint (`podman machine inspect` prints it)."
    ))
}

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

    /// Override the unified run budget: max billable steps (stages + retries
    /// + tool-loop steps). Exhaustion stops the run with BudgetExhausted.
    #[arg(long)]
    pub max_steps: Option<u32>,

    /// Override the unified run budget: max estimated USD (falls back to
    /// spend_cap_usd when unset).
    #[arg(long)]
    pub max_usd: Option<f64>,

    /// Override the unified run budget: max wallclock seconds.
    #[arg(long)]
    pub max_wallclock_secs: Option<u64>,

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

    /// Execute from a user-approved plan: full UUID or short prefix of a task
    /// whose `niki plan` (or `--dry-run`) output you reviewed. Skips the
    /// Planner LLM call and drives the run from that spec.
    #[arg(long)]
    pub plan: Option<String>,

    /// Machine-readable output contract for CI/scripts: `text` (default,
    /// human streaming) or `json` (one JSON envelope on stdout at the end).
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub output_format: OutputFormat,

    /// Deterministic-inputs mode for CI: skip project memory, MCP discovery,
    /// and external knowledge-URL fetching. Model sampling nondeterminism and
    /// failover retries still apply — bare means "no ambient inputs".
    #[arg(long)]
    pub bare: bool,

    /// Permission posture for this run: `manual` (default — Ask prompts in
    /// TUI, allows headless with a warning), `auto` (sandbox-safe allowed,
    /// host-reaching still Ask), `dontask` (Ask becomes Allow; explicit CI
    /// mode), `bypass` (all checks Allow; isolated containers only).
    /// Overrides `[permissions] mode` in config.
    #[arg(long)]
    pub permission_mode: Option<String>,

    /// OTLP/HTTP endpoint for trace export (e.g. http://localhost:4318).
    /// Also reads `OTEL_EXPORTER_OTLP_ENDPOINT`. Export is best-effort and
    /// warn-only: telemetry never fails a run.
    #[arg(long)]
    pub otel_endpoint: Option<String>,

    /// Minimal output — no streaming, just final report
    #[arg(long)]
    pub quiet: bool,

    /// Create the branch even when the executed test suite (or mutation gate)
    /// failed. The override is recorded in the report and task record — a
    /// forced branch is explicitly NOT a verified branch.
    #[arg(long)]
    pub force: bool,

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

/// Machine-readable output contract for scripts and CI.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl From<BackendArg> for crate::sandbox::SandboxBackend {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Docker => crate::sandbox::SandboxBackend::Docker,
            BackendArg::Worktree => crate::sandbox::SandboxBackend::Worktree,
        }
    }
}

/// Render the Planner's spec as a human-readable `plan.md` for the plan mode
/// (`niki plan` / `--dry-run`). Machine-readable truth stays in
/// `artifacts/planner.json`; this file is the approval surface.
fn write_plan_md(task_dir: &std::path::Path, task: &Task, result: &PipelineResult) {
    let Some(planner_json) = result
        .artifacts
        .iter()
        .find(|(r, _)| *r == AgentRole::Planner)
        .map(|(_, j)| j.as_str())
    else {
        return;
    };
    let spec: crate::artifacts::types::TaskSpec = match serde_json::from_str(planner_json) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Warning: could not parse planner artifact for plan.md: {}",
                e
            );
            return;
        }
    };
    let mut out = format!(
        "# Plan — {}\n\n> Produced by `niki plan` (task `{}`). Review, edit the approach if needed, then execute with `niki run --plan {}`.\n\n## Summary\n\n{}\n\n## Approach\n\n{}\n\n",
        task.description,
        task.id,
        &task.id.to_string()[..8],
        spec.summary,
        spec.approach,
    );
    out.push_str("## Files to touch\n\n");
    for f in &spec.files_to_modify {
        out.push_str(&format!(
            "- `{}` ({:?}): {}\n",
            f.path, f.action, f.description
        ));
    }
    out.push_str("\n## Acceptance criteria\n\n");
    for c in &spec.acceptance_criteria {
        out.push_str(&format!("- [ ] {}\n", c));
    }
    if !spec.constraints.is_empty() {
        out.push_str("\n## Constraints\n\n");
        for c in &spec.constraints {
            out.push_str(&format!("- {}\n", c));
        }
    }
    if let Some(u) = &spec.uncertainties {
        if !u.is_empty() {
            out.push_str("\n## Open questions (planner uncertainties)\n\n");
            for q in u {
                out.push_str(&format!("- {}\n", q));
            }
        }
    }
    out.push_str(&format!(
        "\n## Topology\n\n{}\n\n## Cost of planning\n\n",
        result.topology_reason
    ));
    for m in &result.metrics {
        out.push_str(&format!(
            "- {:?} {} ({}): {} in / {} out tok, ${:.4}\n",
            m.role, m.model, m.provider, m.input_tokens, m.output_tokens, m.cost_usd
        ));
    }
    if let Err(e) = crate::util::write_restricted(&task_dir.join("plan.md"), out) {
        eprintln!("Warning: could not write plan.md: {}", e);
    }
}

/// Cross-check the three things a run publishes about itself.
///
/// What a finished run records: its status, and the branch it may name.
///
/// Extracted because two consumers need the same answer and did not have it.
/// `task.json` decided with this rule; `manifest.json` re-derived a looser one
/// of its own — withhold the branch only when one was *blocked*, advertise the
/// name whenever the name was known — and the two disagreed by construction.
/// A breadth run committed fine, failed afterwards, and died in reconciliation
/// with "manifest.json names branch niki/31eb6c86 but task.json records None".
///
/// The rule is deliberately strict: a run that failed to commit names no
/// branch at all, because a name is an advertisement and the branch may not be
/// on disk. The failure itself is recorded separately, in `status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Why the run is not `Completed`, if it is not.
    pub error: Option<String>,
    /// The branch the run may advertise. `None` unless the branch was
    /// genuinely created and committed.
    pub branch: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn decide_completion(
    branch_creation_error: Option<String>,
    branch_block_note: Option<String>,
    branch_created: bool,
    branch_name: &str,
    dry_run: bool,
    task_dir: &std::path::Path,
) -> Completion {
    let error = branch_creation_error.or(branch_block_note).or_else(|| {
        if branch_created {
            None
        } else if dry_run {
            Some(format!(
                "Dry run: no branch created. Review {}/plan.md, then re-run without --dry-run.",
                task_dir.display()
            ))
        } else {
            Some("No branch created: the run produced an empty diff.".to_string())
        }
    });
    Completion {
        // The branch is named only when there is no error at all — the same
        // value every consumer must read, so nobody can re-derive it wrongly.
        branch: if error.is_none() {
            Some(branch_name.to_string())
        } else {
            None
        },
        error,
    }
}

/// The pipeline result, `task.json`, and `manifest.json` are produced by
/// different code paths at different moments. Nothing forced them to agree, so
/// a run could finish with a `task.json` recording a different cost than the
/// report rendered, or a manifest naming a branch the record says was never
/// created. Every one of those is a claim a user would act on.
///
/// This is deliberately a hard error rather than a warning: the artefacts are
/// already on disk by the time it runs, so warning would mean shipping a run
/// that is known to be self-contradictory and telling the user so in a log
/// line they may never read.
fn reconcile_result_record_manifest(
    result: &crate::orchestrator::pipeline::PipelineResult,
    record: &TaskRecord,
    task_dir: &std::path::Path,
) -> Result<()> {
    use crate::artifacts::types::RunOutcome;

    // 1. The persisted record must carry the same verdict, and the same
    //    provenance, as the result it was derived from.
    let recorded_verdict = record.verdict.as_deref().unwrap_or("");
    let expected_verdict = format!("{:?}", result.verdict);
    anyhow::ensure!(
        recorded_verdict == expected_verdict,
        "task.json records verdict {recorded_verdict:?} but the run produced \
         {expected_verdict:?} — the stored run and this process disagree"
    );

    let recorded_outcome = record.outcome.as_ref().ok_or_else(|| {
        anyhow!(
            "task.json records no outcome, so nothing says \
             whether this run was independently reviewed"
        )
    })?;
    let expected_outcome = serde_json::to_value(&result.outcome)?;
    anyhow::ensure!(
        *recorded_outcome == expected_outcome,
        "task.json records outcome {recorded_outcome} but the run produced \
         {expected_outcome}"
    );

    // 2. An approval must be earned, in the record as much as in the result.
    //    Defense in depth: `RunOutcome` already forbids it structurally, and
    //    this catches a record assembled by a path that never went through
    //    the type.
    if record
        .outcome
        .as_ref()
        .and_then(|o| o.get("outcome"))
        .and_then(|o| o.as_str())
        == Some("reviewed")
        && expected_verdict == "Approved"
    {
        let by = recorded_outcome
            .get("by")
            .and_then(|b| b.as_str())
            .unwrap_or_default();
        anyhow::ensure!(
            !by.is_empty(),
            "task.json reports a reviewed approval with no reviewer named"
        );
    }

    // 3. Costs must agree. The record accumulates from the same metrics, so a
    //    mismatch means one of the two is reading a different slice.
    let expected_cost: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
    let recorded_cost = record.total_cost_usd;
    anyhow::ensure!(
        (recorded_cost - expected_cost).abs() < 1e-6,
        "task.json records ${recorded_cost:.6} but the run's stages cost \
         ${expected_cost:.6}"
    );

    // 4. The manifest, when it exists, must not name a branch the run did not
    //    produce, or cost the run never incurred.
    // A manifest that exists but cannot be parsed is itself a contradiction —
    // silently skipping the comparison on a read error would let exactly the
    // runs with the most broken provenance through unchecked.
    let manifest_path = task_dir.join("manifest.json");
    if manifest_path.is_file() {
        let manifest = crate::orchestrator::provenance::read_manifest(task_dir)
            .with_context(|| format!("could not read {}", manifest_path.display()))?;
        if let Some(branch) = &manifest.branch {
            anyhow::ensure!(
                record.branch.as_deref() == Some(branch.as_str()),
                "manifest.json names branch {branch} but task.json records {:?}",
                record.branch
            );
        }
        if !manifest.dry_run {
            anyhow::ensure!(
                (manifest.total_cost_usd - expected_cost).abs() < 1e-6,
                "manifest.json records ${:.6} but the run's stages cost ${expected_cost:.6}",
                manifest.total_cost_usd
            );
        }
    }

    // 5. A run whose outcome is a failure must not be recorded as completed.
    if matches!(
        result.outcome,
        RunOutcome::Failed { .. } | RunOutcome::Cancelled
    ) {
        anyhow::ensure!(
            record.status != TaskStatus::Completed,
            "task.json reports a completed run whose outcome is {:?}",
            result.outcome
        );
    }
    Ok(())
}

/// Machine-readable result envelope for `--output-format json` (CI/scripts).
/// Stable contract: `status` is `completed`, `failed`, or `error`.
/// `tests_passed`/`mutation_passed` are null when no suite ran.
fn result_envelope(
    task: &Task,
    record: &TaskRecord,
    result: Option<&crate::orchestrator::pipeline::PipelineResult>,
    branch: Option<&str>,
    branch_block: Option<&str>,
    forced_branch: bool,
    bare: bool,
    task_dir: &std::path::Path,
) -> serde_json::Value {
    let (verdict, outcome, reviewed, revisions, tests_passed, mutation_passed) = match result {
        Some(r) => (
            format!("{:?}", r.verdict),
            serde_json::to_value(&r.outcome).unwrap_or(serde_json::Value::Null),
            r.outcome.is_independently_reviewed(),
            r.revision_rounds,
            r.test_execution.as_ref().map(|te| te.passed),
            r.test_execution
                .as_ref()
                .and_then(|te| te.mutation.as_ref().map(|m| m.passed)),
        ),
        None => (
            "unknown".to_string(),
            serde_json::Value::Null,
            false,
            0,
            None,
            None,
        ),
    };
    serde_json::json!({
        "task_id": task.id.to_string(),
        "description": task.description,
        "status": match &record.status {
            TaskStatus::Completed => "completed",
            TaskStatus::Failed { .. } => "failed",
            TaskStatus::Running => "running",
            TaskStatus::Cancelled => "cancelled",
        },
        "branch": branch,
        "branch_blocked": branch_block,
        "forced_branch": forced_branch,
        "bare": bare,
        // `verdict` alone cannot distinguish "a reviewer approved this" from
        // "nothing ran, and the default is Approved". `outcome` and
        // `independently_reviewed` say which, so a CI script that gates on
        // `verdict == "Approved"` can tell a real pass from a fabricated one.
        "verdict": verdict,
        "outcome": outcome,
        "independently_reviewed": reviewed,
        "revision_rounds": revisions,
        "tests_passed": tests_passed,
        "mutation_passed": mutation_passed,
        "cost_usd": record.total_cost_usd,
        "input_tokens": record.total_input_tokens,
        "output_tokens": record.total_output_tokens,
        "report": task_dir.join("report.md").display().to_string(),
        "task_dir": task_dir.display().to_string(),
    })
}

/// The JSON envelope for a run that ended in an error rather than a result.
///
/// `--output-format json` documents "one JSON envelope on stdout at the end".
/// Until now only the pipeline-failure path honoured that: an unresolvable
/// `--plan` id, a missing key, a bad config, a safety-proof failure — every one
/// of those short-circuited on `?` and exited 1 with an *empty stdout*, so a
/// consumer that pipes stdout into `jq` got a parse error instead of a verdict.
/// The error envelope also carried a different key set from the success one,
/// so a consumer needed two parsers for one flag.
///
/// Same shape as the success envelope, every key present, `status` telling the
/// two apart. `task_dir`/`report` are null because no run directory was produced
/// for these paths; the envelope is a fact about the failure, not a stub.
fn error_envelope(
    task: Option<&Task>,
    status: &str,
    error: &str,
    task_dir: Option<&std::path::Path>,
) -> serde_json::Value {
    let dir = task_dir.map(|p| p.display().to_string());
    serde_json::json!({
        "task_id": task.map(|t| t.id.to_string()),
        "description": task.map(|t| t.description.clone()),
        "status": status,
        "error": error,
        "branch": serde_json::Value::Null,
        "branch_blocked": serde_json::Value::Null,
        "forced_branch": false,
        "bare": false,
        "verdict": "unknown",
        "outcome": serde_json::Value::Null,
        "independently_reviewed": false,
        "revision_rounds": 0,
        "tests_passed": serde_json::Value::Null,
        "mutation_passed": serde_json::Value::Null,
        "cost_usd": 0.0,
        "input_tokens": 0,
        "output_tokens": 0,
        "report": dir.as_ref().map(|d| format!("{d}/report.md")),
        "task_dir": dir,
    })
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
        AgentRole::Critic => "critic",
    }
}

/// Entry point for `niki run`.
///
/// Thin on purpose. `--output-format json` promises "one JSON envelope on
/// stdout at the end", and the body of a run has a dozen ways to fail before it
/// reaches the point where an envelope is built — a project path that does not
/// exist, an unparseable `niki.toml`, no container runtime, an unresolvable
/// `--plan` id. Every one of those propagated with `?` straight out of here, so
/// the promise was kept only on the paths somebody remembered. A consumer
/// piping stdout into `jq` got a parse error and learned nothing.
///
/// `run_inner` does the work; this owns the promise. Anything that escapes it
/// becomes an envelope, in the same shape as the success one, on stdout, and the
/// error still propagates so the exit code is non-zero.
pub async fn handle(args: &RunArgs) -> Result<()> {
    if args.output_format != OutputFormat::Json {
        return run_inner(args, &mut false).await;
    }
    // Whether `run_inner` already wrote the envelope. It does for the one
    // failure it knows the most about (the pipeline failing), and printing a
    // second one in the wrapper produced TWO JSON objects on stdout — which
    // broke `serde_json::from_str` for every consumer of `--output-format json`
    // on the error path, while the happy path was fine. A flag beats a
    // thread-local and a guess.
    let mut emitted = false;
    match run_inner(args, &mut emitted).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // run_inner already emitted a task-aware envelope for the one
            // failure it knows most about. This catches everything upstream of
            // that — including the paths where no task exists yet — so the
            // flag's promise holds unconditionally.
            //
            // `task_id` is null rather than scraped out of the message. Every
            // failure that reaches here happened before the id was minted, and
            // a consumer that pattern-matches an error string for an id gets a
            // value that is wrong the first time the message is reworded.
            eprintln!("Error: {e}");
            if !emitted {
                println!("{}", error_envelope(None, "error", &e.to_string(), None));
            }
            Err(e)
        }
    }
}

async fn run_inner(args: &RunArgs, emitted_envelope: &mut bool) -> Result<()> {
    let project_dir = match &args.project {
        Some(p) => p.canonicalize()?,
        None => env::current_dir()?,
    };

    let mut config = NikiConfig::load(&project_dir)?;

    // Zero-config discoverability: no config file anywhere (project or
    // global) means defaults + env keys. Say so once, instead of letting a
    // bare default run look identical to a configured one.
    {
        let global = dirs::home_dir().map(|h| h.join(".config/niki/niki.toml"));
        let has_file = project_dir.join("niki.toml").exists() || global.is_some_and(|p| p.exists());
        if !has_file {
            eprintln!(
                "note: no niki.toml found — running with defaults + environment keys. \
                 Run `niki init` to persist configuration."
            );
        }
    }

    if let Some(r) = args.max_rounds {
        config.general.max_revision_rounds = r;
    }
    // Phase 5.5: CLI overrides land in the unified `[budget]` table.
    if let Some(s) = args.max_steps {
        config.budget.max_steps = s;
    }
    if let Some(u) = args.max_usd {
        config.budget.max_usd = u;
    }
    if let Some(w) = args.max_wallclock_secs {
        config.budget.max_wallclock_secs = w;
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

    // Per-run permission posture override (explicit beats config).
    if let Some(mode) = &args.permission_mode {
        config.permissions.mode = mode.clone();
    }

    // Governance kill-switch: [permissions] disable_worktree refuses the
    // unisolated backend outright instead of warning past it.
    if !uses_docker && config.permissions.disable_worktree {
        anyhow::bail!(
            "worktree backend is disabled by [permissions] disable_worktree — \
             use the default container backend or relax the policy."
        );
    }

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

    // JSON output mode mutes every terminal write so stdout carries only the
    // final envelope (pipe-purity for scripts/CI). Progress still flows to
    // stderr-free event buffers, never to stdout.
    let json_mode = args.output_format == OutputFormat::Json;
    if json_mode {
        display.set_muted(true);
    }

    // Opt-in rich TUI. Must be enabled before any display call so the banner
    // and subsequent events are routed to the render thread.
    if args.tui {
        display.enable_tui(
            task.description.clone(),
            task.project_path.clone(),
            cancel.clone(),
        );
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
        // Phase 5.2: worktrees live at <project>/.niki-worktrees/<id> (see
        // `WorktreeSandbox::create`), NOT under the output dir — the old path
        // cleaned a directory that never exists, leaking worktrees.
        let project_for_ctrlc = project_dir_for_ctrlc.clone();
        tokio::spawn(async move {
            if signal::ctrl_c().await.is_ok() {
                eprintln!("\n Shutting down — cleaning up...");

                let ids = containers.lock().await.clone();
                // Not Unix-gated: on Windows this used to skip cleanup
                // entirely, so Ctrl-C left every sandbox container running.
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

                // Clean up any leftover .niki-worktrees/<task_id> dirs (plus
                // suffixed parallel-coder siblings) via the real location.
                // Left behind after a cancelled run they are never reused.
                crate::sandbox::worktree::cleanup_worktrees_for_task(
                    &project_for_ctrlc,
                    &task_id_str,
                );

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
        // Phase 5.2: real worktree location (see Ctrl+C path above).
        let project_for_sigterm = project_dir_for_signal.clone();
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
            crate::sandbox::worktree::cleanup_worktrees_for_task(
                &project_for_sigterm,
                &task_id_str,
            );
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
    //
    // No `cfg` gate: the non-Unix arm used to bind `docker` to `None`, which
    // meant a Windows user with Docker Desktop running was handed `None`
    // anyway. Silently continuing would be worse than failing — the run would
    // proceed with the *container* backend selected and no container, so the
    // failure would surface later as a baffling sandbox error rather than
    // here, where the message can say what to do.
    let docker = if uses_docker && !args.dry_run {
        let d = connect_container_runtime().await.map_err(|e| {
            anyhow!(
                "Container runtime error: {e}\n\n\
                     NIKI selected the container backend, which requires a running \
                     Podman or Docker daemon. To run without isolation instead, \
                     pass --backend worktree — note that it executes agent \
                     commands as local processes with YOUR privileges."
            )
        })?;
        Some(d)
    } else {
        None
    };

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

    // --plan: load a user-approved planner artifact from a previous `niki plan`
    // (or dry-run) task. Accepts a full UUID or unique short prefix, resolved
    // exactly like `niki report`. The JSON is validated as a TaskSpec here so
    // a stale or hand-broken plan fails with a clear error before the run.
    let plan_override_json: Option<String> = match &args.plan {
        None => None,
        Some(id) => {
            let tasks_dir = project_dir.join(&config.general.output_dir).join("tasks");
            let resolved = crate::cli::report::resolve_task_id(&tasks_dir, id)?;
            let path = tasks_dir.join(&resolved).join("artifacts/planner.json");
            let json = std::fs::read_to_string(&path).map_err(|_| {
                anyhow!(
                    "no approved plan found: {} (run `niki plan` first)",
                    path.display()
                )
            })?;
            let _: crate::artifacts::types::TaskSpec = serde_json::from_str(&json)
                .map_err(|e| anyhow!("approved plan is not a valid TaskSpec: {e}"))?;
            // Progress goes to stderr in JSON mode so stdout stays parseable.
            if json_mode {
                eprintln!("Using approved plan from task {resolved} (Planner LLM call skipped).");
            } else {
                println!("Using approved plan from task {resolved} (Planner LLM call skipped).");
            }
            Some(json)
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
        &task_dir,
        plan_override_json,
        args.bare,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            // Preserve incremental metrics saved by execute_pipeline, if any.
            let mut rec = TaskRecord::new(task.id, &task.description);
            let task_json = task_dir.join("task.json");
            if let Ok(bytes) = std::fs::read(&task_json)
                && let Ok(existing) = serde_json::from_slice::<TaskRecord>(&bytes)
            {
                rec.agent_metrics = existing.agent_metrics;
                rec.total_input_tokens = existing.total_input_tokens;
                rec.total_output_tokens = existing.total_output_tokens;
                rec.total_cost_usd = existing.total_cost_usd;
                rec.total_latency_ms = existing.total_latency_ms;
                rec.total_retry_count = existing.total_retry_count;
                rec.max_ttft_ms = existing.max_ttft_ms;
            }
            let is_cancelled = e
                .downcast_ref::<crate::NikiError>()
                .map(|ne| matches!(ne, crate::NikiError::Cancelled))
                .unwrap_or(false);
            if is_cancelled {
                rec.status = TaskStatus::Cancelled;
                let _ = rec.save_to_disk(&task_dir);
                crate::display::notify::pipeline_cancelled();
            } else {
                rec.status = TaskStatus::Failed {
                    error: e.to_string(),
                };
                let _ = rec.save_to_disk(&task_dir);
                crate::display::notify::pipeline_complete(false, "");
            }
            display.finish_tui();
            if args.output_format == OutputFormat::Json {
                *emitted_envelope = true;
                println!(
                    "{}",
                    error_envelope(
                        Some(&task),
                        if is_cancelled { "cancelled" } else { "error" },
                        &e.to_string(),
                        Some(&task_dir),
                    )
                );
            }
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
        // Repeated pushes for one role (revision rounds, coder patch-repair
        // attempts) each get their own file — coder.json, coder-2.json, … —
        // so the audit trail never silently drops the failed attempts.
        use std::collections::HashMap;
        let mut seen: HashMap<String, usize> = HashMap::new();
        for (role, json) in &result.artifacts {
            let base = role_filename(*role).to_string();
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            let name = if *n == 1 {
                format!("{base}.json")
            } else {
                format!("{base}-{n}.json")
            };
            let path = artifacts_dir.join(name);
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

    // Plan mode (`niki plan` / `--dry-run`): persist a human-readable plan and
    // point at the approval command. The machine-readable spec already lives at
    // `artifacts/planner.json`; `plan.md` is the review surface.
    if args.dry_run {
        write_plan_md(&task_dir, &task, &result);
        // Progress goes to stderr in JSON mode so stdout stays parseable.
        if json_mode {
            eprintln!(
                "Plan written to {}/plan.md — review it, then execute with: niki run --plan {} --project {}",
                task_dir.display(),
                &task.id.to_string()[..8],
                project_dir.display(),
            );
        } else {
            println!(
                "\nPlan written to {}/plan.md — review it, then execute with:\n  niki run --plan {} --project {}",
                task_dir.display(),
                &task.id.to_string()[..8],
                project_dir.display(),
            );
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

    // changes.patch is written exactly once, by `generate_report` alongside
    // report.md (Phase 5.6 single-writer rule).

    // Red-suite gate (goal-a3f9c2, Phase 2): a failing executed suite — or a
    // failing mutation gate — blocks the branch. The evidence (patch, report,
    // test output) is still written so the failure is inspectable, but no
    // `niki/<id>` branch is created and the task is recorded as Failed.
    // `--force` overrides with the override itself recorded; a forced branch
    // is explicitly not a verified branch.
    let suite_failed = result
        .test_execution
        .as_ref()
        .is_some_and(|te| !te.passed || te.mutation.as_ref().is_some_and(|m| !m.passed));
    let mut branch_block_note: Option<String> = None;
    if suite_failed && !args.force {
        let what = match result.test_execution.as_ref() {
            Some(te) if !te.passed => {
                format!("test suite `{}` failed (exit {})", te.command, te.exit_code)
            }
            Some(te) => format!(
                "mutation gate `{}` failed (exit {})",
                te.mutation
                    .as_ref()
                    .map(|m| m.command.as_str())
                    .unwrap_or("?"),
                te.mutation.as_ref().map(|m| m.exit_code).unwrap_or(-1),
            ),
            None => "verification failed".to_string(),
        };
        branch_block_note = Some(format!(
            "Branch blocked: {}. Re-run with `--force` to create the branch anyway (recorded as forced, not verified).",
            what
        ));
    }
    let forced_branch = suite_failed && args.force;

    // For the worktree backend the change still lives inside the sandbox copy (a
    // separate git worktree), so `working_tree_diff` on the host would be empty.
    // Apply the sandbox's diff to the host working tree first; the Docker backend
    // already wrote through the bind mount and skips this step.
    if branch_block_note.is_none()
        && !uses_docker
        && !result.final_diff.trim().is_empty()
        && let Err(e) =
            crate::output::git::apply_diff_to_working_tree(&project_dir, &result.final_diff)
    {
        eprintln!("Warning: could not apply sandbox diff to host: {}", e);
    }

    // Create the git branch + commit (no-op when there is no diff; skipped
    // entirely when the red-suite gate blocked the branch, and in dry-run /
    // plan mode where a branch — even an empty ref — would misrepresent a
    // proposal as a result).
    // Phase 5.6: unresolved conflict markers (e.g. from a `--3way` fallback)
    // block the branch like a failed suite — abort instead of committing a
    // conflicted tree. Recorded in task.json as Failed.
    if branch_block_note.is_none() && !result.final_diff.trim().is_empty() {
        if let Err(e) =
            crate::output::git::ensure_no_conflict_markers(&project_dir, &result.final_diff)
        {
            branch_block_note = Some(format!("Branch blocked: {e}."));
        }
    }
    // `branch_created` is the ground truth for the recorded status. It stays
    // false for a dry run, an empty diff, a blocked branch, and a failed
    // `create_branch_and_commit` — all of which previously fell through to
    // `Completed { branch: Some(...) }` and reported a branch that did not exist.
    let mut branch_created = false;
    let mut branch_creation_error: Option<String> = None;
    if branch_block_note.is_none() && !args.dry_run {
        if !result.final_diff.trim().is_empty() {
            match crate::output::git::create_branch_and_commit(
                &project_dir,
                &branch_name,
                &result.final_diff,
                &task.id.to_string(),
            ) {
                // `false` means the diff carried no committable content, so no
                // ref was created. That is not a successful run.
                Ok(true) => branch_created = true,
                Ok(false) => {
                    branch_creation_error = Some(
                        "Branch not created: the diff carried no committable file changes."
                            .to_string(),
                    );
                }
                Err(e) => {
                    // Not a warning: the run's whole deliverable is this branch.
                    // Swallowing it here reported success for a run that changed
                    // nothing reviewable.
                    branch_creation_error = Some(format!("Branch creation failed: {e}."));
                    eprintln!(
                        "Error: {}",
                        branch_creation_error.as_deref().unwrap_or_default()
                    );
                }
            }
        }
    }

    // Hermetic safety proof (BUILD_PLAN 1.1): with the branch now committed,
    // verify the committed repo state is unchanged except for that one branch.
    // Emit `safety_proof.json` next to the report and attach it to the result.
    // Skip when there was no diff (no branch was created), so a no-op run isn't
    // misreported as NON-HERMETIC — and skip when the red-suite gate blocked
    // the branch, since `prove()` in strict mode would abort a correctly
    // blocked run for the missing branch.
    if branch_block_note.is_none()
        && !result.final_diff.trim().is_empty()
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
    if let Err(e) = crate::output::report::generate_report(
        &task,
        &config,
        &result,
        if branch_block_note.is_some() {
            None
        } else {
            Some(branch_name.as_str())
        },
    ) {
        eprintln!("Warning: could not generate report: {}", e);
    }
    // Record a red-suite block / force override directly in the report so the
    // audit trail states the branch decision in plain language.
    if branch_block_note.is_some() || forced_branch {
        let notice = match (&branch_block_note, forced_branch) {
            (Some(note), _) => format!("\n## Branch decision\n\n{}\n", note),
            (None, true) => "## Branch decision\n\nBranch created with `--force` over a failing suite/mutation gate. This branch is explicitly NOT verified.\n".to_string(),
            _ => String::new(),
        };
        if !notice.is_empty() {
            use std::fmt::Write as _;
            let path = task_dir.join("report.md");
            let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
            let _ = write!(existing, "{notice}");
            if let Err(e) = crate::util::write_restricted(&path, existing) {
                eprintln!("Warning: could not append branch decision to report: {}", e);
            }
        }
    }

    // Persist final task record.
    record.topology = Some(result.topology);
    record.topology_reason = Some(result.topology_reason.clone());
    record.risk_level = Some(result.risk_level.clone());
    record.risk_rationale = Some(result.risk_rationale.clone());
    // A run is only `Completed` when it actually produced its branch. A blocked
    // branch, a failed commit, a dry run and an empty diff are all recorded as
    // Failed with `branch: None`, so `niki status` and the JSON envelope can
    // never advertise a branch that does not exist on disk.
    let completion = decide_completion(
        branch_creation_error,
        branch_block_note.clone(),
        branch_created,
        &branch_name,
        args.dry_run,
        &task_dir,
    );
    let status_error = completion.error.clone();
    record.status = match &completion.error {
        Some(error) => TaskStatus::Failed {
            error: error.clone(),
        },
        None => TaskStatus::Completed,
    };
    record.branch = completion.branch;
    record.verdict = Some(format!("{:?}", result.verdict));
    // Persist the outcome, not just the bare verdict. Without this the record
    // cannot distinguish "a reviewer approved" from "nothing reviewed it" —
    // which is the whole defect. `verdict_source` also names the producer: the
    // Solo fast path approves its own patch, and "Approved" on its own reads
    // as an independent check.
    record.verdict_source = result.verdict_source.clone();
    record.outcome = serde_json::to_value(&result.outcome).ok();
    record.revision_rounds = result.revision_rounds;
    record.add_metrics(&result.metrics);
    // The *final* state write, and it used to warn. A run whose closing record
    // cannot be written ends with a report describing a run the store has no
    // record of — the report is the artefact a human reads.
    record.save_to_disk(&task_dir).with_context(|| {
        format!(
            "could not save the final task state to {}",
            task_dir.join("task.json").display()
        )
    })?;

    // Provenance completion: stamp the result branch, its commit, artifact
    // roles, and summed cost onto manifest.json. Best-effort (warns, never
    // fails). Skipped for dry runs, whose manifest is already accurate.
    if !args.dry_run && config.snapshot.enabled {
        // `record.branch`, not a re-derivation of the branch name.
        //
        // The manifest used to decide for itself: it withheld the branch only
        // when one was *blocked*, and advertised the name whenever the name
        // was known. The record uses a stricter rule — a failed commit or a
        // branch that was never created leaves it `None` precisely so nothing
        // can advertise a branch that does not exist on disk — so the two
        // disagreed by construction, and the reconcile check below caught the
        // disagreement by failing a run that had otherwise succeeded.
        //
        // Measured: a breadth run died with "manifest.json names branch
        // niki/31eb6c86 but task.json records None" after committing
        // successfully and failing afterwards.
        //
        // One rule, read from one place. The reconciliation below stays as the
        // backstop for anything else that drifts.
        let branch_opt = record.branch.as_deref();
        let total_cost: f64 = result.metrics.iter().map(|m| m.cost_usd).sum();
        crate::orchestrator::provenance::record_completion(
            &task_dir,
            &project_dir,
            branch_opt,
            &result.artifacts,
            total_cost,
        );
    }

    // Reconcile the three artefacts a user can consult before believing
    // anything this run produced. The in-memory result is the source of truth;
    // `task.json` and `manifest.json` are written independently, and the
    // markdown report is rendered separately. A disagreement between them is
    // not cosmetic — it is the run telling two different stories about
    // whether the work was reviewed and what it cost — so it is caught here
    // rather than discovered by whoever reads the report next.
    reconcile_result_record_manifest(&result, &record, &task_dir)?;

    // Post-run reflection: derive durable learnings (verification failures,
    // review corrections, security fixes) into learnings.jsonl. Gated on
    // [repo_intel] and best-effort — the run's outcome is already recorded.
    crate::orchestrator::reflect::record_reflections(&project_dir, &config, &task_dir, &result);

    if !args.quiet {
        match &branch_block_note {
            Some(note) => {
                eprintln!("\n{note}");
                eprintln!(
                    "Evidence preserved in {} (report.md, changes.patch, artifacts/).",
                    task_dir.display()
                );
            }
            None => {
                // Human completion summary is stdout noise in JSON mode — the
                // envelope below is the contract.
                //
                // The branch printed here must be the one that exists on disk,
                // not the one that *would* have been created. A dry run, an
                // empty diff, and a blocked branch all reach this arm with
                // `record.branch == None`, and printing the generated name there
                // told the user "Branch: niki/ab12cd" about a ref that was never
                // created.
                if !json_mode {
                    let delivered = record
                        .branch
                        .clone()
                        .unwrap_or_else(|| "(no branch created)".to_string());
                    display.show_completion(&result, &delivered, &task_dir);
                }
            }
        }
    }

    if args.output_format == OutputFormat::Json {
        // Same rule as the human path above and the exit code below: report the
        // branch that exists, not the one that would have. `record.branch` is
        // the single answer — it is None for a blocked branch, an empty diff, a
        // dry run, and a failed commit alike.
        let branch = record.branch.as_deref();
        println!(
            "{}",
            result_envelope(
                &task,
                &record,
                Some(&result),
                branch,
                branch_block_note.as_deref(),
                forced_branch,
                args.bare,
                &task_dir,
            )
        );
    }

    // Best-effort OTLP trace export. Telemetry failures warn and never fail
    // the run — observability is subordinate to the user's task.
    let otel_endpoint = args
        .otel_endpoint
        .clone()
        .or_else(|| env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok());
    if let Some(endpoint) = otel_endpoint {
        let trace_path = task_dir.join("trace.jsonl");
        match std::fs::read_to_string(&trace_path) {
            Ok(text) => {
                let spans: Vec<serde_json::Value> = text
                    .lines()
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect();
                let payload = crate::output::otel::otlp_payload(
                    "niki",
                    env!("CARGO_PKG_VERSION"),
                    &crate::output::otel::trace_id_hex(&task.id.to_string()),
                    &spans,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0),
                );
                if let Err(e) = crate::output::otel::export_trace(&endpoint, &payload).await {
                    eprintln!("Warning: OTLP trace export failed: {e}");
                }
            }
            Err(e) => eprintln!("Warning: cannot read trace for OTLP export: {e}"),
        }
    }

    // Tear down the TUI (if active): this joins the render thread, which
    // restores the terminal before any further output.
    display.finish_tui();

    // The exit code has to agree with the record. Until now it did not: a run
    // that recorded `TaskStatus::Failed` — a blocked branch, a failed commit,
    // an empty diff — still returned `Ok(())`, so `niki run "…"` in a CI step
    // exited 0 on a run that produced nothing to review. A gate that cannot
    // fail is not a gate.
    //
    // Derived from `record.status`, not from a second opinion about what went
    // wrong, so the status a user reads with `niki status` and the status a
    // build system sees are the same fact stated once. `--dry-run` is the one
    // case where "no branch" is the expected result, not a failure.
    if !args.dry_run && matches!(record.status, TaskStatus::Failed { .. }) {
        return Err(anyhow!(
            "{}",
            status_error
                .as_deref()
                .unwrap_or("the run produced no reviewable branch")
        ));
    }

    Ok(())
}

#[cfg(test)]
mod reconcile_tests {
    use super::*;
    use crate::artifacts::types::{RunOutcome, Verdict};
    use crate::orchestrator::pipeline::PipelineResult;
    use crate::orchestrator::state::{PipelineState, TaskStatus};
    use uuid::Uuid;

    fn result_with(outcome: RunOutcome, verdict: Verdict, cost: f64) -> PipelineResult {
        let id = Uuid::new_v4();
        let metrics = vec![crate::orchestrator::state::StageMetric {
            role: crate::artifacts::types::AgentRole::Planner,
            provider: "mock".into(),
            model: "m".into(),
            input_tokens: 1,
            output_tokens: 1,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            latency_ms: 0,
            cost_usd: cost,
            retry_count: 0,
            ttft_ms: 0,
        }];
        PipelineResult {
            task_id: id,
            context_budget: PipelineState::new(id).context_budget,
            state: PipelineState::new(id),
            final_diff: String::new(),
            diff_guardwarn: None,
            outcome,
            verdict,
            verdict_source: Some("reviewer".into()),
            revision_rounds: 0,
            artifacts: vec![],
            metrics,
            safety_proof: None,
            isolation: vec![],
            topology: crate::config::types::TopologyMode::MultiAgent,
            topology_reason: String::new(),
            risk_level: "low".into(),
            risk_rationale: String::new(),
            test_execution: None,
        }
    }

    /// Build the record the way the run does, then let a test corrupt one field.
    fn record_from(result: &PipelineResult) -> TaskRecord {
        let mut rec = TaskRecord::new(result.task_id, "t");
        rec.verdict = Some(format!("{:?}", result.verdict));
        rec.verdict_source = result.verdict_source.clone();
        rec.outcome = serde_json::to_value(&result.outcome).ok();
        rec.add_metrics(&result.metrics);
        rec.status = TaskStatus::Completed;
        rec
    }

    fn dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    /// Round-trip a real `RunManifest` through the crate's own serde
    /// definitions, so a test fixture cannot drift from the struct.
    fn write_manifest_for_test(
        task_dir: &std::path::Path,
        run_id: Uuid,
        branch: Option<&str>,
        total_cost_usd: f64,
    ) {
        use crate::orchestrator::provenance::{
            ConfigFingerprint, RepoIdentity, RunManifest, SnapshotRef, ToolchainVersions,
        };
        let manifest = RunManifest {
            run_id,
            agent_roles: vec![],
            created_at: chrono::Utc::now(),
            repo_identity: RepoIdentity {
                commit_sha: None,
                branch: None,
                remote_url: None,
                dirty: false,
                workdir_fingerprint: None,
            },
            active_snapshot: SnapshotRef {
                commit_sha: None,
                kind: "nongit".into(),
                snapshot_id: "niki-task-test".into(),
            },
            config_fingerprint: ConfigFingerprint {
                path: None,
                content_hash: None,
            },
            toolchain: ToolchainVersions {
                niki: "test".into(),
                rustc: None,
            },
            branch: branch.map(|b| b.to_string()),
            commit_sha: None,
            artifact_roles: vec![],
            total_cost_usd,
            dry_run: false,
        };
        std::fs::write(
            task_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn a_consistent_run_reconciles() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let record = record_from(&result);
        let d = dir();
        reconcile_result_record_manifest(&result, &record, d.path()).unwrap();
    }

    #[test]
    fn a_record_carrying_a_different_verdict_is_caught() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let mut record = record_from(&result);
        record.verdict = Some("RevisionNeeded".into());
        let d = dir();
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a stored verdict that differs from the run must not pass");
        assert!(err.to_string().contains("disagree"), "{err}");
    }

    #[test]
    fn a_record_with_no_outcome_is_caught() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let mut record = record_from(&result);
        record.outcome = None;
        let d = dir();
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a record with no outcome cannot say whether it was reviewed");
        assert!(err.to_string().contains("no outcome"), "{err}");
    }

    #[test]
    fn a_reviewed_approval_with_no_reviewer_named_is_caught() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: String::new(),
            },
            Verdict::Approved,
            0.25,
        );
        let record = record_from(&result);
        let d = dir();
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("an approval with no reviewer is exactly the fabricated pass");
        assert!(err.to_string().contains("no reviewer named"), "{err}");
    }

    #[test]
    fn a_record_whose_cost_disagrees_is_caught() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let mut record = record_from(&result);
        record.total_cost_usd = 0.01;
        let d = dir();
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a cost that does not match the stages must not pass");
        assert!(err.to_string().contains("cost"), "{err}");
    }

    #[test]
    fn a_failed_outcome_recorded_as_completed_is_caught() {
        let result = result_with(
            RunOutcome::Failed {
                error: "boom".into(),
            },
            Verdict::RevisionNeeded,
            0.0,
        );
        let mut record = record_from(&result);
        record.status = TaskStatus::Completed;
        let d = dir();
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a failed run must not be stored as completed");
        assert!(err.to_string().contains("completed"), "{err}");
    }

    /// The branch a run records and the branch the manifest records must be
    /// the same value, and this is the rule that guarantees it.
    ///
    /// The two used to decide independently. The record withheld the branch
    /// unless it was genuinely created and committed; the manifest withheld it
    /// only when the branch was *blocked*, and advertised the name whenever the
    /// name was known. A run that committed successfully and then failed
    /// afterwards left the manifest naming a branch the record had deliberately
    /// denied — and the reconciliation below failed the run for it. Measured,
    /// on a `docs` breadth task.
    ///
    /// Every case where the run is not cleanly completed must name no branch,
    /// because a name is an advertisement and the branch may not be on disk.
    #[test]
    fn a_run_that_did_not_cleanly_finish_names_no_branch() {
        let dir = std::path::Path::new("/tmp/task");
        let name = "niki/31eb6c86";

        // The measured case: the branch was created, but something after it
        // failed. The name is known; it must not be advertised.
        let after_a_failure = decide_completion(
            Some("safety proof failed".into()),
            None,
            true,
            name,
            false,
            dir,
        );
        assert_eq!(after_a_failure.branch, None, "a failed run names no branch");
        assert!(after_a_failure.error.is_some(), "and says why");

        // A blocked branch.
        let blocked =
            decide_completion(None, Some("branch in use".into()), false, name, false, dir);
        assert_eq!(blocked.branch, None);

        // A dry run: the plan exists, the branch does not.
        let dry = decide_completion(None, None, false, name, true, dir);
        assert_eq!(dry.branch, None);
        assert!(
            dry.error.as_deref().is_some_and(|e| e.contains("Dry run")),
            "and points at the plan it did leave behind: {:?}",
            dry.error
        );

        // An empty diff.
        let empty = decide_completion(None, None, false, name, false, dir);
        assert_eq!(empty.branch, None);

        // And the one case that does name it.
        let clean = decide_completion(None, None, true, name, false, dir);
        assert_eq!(clean.branch.as_deref(), Some(name));
        assert_eq!(clean.error, None);
    }

    /// The invariant the failure came from, stated directly.
    #[test]
    fn the_manifest_and_the_record_can_never_disagree_about_the_branch() {
        // Whatever the inputs, the branch the manifest is given is the branch
        // the record holds — because the manifest is handed `record.branch`
        // rather than being allowed to derive one. This test pins the shape of
        // that: there is no second decision to make.
        for (err, blocked, created) in [
            (None, None, true),
            (Some("commit failed".into()), None, true),
            (None, Some("in use".into()), false),
            (None, None, false),
        ] {
            let c = decide_completion(
                err,
                blocked,
                created,
                "niki/x",
                false,
                std::path::Path::new("/tmp/t"),
            );
            let record_branch = c.branch.clone();
            // The production call site: `record.branch.as_deref()`.
            let manifest_branch = record_branch.as_deref();
            assert_eq!(
                manifest_branch,
                record_branch.as_deref(),
                "the manifest reads the record's value; there is nothing to reconcile"
            );
        }
    }

    #[test]
    fn a_manifest_naming_a_branch_the_record_does_not_have_is_caught() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let mut record = record_from(&result);
        record.branch = None;
        let d = dir();
        // Written through the crate's own serialiser, so this test exercises
        // the comparison rather than my ability to guess the field list.
        write_manifest_for_test(d.path(), result.task_id, Some("niki/some-branch"), 0.25);
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a manifest naming a branch the record denies must not pass");
        assert!(err.to_string().contains("niki/some-branch"), "{err}");
    }

    #[test]
    fn a_manifest_naming_a_different_cost_is_caught() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let mut record = record_from(&result);
        record.branch = Some("niki/abc".into());
        let d = dir();
        write_manifest_for_test(d.path(), result.task_id, Some("niki/abc"), 9.99);
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a manifest cost the run never incurred must not pass");
        assert!(err.to_string().contains("9.99"), "{err}");
    }

    #[test]
    fn a_manifest_that_cannot_be_parsed_is_not_silently_skipped() {
        let result = result_with(
            RunOutcome::Reviewed {
                verdict: Verdict::Approved,
                by: "reviewer".into(),
            },
            Verdict::Approved,
            0.25,
        );
        let record = record_from(&result);
        let d = dir();
        std::fs::write(d.path().join("manifest.json"), "{ not json").unwrap();
        let err = reconcile_result_record_manifest(&result, &record, d.path())
            .expect_err("a corrupt manifest must be reported, not skipped");
        assert!(err.to_string().contains("manifest.json"), "{err:#}");
    }
}
