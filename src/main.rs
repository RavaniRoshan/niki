use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Restore the default SIGPIPE disposition, so `niki … | head` dies the way
/// every other Unix tool does instead of panicking.
///
/// Rust ignores SIGPIPE at startup and turns a failed write into a panic
/// (`println!` unwraps), so out of the box `niki providers models --plain |
/// head -1` printed
///
/// ```text
/// thread 'main' panicked at library/std/src/io/stdio.rs:1165:9:
/// failed printing to stdout: Broken pipe (os error 32)
/// ```
///
/// and exited 101. Setting `SIG_IGN` — which is what this used to do — does not
/// fix that: ignoring the signal just converts the kill into an `EPIPE` write
/// error, and `println!` panics on that too. The panic is the same either way.
///
/// `SIG_DFL` is the fix. The process is killed by the signal, so the exit
/// status is 141 (128 + SIGPIPE) with nothing on stderr — byte-for-byte the
/// behaviour of `cat`, `git` and `grep`. Nothing is lost: NIKI's stdout is
/// line-buffered, so every line printed before the reader left is already
/// flushed, and the reader got all of them.
#[cfg(unix)]
fn restore_default_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
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
    /// Initialize a new niki.toml configuration file (alias for `config init`)
    Init {
        /// Run interactively (prompt for settings, check env vars)
        #[arg(short, long)]
        interactive: bool,
        /// Scan the project and draft AGENTS.md
        #[arg(long)]
        scan: bool,
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
    /// Research and propose a change without making it (plan mode)
    Plan(niki::cli::plan::PlanArgs),
    /// Inspect and rewind chat/pipeline sessions
    Session(niki::cli::session::SessionArgs),
    /// Resume an interrupted agent session from a checkpoint
    Resume(niki::cli::resume::ResumeArgs),
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
    /// Inspect custom slash commands
    #[allow(clippy::enum_variant_names)]
    Commands(niki::cli::commands::CommandsArgs),
    /// Run NIKI as an Agent Client Protocol (ACP) server over stdio
    Acp(niki::cli::acp::AcpArgs),
    /// Run the engine as a niki-protocol JSON-RPC server over stdio
    Serve(niki::cli::serve::ServeArgs),
    /// Launch the NIKI terminal interface
    Ui(niki::cli::ui::UiArgs),
    /// Emit a consolidated compliance bundle for one task as JSON
    Audit(niki::cli::audit::AuditArgs),
    /// Run a smoke test: quick pipeline check to verify your setup works end-to-end
    Smoke(niki::cli::smoke::SmokeArgs),
    /// Search the web and return a cited summary
    Research(niki::cli::research::ResearchArgs),
    /// Record and transcribe a voice message
    Voice(niki::cli::voice::VoiceArgs),
    /// Inspect repository structure (languages, entry points, risk signals)
    Inspect(niki::cli::inspect::InspectArgs),
    /// Build the project knowledge base (architecture, entities, history)
    Architecture {
        #[command(subcommand)]
        command: niki::cli::architecture::ArchitectureCommands,
    },
    /// Build and query the structural symbol index (advisory code graph)
    Index {
        #[command(subcommand)]
        command: niki::cli::index::IndexCommands,
    },
    /// Distill, promote, and retire versioned project skills
    Skills {
        #[command(subcommand)]
        command: niki::cli::skills::SkillsCommands,
    },
    /// Capture a screenshot for visual verification
    Verify(niki::cli::verify::VerifyArgs),
    /// Run benchmark evaluation tasks through Harbor with strict budget enforcement
    Bench(niki::cli::bench::BenchArgs),
    /// Headless agent runner for external evaluation harnesses (e.g. Harbor)
    Agent(niki::cli::agent::AgentArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(unix)]
    restore_default_sigpipe();

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();
    let command: Commands = match cli.command {
        Some(command) => command,
        // Bare `niki` is the first thing a new user types, and where it lands
        // decides what they think this is.
        //
        // On a terminal, the chat surface is the right answer — that is what
        // Codex and Claude Code do, and matching them is the point. Without
        // one there is nothing to land on: the TUI tries to enter raw mode,
        // fails, returns immediately, and the process exits **0 having printed
        // nothing**. In a script that is indistinguishable from success, and
        // to a person it is a program that hangs for a moment and then hangs
        // up. So it prints what it can do and exits non-zero, which is the
        // conventional answer and the only one a script can act on.
        None => {
            if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                Commands::Chat(niki::cli::chat::ChatArgs::default())
            } else {
                use clap::CommandFactory as _;
                let mut cmd = Cli::command();
                let _ = cmd.print_help();
                eprintln!(
                    "\n\n`niki` with no arguments opens the chat interface, which needs a \
                     terminal.\nTry one of:\n  \
                     niki run \"<task>\"      run the pipeline on a coding task\n  \
                     niki doctor             check this install\n  \
                     niki --help             everything else"
                );
                std::process::exit(2);
            }
        }
    };

    match &command {
        Commands::Run(args) => niki::cli::run::handle(args).await?,
        Commands::Acp(args) => niki::cli::acp::handle(args).await?,
        Commands::Serve(args) => niki::cli::serve::handle(args).await?,
        Commands::Ui(args) => niki::cli::ui::handle(args)?,
        Commands::Audit(args) => niki::cli::audit::handle(args)?,
        Commands::Status(args) => niki::cli::status::handle(args).await?,
        Commands::Report(args) => niki::cli::report::handle(args).await?,
        Commands::Config { command } => niki::cli::config::handle(command).await?,
        Commands::Init { interactive, scan } => {
            niki::cli::config::handle(&niki::cli::config::ConfigCommands::Init {
                interactive: *interactive,
                scan: *scan,
            })
            .await?
        }
        Commands::Recommend(args) => niki::cli::recommend::handle(args).await?,
        Commands::Dashboard(args) => niki::cli::dashboard::handle(args)?,
        Commands::Eval(args) => niki::cli::eval::handle(args).await?,
        Commands::Memory(args) => niki::cli::memory::handle(args)?,
        Commands::Goal(args) => niki::cli::goal::handle(args).await?,
        Commands::Plan(args) => niki::cli::plan::handle(args).await?,
        Commands::Session(args) => niki::cli::session::handle(args)?,
        Commands::Resume(args) => niki::cli::resume::handle(args).await?,
        Commands::Auth { command } => niki::cli::auth::handle(command).await?,
        Commands::Providers(args) => niki::cli::providers::handle(args).await?,
        Commands::Doctor(args) => niki::cli::doctor::handle(args)?,
        Commands::Chat(args) => niki::cli::chat::handle(args).await?,
        Commands::Commands(args) => niki::cli::commands::handle(args)?,
        Commands::Smoke(args) => niki::cli::smoke::handle(args).await?,
        Commands::Research(args) => niki::cli::research::handle(args).await?,
        Commands::Verify(args) => niki::cli::verify::handle(args)?,
        Commands::Voice(args) => niki::cli::voice::handle(args).await?,
        Commands::Inspect(args) => niki::cli::inspect::handle(args)?,
        Commands::Architecture { command } => niki::cli::architecture::handle(command).await?,
        Commands::Index { command } => niki::cli::index::handle(command)?,
        Commands::Skills { command } => niki::cli::skills::handle(command)?,
        Commands::Bench(args) => niki::cli::bench::handle(args.clone()).await?,
        Commands::Agent(args) => niki::cli::agent::handle(args.clone()).await?,
    }

    Ok(())
}
