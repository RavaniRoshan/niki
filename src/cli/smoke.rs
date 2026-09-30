use anyhow::Result;
use clap::Args;
use std::time::Instant;

use crate::config::NikiConfig;

#[derive(Args)]
pub struct SmokeArgs {
    /// Path to the project directory to smoke-test in
    #[arg(short, long, default_value = ".")]
    project_path: String,
    /// Sandbox backend: docker (container) or worktree (no container runtime).
    /// The worktree backend is the zero-setup path (Ollama + worktree = no
    /// container, no API key).
    #[arg(short, long)]
    backend: Option<super::run::BackendArg>,
}

pub async fn handle(args: &SmokeArgs) -> Result<()> {
    let project_path = std::path::PathBuf::from(&args.project_path);
    if !project_path.exists() {
        anyhow::bail!("project path does not exist: {}", project_path.display());
    }

    let config_path = project_path.join("niki.toml");
    if !config_path.exists() {
        anyhow::bail!(
            "no niki.toml found in {}; run `niki init` first",
            project_path.display()
        );
    }

    println!("Smoke test: running a trivial task to verify your setup works end-to-end...\n");

    let start = Instant::now();

    let mut cmd = std::process::Command::new(std::env::current_exe()?);
    cmd.args([
        "run",
        "--project",
        &args.project_path,
        "--max-rounds",
        "1",
        "Add a comment to the first source file you find (or create hello.txt with 'smoke test passed' if none exist).",
    ]);
    if let Some(backend) = &args.backend {
        let name = match backend {
            super::run::BackendArg::Docker => "docker",
            super::run::BackendArg::Worktree => "worktree",
        };
        cmd.args(["--backend", name]);
    } else {
        // No flag: inherit the configured backend rather than letting the
        // default take over.
        //
        // `niki smoke` is the command a user runs to find out whether their
        // setup works. It used to pass no `--backend` at all, so it fell
        // through to the `docker` default — on a machine with no container
        // runtime, `niki smoke` failed for exactly the reason `niki doctor`
        // had just told them was optional. Its own doc comment called
        // "Ollama + worktree" the zero-setup path; nothing in this function
        // used it.
        //
        // An explicit `--backend` still wins, so a user can smoke-test the
        // container path on a machine that defaults to worktree.
        let backend = match NikiConfig::load(&project_path) {
            Ok(cfg) => cfg.docker.backend,
            Err(_) => crate::sandbox::default_backend_for_this_machine(),
        };
        let name = match backend {
            crate::sandbox::SandboxBackend::Docker => "docker",
            crate::sandbox::SandboxBackend::Worktree => "worktree",
        };
        cmd.args(["--backend", name]);
    }
    let status = cmd.status()?;

    let elapsed = start.elapsed();

    if status.success() {
        println!("\nSmoke test PASSED");
        println!("  Elapsed: {:.1}s", elapsed.as_secs_f64());
        println!(
            "  Time-to-first-PR: {:.0}s (<5 min target).",
            elapsed.as_secs_f64()
        );
    } else {
        println!("\nSmoke test FAILED after {:.1}s", elapsed.as_secs_f64());
        println!("  Exit code: {:?}", status.code());
        println!("Run `niki doctor` for diagnostics.");
        std::process::exit(1);
    }

    Ok(())
}
