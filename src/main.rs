use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// On Unix, ignore SIGPIPE so piping output to `head`/`less`/`grep` does not
/// terminate the process. A closed pipe then surfaces as a normal write error
/// instead of a crash (previously `niki recommend | head` exited 101).
#[cfg(unix)]
fn ignore_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a coding task through the NIKI pipeline
    Run(niki::cli::run::RunArgs),
    /// View the status of the current or most recent task
    Status(niki::cli::status::StatusArgs),
    /// View the report for a completed task
    Report(niki::cli::report::ReportArgs),
    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: niki::cli::config::ConfigCommands,
    },
    /// Recommend per-agent models (cost/quality tradeoffs)
    Recommend(niki::cli::recommend::RecommendArgs),
    /// Generate/locate the static HTML dashboard for a task
    Dashboard(niki::cli::dashboard::DashboardArgs),
    /// Run the NIKI-vs-baseline evaluation harness on a seeded-defect dataset
    Eval(niki::cli::eval::EvalArgs),
    /// View and manage agent memory (learned patterns from past runs)
    Memory(niki::cli::memory::MemoryArgs),
    /// Manage persistent goals (autonomous goal runner)
    Goal(niki::cli::goal::GoalArgs),
    /// Manage API credentials (login, logout, status)
    Auth {
        #[command(subcommand)]
        command: niki::cli::auth::AuthCommands,
    },
    /// Manage and check LLM providers
    Providers(niki::cli::providers::ProvidersArgs),
    /// Run diagnostics to verify installation and configuration
    Doctor(niki::cli::doctor::DoctorArgs),
    /// Interactive chat session (TUI)
    Chat(niki::cli::chat::ChatArgs),
    /// Run a smoke test: quick pipeline check to verify your setup works end-to-end
    Smoke(niki::cli::smoke::SmokeArgs),
    /// Search the web and return a cited summary
    Research(niki::cli::research::ResearchArgs),
    /// Capture a screenshot for visual verification
    Verify(niki::cli::verify::VerifyArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(unix)]
    ignore_sigpipe();

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();
    let command = cli.command.unwrap_or_else(|| {
        Commands::Chat(niki::cli::chat::ChatArgs::default())
    });

    match &command {
        Commands::Run(args) => niki::cli::run::handle(args).await?,
        Commands::Status(args) => niki::cli::status::handle(args).await?,
        Commands::Report(args) => niki::cli::report::handle(args).await?,
        Commands::Config { command } => niki::cli::config::handle(command).await?,
        Commands::Recommend(args) => niki::cli::recommend::handle(args)?,
        Commands::Dashboard(args) => niki::cli::dashboard::handle(args)?,
        Commands::Eval(args) => niki::cli::eval::handle(args).await?,
        Commands::Memory(args) => niki::cli::memory::handle(args)?,
        Commands::Goal(args) => niki::cli::goal::handle(args).await?,
        Commands::Auth { command } => niki::cli::auth::handle(command).await?,
        Commands::Providers(args) => niki::cli::providers::handle(args)?,
        Commands::Doctor(args) => niki::cli::doctor::handle(args)?,
        Commands::Chat(args) => niki::cli::chat::handle(args).await?,
        Commands::Smoke(args) => niki::cli::smoke::handle(args).await?,
        Commands::Research(args) => niki::cli::research::handle(args).await?,
        Commands::Verify(args) => niki::cli::verify::handle(args)?,
    }

    Ok(())
}
