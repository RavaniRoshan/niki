use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::sandbox::docker::{DockerSandbox, ActiveContainers};
use crate::knowledge::indexer::index_project;
use crate::artifacts::types::{AgentRole, Verdict, ReviewVerdict};
use crate::llm::provider::create_provider;

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
}

pub async fn execute_pipeline(
    task: &Task,
    config: &NikiConfig,
    docker: &bollard::Docker,
    display: &mut AgenticDisplay,
    containers: ActiveContainers,
    dry_run: bool,
) -> Result<PipelineResult> {
    // 1. Index Project
    let knowledge = index_project(&task.project_path, config)?;
    let knowledge_str = knowledge.render();

    let mut state = super::state::PipelineState::new(task.id);
    let mut artifacts: Vec<(AgentRole, String)> = Vec::new();

    use crate::agents::run_agent;
    use minijinja::context;

    // Planner
    let planner_cfg = config.providers.get(&config.agents.planner.provider)
        .ok_or_else(|| crate::NikiError::Config("Planner provider not configured".to_string()))?;
    let planner_llm = create_provider(&config.agents.planner.provider, planner_cfg)?;
    
    let (planner_json, planner_tk) = run_agent(
        AgentRole::Planner,
        planner_llm.as_ref(),
        &config.agents.planner.model,
        "planner.md",
        context! {
            task_description => task.description.clone(),
            project_knowledge => knowledge_str.clone(),
        },
        "schemas/task_spec.schema.json",
        display,
    ).await?;
    let task_spec: crate::artifacts::types::TaskSpec = serde_json::from_str(&planner_json)?;
    artifacts.push((AgentRole::Planner, planner_json));
    display.agent_done(AgentRole::Planner, crate::display::artifact_render::render_task_spec_summary(&task_spec), planner_tk);
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
        });
    }

    // 2. Initialize Sandbox
    let sandbox = DockerSandbox::create(
        docker,
        AgentRole::Planner,
        &task.project_path,
        &task.id,
        &config.docker,
        containers,
    ).await?;

    // The sandbox image is expected to be pre-baked with the toolchain the pipeline
    // needs (git/node/npm/python3). We verify presence up front rather than installing
    // at runtime, so a misconfigured image fails fast instead of hanging on apt.
    let mut required = config.docker.extra_packages.clone();
    for tool in ["git", "node", "npm", "python3"] {
        if !required.iter().any(|p| p == tool) {
            required.push(tool.to_string());
        }
    }
    sandbox.ensure_tools(&required).await?;

    // Coder
    let coder_cfg = config.providers.get(&config.agents.coder.provider)
        .ok_or_else(|| crate::NikiError::Config("Coder provider not configured".to_string()))?;
    let coder_llm = create_provider(&config.agents.coder.provider, coder_cfg)?;

    // Tester
    let tester_cfg = config.providers.get(&config.agents.tester.provider)
        .ok_or_else(|| crate::NikiError::Config("Tester provider not configured".to_string()))?;
    let tester_llm = create_provider(&config.agents.tester.provider, tester_cfg)?;

    // Reviewer
    let reviewer_cfg = config.providers.get(&config.agents.reviewer.provider)
        .ok_or_else(|| crate::NikiError::Config("Reviewer provider not configured".to_string()))?;
    let reviewer_llm = create_provider(&config.agents.reviewer.provider, reviewer_cfg)?;

    let mut verdict = Verdict::Approved;
    let mut round = 0;
    
    let mut review_feedback: Option<String> = None;

    while round < config.general.max_revision_rounds {
        let (coder_json, coder_tk) = run_agent(
            AgentRole::Coder,
            coder_llm.as_ref(),
            &config.agents.coder.model,
            "coder.md",
            context! {
                input_artifacts => vec![serde_json::to_string_pretty(&task_spec)?],
                revision_context => review_feedback.clone(),
                revision_round => round,
                project_knowledge => knowledge_str.clone(),
            },
            "schemas/code_diff.schema.json",
            display,
        ).await?;
        let code_diff: crate::artifacts::types::CodeDiff = serde_json::from_str(&coder_json)?;
        artifacts.push((AgentRole::Coder, coder_json.clone()));
        display.agent_done(AgentRole::Coder, crate::display::artifact_render::render_code_diff_summary(&code_diff), coder_tk);
        display.update_pipeline_status();

        if let Err(e) = sandbox.apply_patch(&code_diff.unified_diff, &task.project_path).await {
            eprintln!("Warning: Failed to apply coder patch: {}", e);
        }

        // Tester
        let (tester_json, tester_tk) = run_agent(
            AgentRole::Tester,
            tester_llm.as_ref(),
            &config.agents.tester.model,
            "tester.md",
            context! {
                input_artifacts => vec![
                    serde_json::to_string_pretty(&task_spec)?,
                    coder_json.clone()
                ],
                project_knowledge => knowledge_str.clone(),
            },
            "schemas/test_report.schema.json",
            display,
        ).await?;
        let test_report: crate::artifacts::types::TestReport = serde_json::from_str(&tester_json)?;
        artifacts.push((AgentRole::Tester, tester_json.clone()));
        display.agent_done(AgentRole::Tester, crate::display::artifact_render::render_test_report_summary(&test_report), tester_tk);
        display.update_pipeline_status();

        // Reviewer
        let (reviewer_json, reviewer_tk) = run_agent(
            AgentRole::Reviewer,
            reviewer_llm.as_ref(),
            &config.agents.reviewer.model,
            "reviewer.md",
            context! {
                input_artifacts => vec![
                    serde_json::to_string_pretty(&task_spec)?,
                    coder_json.clone(),
                    tester_json.clone()
                ],
                project_knowledge => knowledge_str.clone(),
            },
            "schemas/review_verdict.schema.json",
            display,
        ).await?;
        let review_verdict: crate::artifacts::types::ReviewVerdict = serde_json::from_str(&reviewer_json)?;
        artifacts.push((AgentRole::Reviewer, reviewer_json));
        display.agent_done(AgentRole::Reviewer, crate::display::artifact_render::render_review_verdict_summary(&review_verdict), reviewer_tk);
        display.update_pipeline_status();

        verdict = review_verdict.verdict;
        if matches!(verdict, Verdict::Approved | Verdict::Rejected) {
            break;
        }
        
        if let Some(feedback) = review_verdict.feedback {
            review_feedback = Some(serde_json::to_string_pretty(&feedback)?);
        }
        
        round += 1;
    }

    let final_diff = sandbox.get_diff().await?;

    sandbox.destroy().await?;

    Ok(PipelineResult {
        task_id: task.id,
        state,
        final_diff,
        verdict,
        revision_rounds: round,
        artifacts,
    })
}
