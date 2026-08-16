use anyhow::Result;
use clap::{Args, Subcommand};
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(unix)]
use crate::cli::run::connect_container_runtime;
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::goal::runner::GoalRunner;
use crate::goal::state::{
    GoalState, claim_files, create_claim, env_dir, goals_dir, remove_claim_by_goal,
};
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
    /// Cancel and archive the active goal (alias for cancel)
    Clear,
    /// Run the autonomous goal loop
    Run {
        /// Goal ID to run (optional, runs active goal)
        id: Option<String>,
    },
    /// Run criteria check once without iterating
    Check,
    /// Show environment detection info
    Env,
    /// Fork a goal — writes artifacts and marks status as Drifting
    Fork {
        /// Goal ID to fork (optional, forks active goal)
        id: Option<String>,
    },
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
        GoalCommands::Cancel | GoalCommands::Clear => handle_cancel().await,
        GoalCommands::Run { id } => handle_run(id.as_deref()).await,
    GoalCommands::Check => handle_check().await,
    GoalCommands::Env => handle_env(),
    GoalCommands::Fork { id } => handle_fork(id.as_deref()).await,
}
}

async fn handle_new(objective: &str, scope: Option<&str>, max: u32) -> Result<()> {
    println!("Creating goal: \"{}\"", objective);

    let mut state = crate::goal::creator::GoalCreator::create(objective, scope, max)?;
    state.save()?;

    let session_id = format!("goal-{}", state.id);
    create_claim(&session_id, &state.id)?;

    create_git_branch(&state.branch);

    let goal_dir = goals_dir();
    println!("Goal created: {}", state.slug);
    println!("  ID: {}", state.id);
    println!("  Branch: {}", state.branch);
    println!("  Status: active");
    println!("  Max iterations: {}", state.max_iterations);
    println!("  Scope: {}", state.scope);
    println!("  Criteria: {}", state.criteria.len());
    println!("  Tasks: {}", state.tasks.len());
    println!();
    println!("Next steps:");
    println!(
        "  1. Review and refine criteria in {}",
        goal_dir.join(format!("{}.json", state.slug)).display()
    );
    println!("  2. Run `niki goal status` to see progress");
    println!("  3. Run `niki goal run` to start the autonomous loop");
    println!();
    println!("Criteria:");
    for (i, c) in state.criteria.iter().enumerate() {
        let gate = if c.must_pass {
            "[must-pass]"
        } else {
            "[optional]"
        };
        println!("  {}. {} {} — {}", i + 1, gate, c.label, c.check);
    }
    println!();
    println!("Tasks:");
    for t in &state.tasks {
        println!("  [{}] {}", t.id, t.desc);
    }

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
        None => GoalState::active_goal()?,
    };

    match state {
        Some(s) => print_goal_status(&s),
        None => {
            if id.is_some() {
                println!("Goal not found.");
            } else {
                println!("No active goal. Create one with `niki goal new <objective>`.");
            }
        }
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
        let gate = if c.must_pass { "[must-pass]" } else { "[opt]" };
        println!("  {} {} [{}] {}", icon, gate, c.label, c.check);
    }
}

async fn handle_pause() -> Result<()> {
    let state = GoalState::active_goal()?;
    match state {
        Some(mut s) => {
            s.status = crate::goal::state::GoalStatus::Paused;
            s.save()?;
            remove_claim_by_goal(&s.id)?;
            println!("Goal paused. Resume with `niki goal resume {}`.", s.id);
        }
        None => {
            println!("No active goal to pause.");
        }
    }
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
    let state = GoalState::active_goal()?;
    match state {
        Some(mut s) => {
            s.status = crate::goal::state::GoalStatus::Cancelled;
            s.save()?;
            remove_claim_by_goal(&s.id)?;
            let dir = goals_dir();
            println!(
                "Goal cancelled. State preserved at {}",
                dir.join(format!("{}.json", s.slug)).display()
            );
        }
        None => {
            println!("No active goal to cancel.");
        }
    }
    Ok(())
}

async fn handle_run(id: Option<&str>) -> Result<()> {
    let state = match id {
        Some(goal_id) => GoalState::find_by_id(goal_id)?,
        None => GoalState::active_goal()?,
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

    let config = NikiConfig::load(std::path::Path::new(&state.scope))?;
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
    let state = GoalState::active_goal()?;
    let state = match state {
        Some(s) => s,
        None => {
            println!("No active goal to check.");
            return Ok(());
        }
    };

    println!("Checking criteria for goal: {}", state.slug);
    println!();

    let mut all_pass = true;
    for criterion in &state.criteria {
        let output = std::process::Command::new("bash")
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
                        println!("  ✓ PASS [must-pass]: {}", criterion.label);
                    } else {
                        println!("  ✗ FAIL [must-pass]: {} — {}", criterion.label, stderr);
                        all_pass = false;
                    }
                } else {
                    if exit_ok {
                        println!("  ✓ PASS [optional]: {}", criterion.label);
                    } else {
                        println!("  ✗ FAIL [optional]: {} — {}", criterion.label, stderr);
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

fn create_git_branch(branch_name: &str) {
    let result = std::process::Command::new("git")
        .args(["checkout", "-b", branch_name])
        .output();
    match result {
        Ok(out) if out.status.success() => {
            println!("  Branch: created {}", branch_name);
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("already exists") {
                let _ = std::process::Command::new("git")
                    .args(["checkout", branch_name])
                    .output();
            }
        }
        Err(_) => {}
    }
}

fn handle_env() -> Result<()> {
    use crate::goal::state::env_dir;
    let dir = env_dir();
    let cwd = std::env::current_dir().unwrap_or_default();
    let full_path = cwd.join(dir);

    let has_opencode = cwd.join(".opencode").exists()
        || std::env::var("OPENCODE_SESSION_ID").is_ok()
        || cwd.join("opencode.json").exists()
        || cwd.join("opencode.jsonc").exists();
    let has_kilo = cwd.join(".kilo").exists() || std::env::var("KILO_SESSION_ID").is_ok();
    let has_claude = cwd.join(".claude").exists();

    println!("Environment Detection:");
    println!("  CWD: {}", cwd.display());
    println!("  OpenCode: {}", if has_opencode { "yes" } else { "no" });
    println!("  KiloCode: {}", if has_kilo { "yes" } else { "no" });
    println!("  Claude Code: {}", if has_claude { "yes" } else { "no" });
    println!();
    println!("  Goals directory: {}", full_path.display());
    println!("  Env dir: {}", dir);

    if !full_path.exists() {
        println!();
        println!(
            "  Goals directory does not exist yet. It will be created on the first goal save."
        );
    } else {
        let goal_files: Vec<_> = std::fs::read_dir(&full_path)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        println!("  Existing goal files: {}", goal_files.len());
        for g in &goal_files {
            println!("    {}", g);
        }
    }

    Ok(())
}

async fn handle_fork(id: Option<&str>) -> Result<()> {
    let mut state = match id {
        Some(goal_id) => GoalState::find_by_id(goal_id)?,
        None => GoalState::active_goal()?,
    };
    let state = match state {
        Some(s) => s,
        None => {
            println!("No goal found to fork.");
            return Ok(());
        }
    };

    let fork_dir = state.fork()?;
    println!("Fork artifacts written to: {}", fork_dir.display());
    println!("  goal.md");
    println!("  progress.json");
    println!("  drift.jsonl");
    println!("  environment.lock");
    println!("  open-questions.md");
    Ok(())
}
