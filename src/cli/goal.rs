use anyhow::Result;
use clap::{Args, Subcommand};
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(unix)]
use crate::cli::run::connect_container_runtime;
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::goal::runner::GoalRunner;
use crate::goal::state::{GoalState, claim_files, create_claim, remove_claim_by_goal};
use crate::sandbox::docker::ActiveContainers;

#[derive(Args)]
pub struct GoalArgs {
    #[command(subcommand)]
    command: GoalCommands,
}

#[derive(Subcommand)]
enum GoalCommands {
    /// Create a new goal from an objective string
    New {
        /// The objective to achieve
        objective: String,
        /// Scope directory for this goal
        #[arg(long)]
        scope: Option<String>,
        /// Maximum iterations
        #[arg(long, default_value_t = 30)]
        max: u32,
    },
    /// List all goals
    List,
    /// Show status of the current or specified goal
    Status {
        /// Goal ID to show (optional, shows active goal)
        id: Option<String>,
    },
    /// Pause the active goal
    Pause,
    /// Resume a paused goal by ID
    Resume {
        /// Goal ID to resume
        id: String,
    },
    /// Cancel the active goal
    Cancel,
    /// Run the autonomous goal loop
    Run {
        /// Goal ID to run (optional, runs active goal)
        id: Option<String>,
    },
    /// Run criteria check once without iterating
    Check,
}

pub async fn handle(args: &GoalArgs) -> Result<()> {
    match &args.command {
        GoalCommands::New {
            objective,
            scope,
            max,
        } => handle_new(objective, scope.as_deref(), *max).await,
        GoalCommands::List => handle_list(),
        GoalCommands::Status { id } => handle_status(id.as_deref()).await,
        GoalCommands::Pause => handle_pause().await,
        GoalCommands::Resume { id } => handle_resume(id).await,
        GoalCommands::Cancel => handle_cancel().await,
        GoalCommands::Run { id } => handle_run(id.as_deref()).await,
        GoalCommands::Check => handle_check().await,
    }
}

async fn handle_new(objective: &str, scope: Option<&str>, max: u32) -> Result<()> {
    println!("Creating goal: \"{}\"", objective);

    let slug = slugify(objective);
    let id = generate_id();
    let branch_name = format!("goal/{}-{}", slug, id);

    let scope_lock = match scope {
        Some(s) => vec![s.to_string()],
        None => vec![".".to_string()],
    };

    let state = GoalState {
        id: id.clone(),
        slug: format!("{}-{}", slug, id),
        objective: objective.to_string(),
        status: crate::goal::state::GoalStatus::Active,
        branch: branch_name,
        scope: scope.unwrap_or(".").to_string(),
        scope_lock,
        scope_flex: vec![],
        criteria: vec![],
        tasks: vec![],
        current_task: 0,
        iterations: 0,
        budget_used: 0,
        max_iterations: max,
        negative_knowledge: vec![],
        context_summary: format!("Goal: {}\nIteration 0: created.\n", objective),
        created_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
    };

    state.save()?;

    let session_id = format!("goal-{}", id);
    create_claim(&session_id, &id)?;

    println!("Goal created: {}", state.slug);
    println!("  ID: {}", state.id);
    println!("  Branch: {}", state.branch);
    println!("  Status: active");
    println!("  Max iterations: {}", state.max_iterations);
    println!("  Scope: {}", state.scope);
    println!();
    println!("Next steps:");
    println!(
        "  1. Review and refine criteria in .opencode/goals/{}.json",
        state.slug
    );
    println!("  2. Run `niki goal status` to see progress");
    println!(
        "  3. Run `niki goal resume {}` to start the autonomous loop",
        state.id
    );

    Ok(())
}

fn handle_list() -> Result<()> {
    let states = GoalState::load_all()?;
    if states.is_empty() {
        println!("No goals found.");
        return Ok(());
    }

    println!("  ID      STATUS   SLUG                                    CREATED");
    println!("  ------- -------- --------------------------------------- -------------------");
    for state in &states {
        let created = state.created_at.chars().take(10).collect::<String>();
        println!(
            "  {:<8} {:<8} {:<40} {}",
            state.id, state.status, state.slug, created
        );
    }
    Ok(())
}

async fn handle_status(id: Option<&str>) -> Result<()> {
    let state = match id {
        Some(goal_id) => GoalState::find_by_id(goal_id)?,
        None => {
            let claims = claim_files()?;
            if claims.is_empty() {
                println!("No active goal. Create one with `niki goal new <objective>`.");
                return Ok(());
            }
            let latest = claims
                .into_iter()
                .max_by_key(|c| c.claimed_at.clone())
                .ok_or_else(|| anyhow::anyhow!("No active goal"))?;
            GoalState::find_by_id(&latest.goal_id)?
        }
    };

    match state {
        Some(s) => print_goal_status(&s),
        None => println!("Goal not found."),
    }
    Ok(())
}

fn print_goal_status(state: &GoalState) {
    println!("Goal: {}", state.objective);
    println!("  ID: {}", state.id);
    println!("  Slug: {}", state.slug);
    println!("  Status: {}", state.status);
    println!("  Branch: {}", state.branch);
    println!("  Iterations: {}", state.iterations);
    println!("  Max iterations: {}", state.max_iterations);
    println!("  Scope: {}", state.scope);
    println!("  Scope lock: {}", state.scope_lock.join(", "));
    println!();

    let total_tasks = state.tasks.len();
    let done_tasks = state
        .tasks
        .iter()
        .filter(|t| t.status == crate::goal::state::TaskStatus::Done)
        .count();
    println!("Tasks: {}/{} done", done_tasks, total_tasks);
    for task in &state.tasks {
        let icon = match task.status {
            crate::goal::state::TaskStatus::Done => "✓",
            crate::goal::state::TaskStatus::InProgress => "→",
            crate::goal::state::TaskStatus::Blocked => "✗",
            _ => " ",
        };
        println!("  {} [{}] {}", icon, task.id, task.desc);
    }
    println!();

    let total_criteria = state.criteria.len();
    let passed = state
        .criteria
        .iter()
        .filter(|c| c.result.as_deref() == Some("PASS"))
        .count();
    println!("Criteria: {}/{} passed", passed, total_criteria);
    for c in &state.criteria {
        let icon = if c.result.as_deref() == Some("PASS") {
            "✓"
        } else {
            " "
        };
        println!("  {} [{}] {}", icon, c.label, c.check);
    }
}

async fn handle_pause() -> Result<()> {
    let claims = claim_files()?;
    if claims.is_empty() {
        println!("No active goal to pause.");
        return Ok(());
    }
    let latest = claims
        .into_iter()
        .max_by_key(|c| c.claimed_at.clone())
        .ok_or_else(|| anyhow::anyhow!("No active goal"))?;
    let mut state = GoalState::find_by_id(&latest.goal_id)?
        .ok_or_else(|| anyhow::anyhow!("Goal state not found"))?;
    state.status = crate::goal::state::GoalStatus::Paused;
    state.save()?;
    remove_claim_by_goal(&state.id)?;
    println!("Goal paused. Resume with `niki goal resume {}`.", state.id);
    Ok(())
}

async fn handle_resume(id: &str) -> Result<()> {
    let mut state =
        GoalState::find_by_id(id)?.ok_or_else(|| anyhow::anyhow!("Goal not found: {}", id))?;
    if state.status != crate::goal::state::GoalStatus::Paused {
        println!("Goal {} is not paused (status: {}).", id, state.status);
        return Ok(());
    }
    state.status = crate::goal::state::GoalStatus::Active;
    state.save()?;
    let session_id = format!("goal-{}", id);
    create_claim(&session_id, id)?;
    println!(
        "Goal resumed. {} iterations remaining.",
        state.max_iterations - state.iterations
    );
    Ok(())
}

async fn handle_cancel() -> Result<()> {
    let claims = claim_files()?;
    if claims.is_empty() {
        println!("No active goal to cancel.");
        return Ok(());
    }
    let latest = claims
        .into_iter()
        .max_by_key(|c| c.claimed_at.clone())
        .ok_or_else(|| anyhow::anyhow!("No active goal"))?;
    let mut state = GoalState::find_by_id(&latest.goal_id)?
        .ok_or_else(|| anyhow::anyhow!("Goal state not found"))?;
    state.status = crate::goal::state::GoalStatus::Cancelled;
    state.save()?;
    remove_claim_by_goal(&state.id)?;
    println!(
        "Goal cancelled. State preserved at .opencode/goals/{}.json",
        state.slug
    );
    Ok(())
}

async fn handle_run(id: Option<&str>) -> Result<()> {
    let state = match id {
        Some(goal_id) => GoalState::find_by_id(goal_id)?,
        None => {
            let claims = claim_files()?;
            if claims.is_empty() {
                println!("No active goal. Create one with `niki goal new <objective>`.");
                return Ok(());
            }
            let latest = claims
                .into_iter()
                .max_by_key(|c| c.claimed_at.clone())
                .ok_or_else(|| anyhow::anyhow!("No active goal"))?;
            GoalState::find_by_id(&latest.goal_id)?
        }
    };

    let mut state = state.ok_or_else(|| anyhow::anyhow!("Goal not found"))?;

    if state.status != crate::goal::state::GoalStatus::Active {
        println!(
            "Goal {} is not active (status: {}).",
            state.id, state.status
        );
        return Ok(());
    }

    println!("Starting autonomous goal loop: {}", state.objective);
    println!("  ID: {}", state.id);
    println!("  Max iterations: {}", state.max_iterations);
    println!();

    let config = NikiConfig::load(std::path::Path::new(&state.scope)).unwrap_or_default();
    #[cfg(unix)]
    let docker = match connect_container_runtime().await {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!(
                "Warning: no container runtime available: {}. Using worktree backend.",
                e
            );
            None
        }
    };
    #[cfg(not(unix))]
    let docker: Option<bollard::Docker> = None;
    let docker_ref = docker.as_ref();
    let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));
    let mut display = AgenticDisplay::new();

    GoalRunner::run(&mut state, &config, docker_ref, &mut display, containers).await?;

    println!();
    println!("Goal loop finished.");
    println!("  Status: {}", state.status);
    println!("  Iterations: {}", state.iterations);
    println!(
        "  Tasks completed: {}/{}",
        state
            .tasks
            .iter()
            .filter(|t| t.status == crate::goal::state::TaskStatus::Done)
            .count(),
        state.tasks.len()
    );

    Ok(())
}

async fn handle_check() -> Result<()> {
    let claims = claim_files()?;
    if claims.is_empty() {
        println!("No active goal to check.");
        return Ok(());
    }
    let latest = claims
        .into_iter()
        .max_by_key(|c| c.claimed_at.clone())
        .ok_or_else(|| anyhow::anyhow!("No active goal"))?;
    let state = GoalState::find_by_id(&latest.goal_id)?
        .ok_or_else(|| anyhow::anyhow!("Goal state not found"))?;

    println!("Checking criteria for goal: {}", state.slug);
    println!();

    let mut all_pass = true;
    for criterion in &state.criteria {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&criterion.check)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let exit_ok = out.status.success();

                if criterion.must_pass {
                    if exit_ok && !stdout.contains("FAIL") {
                        println!("  ✓ PASS: {}", criterion.label);
                    } else {
                        println!("  ✗ FAIL: {} — {}", criterion.label, stderr);
                        all_pass = false;
                    }
                } else {
                    if exit_ok {
                        println!("  ✓ PASS: {}", criterion.label);
                    } else {
                        println!("  ✗ FAIL (optional): {} — {}", criterion.label, stderr);
                    }
                }
            }
            Err(e) => {
                println!("  ✗ ERROR: {} — {}", criterion.label, e);
                all_pass = false;
            }
        }
    }

    println!();
    if all_pass {
        println!("All must-pass criteria passed.");
    } else {
        println!("Some criteria failed. Goal is not complete yet.");
    }

    Ok(())
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-")
}

fn generate_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    chrono::Utc::now().timestamp().hash(&mut hasher);
    format!("{:x}", hasher.finish())[..6].to_string()
}
