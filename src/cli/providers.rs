use anyhow::Result;
use clap::Subcommand;
use crate::config::NikiConfig;

#[derive(Subcommand)]
pub enum ProviderCommands {
    /// Check health of all configured providers (sends a minimal test request)
    Check,
}

#[derive(clap::Args)]
pub struct ProvidersArgs {
    #[command(subcommand)]
    pub command: ProviderCommands,
}

pub fn handle(args: &ProvidersArgs) -> Result<()> {
    match &args.command {
        ProviderCommands::Check => handle_check(),
    }
}

fn handle_check() -> Result<()> {
    let config = NikiConfig::load(std::path::Path::new("."))?;

    if config.providers.is_empty() {
        println!("No providers configured. Add providers to niki.toml first.");
        return Ok(());
    }

    println!("Checking provider health...\n");

    let runtime = tokio::runtime::Runtime::new()?;
    let results = runtime.block_on(
        crate::llm::failover::check_provider_health(&config.providers)
    );

    let mut all_ok = true;
    for r in &results {
        let status = if r.ok { "✓" } else { "✗" };
        let latency = format!("{}ms", r.latency_ms);
        let model = config.providers
            .get(&r.provider)
            .map(|p| p.default_model.as_str())
            .unwrap_or("unknown");

        if r.ok {
            println!("  {} {} ({}) — {} — healthy", status, r.provider, model, latency);
        } else {
            all_ok = false;
            println!("  {} {} ({}) — {} — {}", status, r.provider, model, latency, r.error.as_deref().unwrap_or("unknown error"));
        }
    }

    println!();
    if all_ok {
        println!("All {} providers healthy.", results.len());
    } else {
        let healthy = results.iter().filter(|r| r.ok).count();
        let total = results.len();
        println!("{}/{} providers healthy.", healthy, total);
    }

    Ok(())
}
