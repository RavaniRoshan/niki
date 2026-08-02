use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::artifacts::types::IssueCategory;
use crate::eval::{Difficulty, render_report_md, run_eval};

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

    /// Filter by category: security, logic, correctness, boundary, etc.
    #[arg(long)]
    pub category: Option<String>,

    /// Filter by difficulty: easy, medium, hard.
    #[arg(long)]
    pub difficulty: Option<String>,

    /// Output format: text (default) or json.
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Limit the number of cases to run.
    #[arg(short = 'n', long)]
    pub limit: Option<usize>,
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

    let mut report = run_eval(&dataset, args.live, &project).await?;

    // Apply filters
    if let Some(ref cat_str) = args.category {
        let cat_filter = parse_category(cat_str);
        report.cases.retain(|c| c.defect_category == cat_filter);
        report.n_cases = report.cases.len() as u32;
        // Recalculate metrics for filtered set
        report = recalculate_report(report);
    }

    if let Some(ref diff_str) = args.difficulty {
        let diff_filter = parse_difficulty(diff_str);
        report.cases.retain(|c| c.difficulty == diff_filter);
        report.n_cases = report.cases.len() as u32;
        report = recalculate_report(report);
    }

    if let Some(limit) = args.limit {
        report.cases.truncate(limit);
        report.n_cases = report.cases.len() as u32;
        report = recalculate_report(report);
    }

    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&report)?;
            println!("{}", json);
            let out = args
                .out
                .clone()
                .unwrap_or_else(|| PathBuf::from(".niki/eval"));
            std::fs::create_dir_all(&out)?;
            std::fs::write(out.join("eval_report.json"), &json)?;
            eprintln!("Wrote {}/eval_report.json", out.display());
        }
        _ => {
            let md = render_report_md(&report);
            println!("{}", md);
            let out = args
                .out
                .clone()
                .unwrap_or_else(|| PathBuf::from(".niki/eval"));
            std::fs::create_dir_all(&out)?;
            std::fs::write(out.join("eval_report.md"), &md)?;
            std::fs::write(
                out.join("eval_report.json"),
                serde_json::to_string_pretty(&report)?,
            )?;
            eprintln!(
                "Wrote {} and {}/eval_report.json",
                out.join("eval_report.md").display(),
                out.display()
            );
        }
    }

    // Regression detection: exit non-zero if any expected-caught defect was missed
    let regressions = report
        .cases
        .iter()
        .filter(|c| c.expected_caught && !c.niki.caught)
        .count();
    if regressions > 0 {
        eprintln!(
            "ERROR: {} regression(s) detected — expected-caught defects were missed",
            regressions
        );
        std::process::exit(1);
    }

    Ok(())
}

fn parse_category(s: &str) -> IssueCategory {
    match s.to_lowercase().as_str() {
        "security" => IssueCategory::Security,
        "logic" => IssueCategory::Logic,
        "correctness" => IssueCategory::Correctness,
        "boundary" => IssueCategory::Boundary,
        "bug" => IssueCategory::Bug,
        "performance" => IssueCategory::Performance,
        "style" => IssueCategory::Style,
        "test_gap" | "testgap" => IssueCategory::TestGap,
        "spec_deviation" | "specdeviation" => IssueCategory::SpecDeviation,
        _ => {
            eprintln!(
                "Unknown category: {}. Valid: security, logic, correctness, boundary, bug, performance, style, test_gap, spec_deviation",
                s
            );
            std::process::exit(1);
        }
    }
}

fn parse_difficulty(s: &str) -> Difficulty {
    match s.to_lowercase().as_str() {
        "easy" => Difficulty::Easy,
        "medium" => Difficulty::Medium,
        "hard" => Difficulty::Hard,
        _ => {
            eprintln!("Unknown difficulty: {}. Valid: easy, medium, hard", s);
            std::process::exit(1);
        }
    }
}

fn recalculate_report(mut report: crate::eval::EvalReport) -> crate::eval::EvalReport {
    let expected: Vec<&crate::eval::CaseResult> =
        report.cases.iter().filter(|c| c.expected_caught).collect();
    let n = expected.len().max(1) as f64;
    report.niki_catch_rate = expected.iter().filter(|c| c.niki.caught).count() as f64 / n;
    report.baseline_catch_rate = expected.iter().filter(|c| c.baseline.caught).count() as f64 / n;
    report.niki_false_approvals = expected.iter().filter(|c| !c.niki.caught).count() as u32;
    report.baseline_false_approvals = expected.iter().filter(|c| !c.baseline.caught).count() as u32;
    report.false_approval_reduction_pct = if report.baseline_false_approvals > 0 {
        ((report.baseline_false_approvals - report.niki_false_approvals) as f64
            / report.baseline_false_approvals as f64)
            * 100.0
    } else {
        0.0
    };

    // Per-category metrics
    let mut category_map: std::collections::HashMap<IssueCategory, Vec<&crate::eval::CaseResult>> =
        std::collections::HashMap::new();
    for c in &expected {
        category_map.entry(c.defect_category).or_default().push(c);
    }
    report.categories = category_map
        .into_iter()
        .map(|(cat, cs)| {
            let total = cs.len() as u32;
            let niki_caught = cs.iter().filter(|c| c.niki.caught).count() as u32;
            let baseline_caught = cs.iter().filter(|c| c.baseline.caught).count() as u32;
            crate::eval::CategoryMetrics {
                category: cat,
                total,
                niki_caught,
                baseline_caught,
                niki_catch_rate: if total > 0 {
                    niki_caught as f64 / total as f64
                } else {
                    0.0
                },
                baseline_catch_rate: if total > 0 {
                    baseline_caught as f64 / total as f64
                } else {
                    0.0
                },
            }
        })
        .collect();
    report
        .categories
        .sort_by(|a, b| format!("{:?}", a.category).cmp(&format!("{:?}", b.category)));

    report
}
