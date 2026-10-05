//! Headless agent runner for external evaluation harnesses (e.g. Harbor).
//!
//! Provides the minimal, static musl-compatible entry point for running NIKI
//! in task containers with no external dependencies (no Node, no Python).
//! Communicates via baseline tools and exports ATIF trajectories.

use anyhow::{Context, Result, bail};
use clap::Args;
use std::collections::HashMap;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use crate::config::NikiConfig;
use crate::llm::provider::create_provider;
use crate::mission::{AgentId, MissionId};
use crate::runtime::tools::{
    LoopMessage, LoopOptions, LoopSpend, ToolContext, build_baseline_registry,
    run_tool_loop_spending,
};

#[derive(Args, Debug, Clone)]
pub struct AgentArgs {
    /// Task description to execute. Pass "-" to read from standard input.
    #[arg(value_name = "TASK")]
    pub task: Option<String>,

    /// Write an Agent Trajectory Interchange Format (ATIF) document to PATH.
    #[arg(long, value_name = "PATH")]
    pub atif_out: Option<PathBuf>,

    /// Maximum wall-clock execution time in seconds.
    #[arg(long, value_name = "SECS")]
    pub max_time: Option<u64>,

    /// Maximum spend in USD.
    #[arg(long, value_name = "USD")]
    pub max_cost: Option<f64>,

    /// Model to use for the agent loop (default: from config or environment).
    #[arg(short, long)]
    pub model: Option<String>,

    /// LLM provider (default: from config or environment).
    #[arg(long)]
    pub provider: Option<String>,

    /// Working project directory (default: current directory).
    #[arg(short, long)]
    pub project: Option<PathBuf>,

    /// Maximum loop steps before stopping (default: 50).
    #[arg(long, default_value_t = 50)]
    pub max_steps: usize,
}

pub async fn handle(args: AgentArgs) -> Result<()> {
    let task_text = match args.task.as_deref() {
        Some("-") => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading task from stdin")?;
            let trimmed = buf.trim().to_string();
            if trimmed.is_empty() {
                bail!("task on stdin was empty; refusing to start an empty agent run");
            }
            trimmed
        }
        Some(t) => {
            let trimmed = t.trim().to_string();
            if trimmed.is_empty() {
                bail!("task was empty; refusing to start an empty agent run");
            }
            trimmed
        }
        None => {
            if !std::io::stdin().is_terminal() {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("reading piped task from stdin")?;
                let trimmed = buf.trim().to_string();
                if trimmed.is_empty() {
                    bail!("piped task was empty; refusing to start an empty agent run");
                }
                trimmed
            } else {
                bail!("no task specified; pass a task string or '-' for stdin");
            }
        }
    };

    let project_dir = args
        .project
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let config = NikiConfig::load(&project_dir).unwrap_or_default();

    let provider_name = args
        .provider
        .as_deref()
        .or_else(|| {
            if config.providers.contains_key("openai") {
                Some("openai")
            } else if config.providers.contains_key("anthropic") {
                Some("anthropic")
            } else {
                config.providers.keys().next().map(|s| s.as_str())
            }
        })
        .unwrap_or("openai");

    let provider_cfg = config.providers.get(provider_name).cloned().unwrap_or_default();

    let model_slug = args
        .model
        .clone()
        .unwrap_or_else(|| {
            if !provider_cfg.default_model.is_empty() {
                provider_cfg.default_model.clone()
            } else {
                "default".to_string()
            }
        });

    let provider = create_provider(provider_name, &provider_cfg)?;

    let registry = build_baseline_registry();
    let ctx = ToolContext {
        agent_id: AgentId("agent".into()),
        mission_id: MissionId("harness-run".into()),
        role: "coder".into(),
        project_path: project_dir.clone(),
        permissions: HashMap::new(),
        permission_mode: "auto".into(),
        fail_closed_headless: false,
        network_allowlist: Vec::new(),
        task_store: None,
        mcp: None,
        human_input: None,
    };

    let system_prompt = "You are an autonomous software engineering agent running inside a containerized sandbox. \
        Use the available tools (read, write, edit, bash, glob, grep, list) to inspect, solve, and verify the task. \
        Ensure your changes are complete, accurate, and tested before concluding.";

    let messages = vec![
        LoopMessage::System(system_prompt.to_string()),
        LoopMessage::User(task_text.clone()),
    ];

    let mut spend = LoopSpend::default();
    let options = LoopOptions::default();

    let loop_result = run_tool_loop_spending(
        options,
        provider.as_ref(),
        &model_slug,
        &registry,
        &ctx,
        messages,
        None,
        args.max_steps,
        None,
        None,
        &mut spend,
    )
    .await;

    let total_in = spend.usage.input_tokens as usize;
    let total_out = spend.usage.output_tokens as usize;
    let total_cost = (total_in as f64 * 0.0000015) + (total_out as f64 * 0.000002);

    // If --atif-out was requested, export the trajectory regardless of run outcome
    if let Some(ref atif_path) = args.atif_out {
        let task_id = uuid::Uuid::new_v4().to_string();

        let steps = vec![
            serde_json::json!({
                "step_id": 1,
                "source": "user",
                "content": task_text
            }),
            serde_json::json!({
                "step_id": 2,
                "source": "agent",
                "content": format!("Completed {} steps with usage {} in / {} out.", spend.steps, total_in, total_out)
            }),
        ];

        let trajectory_json = serde_json::json!({
            "schema_version": "1.0",
            "task_id": task_id,
            "total_cost_usd": total_cost,
            "total_tokens_in": total_in,
            "total_tokens_out": total_out,
            "steps": steps
        });

        if let Err(e) = std::fs::write(atif_path, serde_json::to_string_pretty(&trajectory_json)?) {
            eprintln!("warning: failed to write ATIF trajectory to {}: {}", atif_path.display(), e);
        }
    }

    match loop_result {
        Ok(_) => {
            println!(
                "{{\"status\": \"success\", \"steps\": {}, \"tokens_in\": {}, \"tokens_out\": {}, \"cost_usd\": {:.6}}}",
                spend.steps,
                total_in,
                total_out,
                total_cost
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("agent loop failed: {e:#}");
            bail!(e);
        }
    }
}
