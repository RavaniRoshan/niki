pub mod config;
pub mod creator;
pub mod runner;
pub mod state;

pub use config::GoalConfig;
pub use creator::GoalCreator;
pub use runner::GoalRunner;
pub use state::{
    ClaimFile, GoalCriterion, GoalState, GoalStatus, GoalTask, TaskStatus, claim_files, claim_path,
    create_claim, env_dir, goals_dir, remove_claim, remove_claim_by_goal, state_path,
};

#[cfg(test)]
pub static TEST_CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
