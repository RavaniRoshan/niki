use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let cli = Cli::parse();

    match &cli.command {
        Commands::Run(args) => niki::cli::run::handle(args).await?,
        Commands::Status(args) => niki::cli::status::handle(args).await?,
        Commands::Report(args) => niki::cli::report::handle(args).await?,
        Commands::Config { command } => niki::cli::config::handle(command).await?,
        Commands::Recommend(args) => niki::cli::recommend::handle(args)?,
        Commands::Dashboard(args) => niki::cli::dashboard::handle(args)?,
    }

    Ok(())
}
