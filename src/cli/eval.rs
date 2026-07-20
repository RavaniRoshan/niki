use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::eval::{render_report_md, run_eval};

#[derive(Args)]
pub struct EvalArgs {
    /// Path to the eval dataset TOML (default: evals/dataset.toml).
    #[arg(short, long)]
    pub dataset: Option<PathBuf>,

    /// Drive the real pipeline against live models (needs API keys + sandbox).
    /// Default: replay recorded fixtures deterministically (no keys, no cost).
    #[arg(long)]
    pub live: bool,

    /// Directory to write eval_report.md / eval_report.json.
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// Project directory used for --live runs (default: current directory).
    #[arg(short, long)]
    pub project: Option<PathBuf>,
}

pub async fn handle(args: &EvalArgs) -> Result<()> {
    let dataset = args
        .dataset
        .clone()
        .unwrap_or_else(|| PathBuf::from("evals/dataset.toml"));
    let project = match &args.project {
        Some(p) => p.clone(),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    let report = run_eval(&dataset, args.live, &project).await?;
    let md = render_report_md(&report);

    println!("{}", md);

    let out = args.out.clone().unwrap_or_else(|| PathBuf::from(".niki/eval"));
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("eval_report.md"), &md)?;
    std::fs::write(out.join("eval_report.json"), serde_json::to_string_pretty(&report)?)?;
    eprintln!("Wrote {} and {}/eval_report.json", out.join("eval_report.md").display(), out.display());

    Ok(())
}
