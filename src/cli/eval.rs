use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::artifacts::types::IssueCategory;
use crate::eval::{Difficulty, render_report_md, run_eval};

#[derive(Args)]
pub struct EvalArgs {
    #[command(subcommand)]
    pub command: Option<EvalCommands>,
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

#[derive(Subcommand)]
pub enum EvalCommands {
    /// Record a maintainer merge-worthiness judgment on one case
    /// (METR-style human layer over the automated grader).
    Grade {
        /// Case id as in the dataset TOML
        #[arg(long)]
        case: String,
        /// Verdict: `merge` (would merge to main) or `no-merge`
        #[arg(long)]
        verdict: String,
        /// Reviewer name recorded with the grade
        #[arg(long)]
        reviewer: String,
        /// Optional note (what would block the merge, or why it passes)
        #[arg(long, default_value = "")]
        note: String,
        /// Path to the eval dataset TOML (default: evals/dataset.toml)
        #[arg(short, long)]
        dataset: Option<PathBuf>,
    },
}

pub async fn handle(args: &EvalArgs) -> Result<()> {
    if let Some(EvalCommands::Grade {
        case,
        verdict,
        reviewer,
        note,
        dataset,
    }) = &args.command
    {
        return handle_grade(case, verdict, reviewer, note, dataset.as_ref());
    }
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

    // Disclosure manifest (research report VG-12 / docs/benchmarks.md): every
    // published number travels with harness commit, dataset, date, mode, and
    // cost — the minimum for anyone else to reproduce or dispute the figures.
    let manifest_out = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(".niki/eval"));
    std::fs::create_dir_all(&manifest_out)?;
    let manifest = serde_json::json!({
        "date_utc": report.run_date,
        "niki_version": report.niki_version,
        "mode": if report.live { "live" } else { "replay" },
        "dataset": dataset.display().to_string(),
        "n_cases": report.n_cases,
        "harness_commit": report.harness_commit,
        "harness_dirty": report.harness_dirty,
        "niki_catch_rate": report.niki_catch_rate,
        "baseline_catch_rate": report.baseline_catch_rate,
        "false_approval_reduction_pct": report.false_approval_reduction_pct,
        "total_cost_usd": report.total_cost_usd,
        "cost_per_niki_caught": report.cost_per_niki_caught,
        "graded_cases": report.graded_cases,
        "grader_agreement": report.grader_agreement,
        "success_definition": "seeded defect surfaced by reviewer issues or upheld Red challenge (test-passing only, not maintainer-merge grading)",
    });
    std::fs::write(
        manifest_out.join("eval-manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    eprintln!("Wrote {}/eval-manifest.json", manifest_out.display());

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

/// Record a maintainer grade for one eval case. The grade lives next to the
/// dataset (`<dataset-dir>/grades/<case-id>.json`) so it travels with the
/// fixtures and shows up in every future report's agreement metric.
fn handle_grade(
    case: &str,
    verdict: &str,
    reviewer: &str,
    note: &str,
    dataset: Option<&PathBuf>,
) -> Result<()> {
    use crate::eval::MaintainerGrade;

    let merge_worthy = match verdict.to_lowercase().as_str() {
        "merge" | "merge-worthy" | "yes" | "true" => true,
        "no-merge" | "no_merge" | "not-merge-worthy" | "no" | "false" => false,
        other => anyhow::bail!("unknown verdict '{other}': expected `merge` or `no-merge`"),
    };
    let dataset_path = dataset
        .cloned()
        .unwrap_or_else(|| PathBuf::from("evals/dataset.toml"));
    let dataset_dir = dataset_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // Validate the case id against the dataset so typos don't create orphans.
    let raw = std::fs::read_to_string(&dataset_path)
        .map_err(|e| anyhow::anyhow!("cannot read dataset {}: {e}", dataset_path.display()))?;
    #[derive(serde::Deserialize)]
    struct MinimalCase {
        id: String,
    }
    #[derive(serde::Deserialize)]
    struct MinimalDataset {
        #[serde(default)]
        cases: Vec<MinimalCase>,
    }
    let ds: MinimalDataset = toml::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("cannot parse dataset {}: {e}", dataset_path.display()))?;
    if !ds.cases.iter().any(|c| c.id == case) {
        anyhow::bail!(
            "unknown case '{case}': no such id in {}",
            dataset_path.display()
        );
    }
    let grades_dir = dataset_dir.join("grades");
    std::fs::create_dir_all(&grades_dir)?;
    let grade = MaintainerGrade {
        case_id: case.to_string(),
        reviewer: reviewer.to_string(),
        merge_worthy,
        note: note.to_string(),
        date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
    };
    let path = grades_dir.join(format!("{case}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&grade)?)?;
    println!(
        "Recorded grade for '{case}': {} (reviewer: {reviewer}) → {}",
        if merge_worthy {
            "merge-worthy"
        } else {
            "not merge-worthy"
        },
        path.display()
    );
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

    // Recompute spend totals over the filtered set so cost discipline survives
    // --category/--difficulty/--limit slicing.
    report.total_cost_usd = report.cases.iter().map(|c| c.cost_usd).sum();
    let caught_n = report
        .cases
        .iter()
        .filter(|c| c.expected_caught && c.niki.caught)
        .count();
    report.cost_per_niki_caught = if caught_n > 0 && report.total_cost_usd > 0.0 {
        Some(report.total_cost_usd / caught_n as f64)
    } else {
        None
    };
    // Recompute the human layer over the filtered set from the grades the
    // report already carries (no re-read: filters must not drop judgments).
    report.graded_cases = report
        .cases
        .iter()
        .filter(|c| c.expected_caught && report.grades.contains_key(&c.case_id))
        .count() as u32;
    report.grader_agreement = crate::eval::grader_agreement(&report.cases, &report.grades);

    report
}
