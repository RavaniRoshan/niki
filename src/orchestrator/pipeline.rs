use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::artifacts::types::{
    AgentRole, CodeDiff, ReviewVerdict, SecurityVerdict, Synthesis, TaskSpec, TestReport, Verdict,
};
use crate::config::NikiConfig;
use crate::config::types::PipelineStageConfig;
use crate::cost::compute_cost;
use crate::display::agent_stream::AgenticDisplay;
use crate::knowledge::indexer::index_project;
use crate::llm::provider::{create_provider, LlmProvider};
use crate::orchestrator::state::StageMetric;
use crate::sandbox::{ActiveContainers, create_sandbox, SandboxBackend};

use crate::agents::run_agent;
use minijinja::context;

pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub project_path: PathBuf,
}

pub struct PipelineResult {
    pub task_id: Uuid,
    pub state: super::state::PipelineState,
    pub final_diff: String,
    pub verdict: Verdict,
    pub revision_rounds: u32,
    /// Raw JSON artifacts produced by each agent, in execution order.
    pub artifacts: Vec<(AgentRole, String)>,
    /// Per-agent cost & latency, in execution order.
    pub metrics: Vec<StageMetric>,
}

/// Typed output of a single pipeline stage, used for role-specific handling.
pub enum RoleOutput {
    Planner(TaskSpec),
    Coder(CodeDiff),
    Tester(TestReport),
    Reviewer(ReviewVerdict),
    Synthesizer(Synthesis),
    SecurityAuditor(SecurityVerdict),
}

/// The ordered stages to run, honoring a user-defined `[pipeline]` topology when
/// present, otherwise the classic Planner → Coder → Tester → Reviewer wiring.
/// When `[security] enabled = true`, an independent `SecurityAuditor` stage is
/// appended after the Reviewer (#4).
pub fn resolve_stages(config: &NikiConfig) -> Vec<PipelineStageConfig> {
    let mut stages = if !config.pipeline.stages.is_empty() {
        config.pipeline.stages.clone()
    } else {
        vec![
            stage(AgentRole::Planner, &config.agents.planner.provider, &config.agents.planner.model),
            stage(AgentRole::Coder, &config.agents.coder.provider, &config.agents.coder.model),
            stage(AgentRole::Tester, &config.agents.tester.provider, &config.agents.tester.model),
            stage(AgentRole::Reviewer, &config.agents.reviewer.provider, &config.agents.reviewer.model),
        ]
    };

    if config.security.enabled {
        let (provider, model) = security_stage_target(config);
        if !stages.iter().any(|s| s.role == AgentRole::SecurityAuditor) {
            stages.push(stage(AgentRole::SecurityAuditor, &provider, &model));
        }
    }

    // Parallel-coder mode (#3) always reconciles its coders through a
    // Synthesizer; inject the stage when enabled so the pipeline can find it.
    if config.parallel.enabled && config.parallel.coder_count > 1 {
        if !stages.iter().any(|s| s.role == AgentRole::Synthesizer) {
            stages.push(stage(
                AgentRole::Synthesizer,
                &config.agents.synthesizer.provider,
                &config.agents.synthesizer.model,
            ));
        }
    }
    stages
}

/// Resolve the provider/model for the injected SecurityAuditor stage: explicit
/// `[security] provider/model` overrides win, otherwise fall back to the
/// `[agents] security_auditor` binding.
fn security_stage_target(config: &NikiConfig) -> (String, String) {
    let agent = &config.agents.security_auditor;
    (
        config
            .security
            .provider
            .clone()
            .unwrap_or_else(|| agent.provider.clone()),
        config
            .security
            .model
            .clone()
            .unwrap_or_else(|| agent.model.clone()),
    )
}

/// The toolchain the sandbox image/process is expected to pre-bake. Verified up
/// front so a misconfigured environment fails fast instead of hanging on a runtime
/// install. The configured `extra_packages` are always included.
fn required_tools(config: &NikiConfig) -> Vec<String> {
    let mut required = config.docker.extra_packages.clone();
    for tool in ["git", "node", "npm", "python3"] {
        if !required.iter().any(|p| p == tool) {
            required.push(tool.to_string());
        }
    }
    required
}

fn stage(role: AgentRole, provider: &str, model: &str) -> PipelineStageConfig {
    PipelineStageConfig {
        role,
        provider: provider.to_string(),
        model: model.to_string(),
        skip: false,
    }
}

/// A pipeline always needs a Planner to produce the spec; inject one if the
/// user's topology omitted it.
fn ensure_planner(stages: Vec<PipelineStageConfig>, config: &NikiConfig) -> Vec<PipelineStageConfig> {
    if !stages.iter().any(|s| s.role == AgentRole::Planner) {
        let mut out = vec![stage(AgentRole::Planner, &config.agents.planner.provider, &config.agents.planner.model)];
        out.extend(stages);
        out
    } else {
        stages
    }
}

fn provider_for(provider: &str, config: &NikiConfig) -> Result<Box<dyn LlmProvider>> {
    let cfg = config
        .providers
        .get(provider)
        .ok_or_else(|| crate::NikiError::Config(format!("Provider '{}' not configured", provider)))?;
    create_provider(provider, cfg)
}

/// Read the current on-disk contents of every file the spec wants to modify, so the
/// Coder can produce a diff that edits the existing code instead of recreating it.
fn build_current_files(spec: &TaskSpec, project_path: &Path) -> String {
    let mut out = String::new();
    for fc in &spec.files_to_modify {
        let p = project_path.join(&fc.path);
        match std::fs::read_to_string(&p) {
            Ok(content) => {
                out.push_str(&format!(
                    "### File: {} (action: {:?})\n```\n{}\n```\n\n",
                    fc.path, fc.action, content
                ));
            }
            Err(_) => {
                out.push_str(&format!(
                    "### File: {} (does not exist yet — will be created)\n\n",
                    fc.path
                ));
            }
        }
    }
    if out.is_empty() {
        out.push_str("(no files listed to modify)");
    }
    out
}

/// Run `count` Coder agents concurrently (#3). Each coder is isolated in its
/// OWN git worktree sandbox so its changes can never collide with the others
/// (docker bind-mounts would share the host dir and conflict). Each coder's
/// patch is applied to its own worktree and its produced `CodeDiff` is
/// returned; the caller reconciles them through the Synthesizer stage.
///
/// Coders run as independent tokio tasks, each owning a forked `AgenticDisplay`
/// that forwards events to the single visible TUI. They share the provider and
/// the task spec, but no mutable pipeline state, so there is no contention.
#[allow(clippy::too_many_arguments)]
async fn run_parallel_coders(
    count: u32,
    coder_llm: Arc<dyn LlmProvider>,
    model: &str,
    provider: &str,
    task_spec: &TaskSpec,
    knowledge_str: &str,
    project_path: &Path,
    config: &NikiConfig,
    containers: ActiveContainers,
    task_id: &Uuid,
    base_display: &AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
) -> Result<Vec<CodeDiff>> {
    let mut tasks = Vec::new();
    for _ in 0..count.max(1) {
        let llm = coder_llm.clone();
        let model = model.to_string();
        let provider = provider.to_string();
        let task_spec = task_spec.clone();
        let knowledge = knowledge_str.to_string();
        let project_path = project_path.to_path_buf();
        let config = config.clone();
        let containers = containers.clone();
        let task_id = *task_id;
        let mut disp = base_display.fork();

        tasks.push(tokio::spawn(async move {
            // Own worktree sandbox per coder → isolated changes.
            let sandbox = create_sandbox(
                SandboxBackend::Worktree,
                None,
                AgentRole::Coder,
                &project_path,
                &task_id,
                &config.docker,
                containers,
            )
            .await?;
            sandbox.ensure_tools(&required_tools(&config)).await?;

            let mut local_metrics: Vec<StageMetric> = Vec::new();
            let (_json, _summary, output) = run_role(
                AgentRole::Coder,
                &*llm,
                &model,
                &provider,
                &task_spec,
                "",
                "",
                0,
                &knowledge,
                &project_path,
                None,
                &mut disp,
                &mut local_metrics,
            )
            .await?;

            let diff = match output {
                RoleOutput::Coder(d) => d,
                _ => unreachable!("coder stage yields a CodeDiff"),
            };
            // Apply to this coder's own worktree so `get_diff` reflects only its change.
            if let Err(e) = sandbox.apply_patch(&diff.unified_diff, &project_path).await {
                eprintln!("Warning: coder worktree patch failed: {}", e);
            }
            let _wt_diff = sandbox.get_diff().await?;
            sandbox.destroy().await?;
            Ok::<_, anyhow::Error>((diff, local_metrics))
        }));
    }

    let mut out = Vec::new();
    for t in tasks {
        let (diff, local_metrics) = t
            .await
            .map_err(|e| anyhow::anyhow!("parallel coder task failed: {}", e))??;
        metrics.extend(local_metrics);
        out.push(diff);
    }
    Ok(out)
}

/// Run one agent: stream its output, measure latency, compute cost, record a
/// metric, and return the raw JSON artifact.
async fn run_stage(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    provider: &str,
    template_name: &str,
    ctx: minijinja::Value,
    schema_path: &str,
    display: &mut AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
) -> Result<String> {
    let start = Instant::now();
    let (json, usage) = run_agent(role, llm, model, template_name, ctx, schema_path, display).await?;
    let latency_ms = start.elapsed().as_millis() as u64;
    let cost_usd = compute_cost(provider, model, &usage);
    metrics.push(StageMetric {
        role,
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        latency_ms,
        cost_usd,
    });
    Ok(json)
}

/// Prompt template + JSON schema for a given role.
fn role_prompt(role: AgentRole) -> (&'static str, &'static str) {
    match role {
        AgentRole::Planner => ("planner.md", "schemas/task_spec.schema.json"),
        AgentRole::Coder => ("coder.md", "schemas/code_diff.schema.json"),
        AgentRole::Tester => ("tester.md", "schemas/test_report.schema.json"),
        AgentRole::Reviewer => ("reviewer.md", "schemas/review_verdict.schema.json"),
        AgentRole::Synthesizer => ("synthesizer.md", "schemas/synthesis.schema.json"),
        AgentRole::SecurityAuditor => ("security_auditor.md", "schemas/security_audit.schema.json"),
    }
}

/// Run a role-aware stage: build the role-specific prompt context, execute the
/// agent, parse the artifact, and render a summary for display.
///
/// Only body stages (Coder/Tester/Reviewer) flow through here; the Planner is
/// handled as the pipeline entry point in `execute_pipeline`.
#[allow(clippy::too_many_arguments)]
async fn run_role(
    role: AgentRole,
    llm: &dyn LlmProvider,
    model: &str,
    provider: &str,
    task_spec: &TaskSpec,
    coder_json: &str,
    tester_json: &str,
    round: u32,
    knowledge_str: &str,
    project_path: &Path,
    review_feedback: Option<&String>,
    display: &mut AgenticDisplay,
    metrics: &mut Vec<StageMetric>,
) -> Result<(String, Vec<String>, RoleOutput)> {
    let task_spec_json = serde_json::to_string_pretty(task_spec)?;
    let (template, schema) = role_prompt(role);

    let ctx = match role {
        AgentRole::Coder => context! {
            input_artifacts => vec![task_spec_json.clone()],
            revision_context => review_feedback.cloned(),
            revision_round => round,
            project_knowledge => knowledge_str.to_string(),
            current_files => build_current_files(task_spec, project_path),
        },
        AgentRole::Tester => context! {
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
        },
        AgentRole::Reviewer => context! {
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string(), tester_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
        },
        AgentRole::Synthesizer => context! {
            // In the parallel-coder flow (#3) `coder_json` carries every coder
            // diff concatenated; the Synthesizer reconciles them into one change.
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
        },
        AgentRole::SecurityAuditor => context! {
            input_artifacts => vec![task_spec_json.clone(), coder_json.to_string()],
            project_knowledge => knowledge_str.to_string(),
        },
        AgentRole::Planner => {
            // Should never happen — the Planner is run separately. Keep the
            // match exhaustive and surface a clear error if it does.
            return Err(crate::NikiError::Config("Planner must not run as a body stage".into()).into());
        }
    };

    let json = run_stage(
        role,
        llm,
        model,
        provider,
        template,
        ctx,
        schema,
        display,
        metrics,
    )
    .await?;

    let output = parse_role(role, &json)?;
    let summary = match &output {
        RoleOutput::Planner(s) => crate::display::artifact_render::render_task_spec_summary(s),
        RoleOutput::Coder(d) => crate::display::artifact_render::render_code_diff_summary(d),
        RoleOutput::Tester(t) => crate::display::artifact_render::render_test_report_summary(t),
        RoleOutput::Reviewer(v) => crate::display::artifact_render::render_review_verdict_summary(v),
        RoleOutput::Synthesizer(s) => crate::display::artifact_render::render_synthesis_summary(s),
        RoleOutput::SecurityAuditor(v) => crate::display::artifact_render::render_security_verdict_summary(v),
    };
    Ok((json, summary, output))
}

fn parse_role(role: AgentRole, json: &str) -> Result<RoleOutput> {
    Ok(match role {
        AgentRole::Planner => RoleOutput::Planner(serde_json::from_str(json)?),
        AgentRole::Coder => RoleOutput::Coder(serde_json::from_str(json)?),
        AgentRole::Tester => RoleOutput::Tester(serde_json::from_str(json)?),
        AgentRole::Reviewer => RoleOutput::Reviewer(serde_json::from_str(json)?),
        AgentRole::Synthesizer => RoleOutput::Synthesizer(serde_json::from_str(json)?),
        AgentRole::SecurityAuditor => RoleOutput::SecurityAuditor(serde_json::from_str(json)?),
    })
}

pub async fn execute_pipeline(
    task: &Task,
    config: &NikiConfig,
    docker: Option<&bollard::Docker>,
    display: &mut AgenticDisplay,
    containers: ActiveContainers,
    dry_run: bool,
) -> Result<PipelineResult> {
    // 1. Index Project
    let knowledge = index_project(&task.project_path, config).await?;
    let knowledge_str = knowledge.render();

    let state = super::state::PipelineState::new(task.id);
    let mut artifacts: Vec<(AgentRole, String)> = Vec::new();
    let mut metrics: Vec<StageMetric> = Vec::new();

    // Resolve the ordered, data-driven stage list.
    let stages = ensure_planner(resolve_stages(config), config);

    // --- Planner (entry point) ---
    let planner_stage = stages
        .iter()
        .find(|s| s.role == AgentRole::Planner && !s.skip)
        .ok_or_else(|| crate::NikiError::Config("No Planner stage configured".to_string()))?;
    let planner_llm = provider_for(&planner_stage.provider, config)?;

    let planner_json = run_stage(
        AgentRole::Planner,
        planner_llm.as_ref(),
        &planner_stage.model,
        &planner_stage.provider,
        "planner.md",
        context! {
            task_description => task.description.clone(),
            project_knowledge => knowledge_str.clone(),
        },
        "schemas/task_spec.schema.json",
        display,
        &mut metrics,
    )
    .await?;
    let task_spec: TaskSpec = serde_json::from_str(&planner_json)?;
    artifacts.push((AgentRole::Planner, planner_json.clone()));
    let pm = metrics.last().unwrap();
    display.agent_done(
        AgentRole::Planner,
        crate::display::artifact_render::render_task_spec_summary(&task_spec),
        pm.usage(),
        pm.cost_usd,
    );
    display.update_pipeline_status();

    // Dry-run: stop after the Planner and surface the spec without executing.
    if dry_run {
        return Ok(PipelineResult {
            task_id: task.id,
            state,
            final_diff: String::new(),
            verdict: Verdict::Approved,
            revision_rounds: 0,
            artifacts,
            metrics,
        });
    }

    // 2. Initialize Sandbox (backend chosen by config: docker / worktree / cloud)
    // `containers` is an Arc and is cloned here so the parallel-coder path below
    // can hand its own clone to each per-coder worktree sandbox.
    let sandbox = create_sandbox(
        config.docker.backend,
        docker,
        AgentRole::Planner,
        &task.project_path,
        &task.id,
        &config.docker,
        containers.clone(),
    )
    .await?;

    // The sandbox image is expected to be pre-baked with the toolchain the pipeline
    // needs (git/node/npm/python3). We verify presence up front rather than installing
    // at runtime, so a misconfigured image fails fast instead of hanging on apt.
    let required = required_tools(config);
    sandbox.ensure_tools(&required).await?;

    // --- Body stages (everything after the Planner), in configured order ---
    let body_stages: Vec<&PipelineStageConfig> = stages
        .iter()
        .filter(|s| s.role != AgentRole::Planner && !s.skip)
        .collect();

    // Build one provider client per distinct provider referenced by the stages.
    // Stored as `Arc` so the parallel-coder path can move a clone into a
    // spawned task without fighting the borrow checker.
    let mut provider_cache: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    for s in &body_stages {
        if !provider_cache.contains_key(&s.provider) {
            let llm = provider_for(&s.provider, config)?;
            provider_cache.insert(s.provider.clone(), Arc::from(llm));
        }
    }

    let max_rounds = config
        .pipeline
        .max_revision_rounds
        .unwrap_or(config.general.max_revision_rounds);
    let has_reviewer = body_stages.iter().any(|s| s.role == AgentRole::Reviewer);

    let mut coder_json = String::new();
    let mut tester_json = String::new();
    let mut review_feedback: Option<String> = None;
    let mut verdict = Verdict::Approved;
    let mut round = 0;

    if config.parallel.enabled && config.parallel.coder_count > 1 {
        // ── Parallel-coder mode (#3) ────────────────────────────────────────
        // 1) Run N coders concurrently, each isolated in its own git worktree.
        let coder_stage = body_stages
            .iter()
            .find(|s| s.role == AgentRole::Coder)
            .expect("parallel mode requires a Coder stage");
        let per_coder = run_parallel_coders(
            config.parallel.coder_count,
            provider_cache.get(&coder_stage.provider).unwrap().clone(),
            &coder_stage.model,
            &coder_stage.provider,
            &task_spec,
            &knowledge_str,
            &task.project_path,
            config,
            containers.clone(),
            &task.id,
            display,
            &mut metrics,
        )
        .await?;

        // 2) Reconcile the per-coder diffs through the Synthesizer stage
        //    (injected by `resolve_stages` when parallel mode is on).
        let synth_stage = body_stages
            .iter()
            .find(|s| s.role == AgentRole::Synthesizer)
            .expect("parallel mode requires a Synthesizer stage");
        let synth_llm = provider_cache.get(&synth_stage.provider).unwrap();
        let coder_json_in = serde_json::to_string(&per_coder)?;
        let (json, summary, role_output) = run_role(
            AgentRole::Synthesizer,
            &**synth_llm,
            &synth_stage.model,
            &synth_stage.provider,
            &task_spec,
            &coder_json_in,
            "",
            0,
            &knowledge_str,
            &task.project_path,
            None,
            display,
            &mut metrics,
        )
        .await?;
        artifacts.push((AgentRole::Synthesizer, json.clone()));
        let m = metrics.last().unwrap();
        display.agent_done(AgentRole::Synthesizer, summary, m.usage(), m.cost_usd);
        display.update_pipeline_status();

        let merged = match role_output {
            RoleOutput::Synthesizer(s) => s.merged,
            _ => unreachable!("synthesizer stage yields a Synthesis"),
        };
        coder_json = serde_json::to_string_pretty(&merged)?;
        if let Err(e) = sandbox.apply_patch(&merged.unified_diff, &task.project_path).await {
            eprintln!("Warning: Failed to apply synthesis patch: {}", e);
        }

        // 3) Run the remaining stages (Tester / Reviewer / SecurityAuditor)
        //    exactly once. In parallel mode the coders don't re-run on revision
        //    feedback, so there is no inner revision loop.
        for stage in body_stages
            .iter()
            .filter(|s| s.role != AgentRole::Coder && s.role != AgentRole::Synthesizer)
        {
            let llm = provider_cache.get(&stage.provider).unwrap();
            let (json, summary, role_output) = run_role(
                stage.role,
                &**llm,
                &stage.model,
                &stage.provider,
                &task_spec,
                &coder_json,
                &tester_json,
                0,
                &knowledge_str,
                &task.project_path,
                None,
                display,
                &mut metrics,
            )
            .await?;
            artifacts.push((stage.role, json.clone()));
            let m = metrics.last().unwrap();
            display.agent_done(stage.role, summary, m.usage(), m.cost_usd);
            display.update_pipeline_status();

            match role_output {
                RoleOutput::Tester(_) => {
                    tester_json = json;
                }
                RoleOutput::Reviewer(v) => {
                    verdict = v.verdict;
                }
                RoleOutput::SecurityAuditor(_) => {}
                _ => unreachable!("only Tester/Reviewer/SecurityAuditor remain"),
            }
        }
    } else {
    while round < max_rounds {
        for stage in &body_stages {
            let llm = provider_cache.get(&stage.provider).unwrap();
            let (json, summary, role_output) = run_role(
                stage.role,
                &**llm,
                &stage.model,
                &stage.provider,
                &task_spec,
                &coder_json,
                &tester_json,
                round,
                &knowledge_str,
                &task.project_path,
                review_feedback.as_ref(),
                display,
                &mut metrics,
            )
            .await?;
            artifacts.push((stage.role, json.clone()));
            let m = metrics.last().unwrap();
            display.agent_done(stage.role, summary, m.usage(), m.cost_usd);
            display.update_pipeline_status();

            match role_output {
                RoleOutput::Coder(diff) => {
                    coder_json = json;
                    if let Err(e) = sandbox.apply_patch(&diff.unified_diff, &task.project_path).await {
                        eprintln!("Warning: Failed to apply coder patch: {}", e);
                    }
                }
                RoleOutput::Tester(_) => {
                    tester_json = json;
                }
                RoleOutput::Reviewer(v) => {
                    verdict = v.verdict;
                    review_feedback = match v.feedback {
                        Some(f) => Some(serde_json::to_string_pretty(&f)?),
                        None => None,
                    };
                }
                RoleOutput::Synthesizer(s) => {
                    // The reconciled change replaces the per-coder diffs for the
                    // downstream Tester/Reviewer stages.
                    coder_json = serde_json::to_string_pretty(&s.merged)?;
                    if let Err(e) = sandbox.apply_patch(&s.merged.unified_diff, &task.project_path).await {
                        eprintln!("Warning: Failed to apply synthesis patch: {}", e);
                    }
                }
                RoleOutput::SecurityAuditor(v) => {
                    // The security verdict is recorded as an artifact. By default it
                    // does not gate the revision loop (the Reviewer owns the gate); an
                    // explicit Rejected with no reviewer overrides to a revision.
                    if matches!(v.verdict, Verdict::Rejected) && !has_reviewer {
                        verdict = Verdict::RevisionNeeded;
                    }
                }
                RoleOutput::Planner(_) => unreachable!("planner is handled separately"),
            }
        }

        if has_reviewer {
            if matches!(verdict, Verdict::Approved | Verdict::Rejected) {
                break;
            }
        } else {
            // No reviewer to gate the loop on; one pass is enough.
            break;
        }
        round += 1;
    }
    }

    // Read the resulting diff. For the Docker backend the patch was applied to the
    // bind-mounted host project, so we read the host working tree directly. For
    // worktree/cloud the change lives only in the sandbox copy, so we read it from
    // there (the run step applies it back to the host before committing).
    let final_diff = match config.docker.backend {
        SandboxBackend::Docker => crate::output::git::working_tree_diff(&task.project_path),
        _ => sandbox.get_diff().await?,
    };

    sandbox.destroy().await?;

    Ok(PipelineResult {
        task_id: task.id,
        state,
        final_diff,
        verdict,
        revision_rounds: round,
        artifacts,
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;

    #[test]
    fn default_pipeline_is_four_stages() {
        let c = NikiConfig::default();
        let s = resolve_stages(&c);
        assert_eq!(s.len(), 4);
        assert!(s.iter().any(|x| x.role == AgentRole::Planner));
        assert!(s.iter().any(|x| x.role == AgentRole::Coder));
    }

    #[test]
    fn security_injects_auditor() {
        let mut c = NikiConfig::default();
        c.security.enabled = true;
        let s = resolve_stages(&c);
        assert!(s.iter().any(|x| x.role == AgentRole::SecurityAuditor));
    }

    #[test]
    fn parallel_injects_synthesizer() {
        let mut c = NikiConfig::default();
        c.parallel.enabled = true;
        c.parallel.coder_count = 3;
        let s = resolve_stages(&c);
        assert!(s.iter().any(|x| x.role == AgentRole::Synthesizer));
    }

    #[test]
    fn required_tools_includes_base_set() {
        let c = NikiConfig::default();
        let t = required_tools(&c);
        assert!(t.iter().any(|p| p == "git"));
        assert!(t.iter().any(|p| p == "python3"));
    }
}
