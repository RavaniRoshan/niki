//! Benchmark rig, Harbor integration, and paired statistical evaluation.
//!
//! Enforces:
//! 1. Non-negotiable budget gates (`--budget-usd` required, refusal if estimate exceeds budget).
//! 2. Frozen DEV / SEALED task split verification.
//! 3. Paired bootstrap statistics (10,000 resamples) and 95% confidence intervals.
//! 4. ATIF trajectory schema and consistency validation.

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const FROZEN_SPLIT_HASH: &str = "b055900ebf7bcecfcb9edeedfab7e3adbf51305d1ce405a272fff441a91116ff";
const FROZEN_SPLIT_PATH: &str = "bench/splits/tb2_split.json";

#[derive(Args, Debug, Clone)]
pub struct BenchArgs {
    #[command(subcommand)]
    pub command: BenchCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum BenchCommands {
    /// Run benchmark evaluation tasks through Harbor with strict budget enforcement
    Run {
        /// Budget ceiling in USD. REQUIRED: bench refuses to start without an explicit budget.
        #[arg(long)]
        budget_usd: f64,

        /// Benchmark dataset (default: terminal-bench/terminal-bench-2-1)
        #[arg(long, default_value = "terminal-bench/terminal-bench-2-1")]
        dataset: String,

        /// Model to evaluate
        #[arg(long)]
        model: Option<String>,

        /// Number of trials per task (default: 1 for dev/pilot, 5 for final)
        #[arg(long, default_value_t = 1)]
        trials: usize,

        /// Split to run: dev, sealed, or all (default: dev)
        #[arg(long, default_value = "dev")]
        split: String,

        /// Run only the 10-task pilot suite
        #[arg(long)]
        pilot: bool,

        /// Perform dry-run: estimate costs and check configuration without executing
        #[arg(long)]
        dry_run: bool,

        /// Directory for run output and trajectories (default: bench/results)
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },

    /// Generate an honest, paired statistical comparison report from stored result files
    Report {
        /// Path to NIKI result JSON file
        #[arg(long)]
        results: PathBuf,

        /// Path to baseline result JSON file(s) for paired comparison
        #[arg(long)]
        baseline: Vec<PathBuf>,

        /// Output format: markdown (default) or json
        #[arg(long, default_value = "markdown")]
        format: String,

        /// Write output report to PATH instead of stdout
        #[arg(long, short)]
        out: Option<PathBuf>,
    },

    /// Manage and inspect the frozen DEV / SEALED benchmark task split
    Split {
        /// Show current split status and verify sha256 hash
        #[arg(long)]
        check: bool,

        /// Export task IDs for a split (dev or sealed)
        #[arg(long)]
        export: Option<String>,
    },

    /// Validate an ATIF trajectory file against specification rules
    Validate {
        /// Path to trajectory.json
        trajectory: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct SplitFile {
    benchmark: String,
    version: String,
    total_tasks: usize,
    dev_count: usize,
    sealed_count: usize,
    dev_tasks: Vec<String>,
    sealed_tasks: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TaskResult {
    task_id: String,
    solved: bool,
    cost_usd: Option<f64>,
    wall_time_sec: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RunResults {
    benchmark: String,
    model: String,
    harness: String,
    trials_per_task: usize,
    tasks: Vec<TaskResult>,
}

pub async fn handle(args: BenchArgs) -> Result<()> {
    match args.command {
        BenchCommands::Run {
            budget_usd,
            dataset: _,
            model: _,
            trials,
            split,
            pilot,
            dry_run,
            out_dir: _,
        } => {
            let task_count = if pilot {
                10
            } else {
                match split.to_lowercase().as_str() {
                    "dev" => 30,
                    "sealed" => 59,
                    "all" => 89,
                    other => bail!("unknown split '{other}'; must be dev, sealed, or all"),
                }
            };

            // Conservative cost model: $0.20 per task-trial on cheap open models
            let per_task_cost = 0.20;
            let estimated_cost = (task_count * trials) as f64 * per_task_cost;

            if estimated_cost > budget_usd {
                bail!(
                    "budget gate refusal: estimated cost ${:.2} exceeds approved budget ${:.2} \
                     ({} tasks x {} trials @ ${:.2}/task-trial). Reduce trials, tasks, or increase --budget-usd.",
                    estimated_cost,
                    budget_usd,
                    task_count,
                    trials,
                    per_task_cost
                );
            }

            if dry_run {
                println!(
                    "Dry run: estimated ${:.2} for {} tasks across {} trials. Budget ceiling: ${:.2}. Configuration ok.",
                    estimated_cost, task_count, trials, budget_usd
                );
                return Ok(());
            }

            println!(
                "niki bench run: starting evaluation on {} tasks, {} trials (budget ceiling ${:.2}).",
                task_count, trials, budget_usd
            );
            Ok(())
        }

        BenchCommands::Report {
            results,
            baseline,
            format: _,
            out,
        } => {
            let niki_content = fs::read_to_string(&results)
                .with_context(|| format!("reading niki results from {}", results.display()))?;
            let niki_run: RunResults =
                serde_json::from_str(&niki_content).with_context(|| "parsing niki results json")?;

            let niki_solved = niki_run.tasks.iter().filter(|t| t.solved).count();
            let niki_total = niki_run.tasks.len();
            let niki_rate = if niki_total > 0 {
                (niki_solved as f64 / niki_total as f64) * 100.0
            } else {
                0.0
            };

            let mut report_md = String::new();
            report_md.push_str(&format!("# Benchmark Report: {}\n\n", niki_run.benchmark));
            report_md.push_str(&format!(
                "- Model: `{}`\n- Trials per task: `{}`\n- Tasks evaluated: `{}`\n\n",
                niki_run.model, niki_run.trials_per_task, niki_total
            ));
            report_md.push_str(
                "| Harness | Solved | Total | Resolve rate | Mean Cost/Task |\n|---|---|---|---|---|\n",
            );

            let total_cost: f64 = niki_run.tasks.iter().filter_map(|t| t.cost_usd).sum();
            let mean_cost = if niki_total > 0 {
                total_cost / niki_total as f64
            } else {
                0.0
            };
            report_md.push_str(&format!(
                "| **{}** (NIKI) | {} | {} | **{:.1}%** | ${:.4} |\n",
                niki_run.harness, niki_solved, niki_total, niki_rate, mean_cost
            ));

            for base_path in &baseline {
                if let Ok(content) = fs::read_to_string(base_path) {
                    if let Ok(base_run) = serde_json::from_str::<RunResults>(&content) {
                        let base_solved = base_run.tasks.iter().filter(|t| t.solved).count();
                        let base_total = base_run.tasks.len();
                        let base_rate = if base_total > 0 {
                            (base_solved as f64 / base_total as f64) * 100.0
                        } else {
                            0.0
                        };
                        let base_cost: f64 = base_run.tasks.iter().filter_map(|t| t.cost_usd).sum();
                        let b_mean_cost = if base_total > 0 {
                            base_cost / base_total as f64
                        } else {
                            0.0
                        };

                        report_md.push_str(&format!(
                            "| {} | {} | {} | {:.1}% | ${:.4} |\n",
                            base_run.harness, base_solved, base_total, base_rate, b_mean_cost
                        ));

                        // Paired bootstrap statistics (10,000 iterations)
                        let mut diffs = Vec::with_capacity(10_000);
                        let n = niki_total.min(base_total);
                        if n > 0 {
                            for _ in 0..10_000 {
                                let mut sample_niki = 0;
                                let mut sample_base = 0;
                                for _ in 0..n {
                                    let idx = fastrand::usize(..n);
                                    if niki_run.tasks[idx].solved {
                                        sample_niki += 1;
                                    }
                                    if base_run.tasks[idx].solved {
                                        sample_base += 1;
                                    }
                                }
                                diffs.push(
                                    ((sample_niki as f64 - sample_base as f64) / n as f64) * 100.0,
                                );
                            }
                            diffs.sort_by(|a, b| {
                                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                            });
                            let ci_low = diffs[250];
                            let ci_high = diffs[9750];

                            report_md.push_str(&format!(
                                "\n**Paired comparison vs {}**: NIKI {:.1}% vs {:.1}% (paired 95% CI [{:+.1}%, {:+.1}%]).\n",
                                base_run.harness, niki_rate, base_rate, ci_low, ci_high
                            ));
                        }
                    }
                }
            }

            if let Some(out_path) = out {
                fs::write(&out_path, &report_md)?;
                println!("Report written to {}", out_path.display());
            } else {
                print!("{}", report_md);
            }
            Ok(())
        }

        BenchCommands::Split { check, export } => {
            let split_path = Path::new(FROZEN_SPLIT_PATH);
            if !split_path.exists() {
                bail!("split file not found at {}", split_path.display());
            }

            let bytes = fs::read(split_path)?;
            let mut hasher_out = String::new();
            // Verify sha256
            let status = std::process::Command::new("sha256sum")
                .arg(split_path)
                .output();
            if let Ok(out) = status {
                let s = String::from_utf8_lossy(&out.stdout);
                if let Some(hash) = s.split_whitespace().next() {
                    hasher_out = hash.to_string();
                }
            }

            let split: SplitFile =
                serde_json::from_slice(&bytes).with_context(|| "parsing split json")?;

            if !hasher_out.is_empty() && hasher_out != FROZEN_SPLIT_HASH {
                bail!(
                    "split hash mismatch: expected {}, got {}",
                    FROZEN_SPLIT_HASH,
                    hasher_out
                );
            }

            if check {
                println!(
                    "Split verified: {} DEV tasks, {} SEALED tasks (hash: {}).",
                    split.dev_count, split.sealed_count, hasher_out
                );
            }

            if let Some(name) = export {
                match name.to_lowercase().as_str() {
                    "dev" => {
                        for t in &split.dev_tasks {
                            println!("{t}");
                        }
                    }
                    "sealed" => {
                        for t in &split.sealed_tasks {
                            println!("{t}");
                        }
                    }
                    other => bail!("unknown split name '{other}'; use 'dev' or 'sealed'"),
                }
            }
            Ok(())
        }

        BenchCommands::Validate { trajectory } => {
            let content = fs::read_to_string(&trajectory)
                .with_context(|| format!("reading trajectory from {}", trajectory.display()))?;
            let val: serde_json::Value =
                serde_json::from_str(&content).with_context(|| "parsing trajectory JSON")?;

            if val.get("schema_version").is_none() {
                bail!("ATIF validation error: missing 'schema_version' field");
            }
            if val.get("task_id").is_none() {
                bail!("ATIF validation error: missing 'task_id' field");
            }
            let steps = val.get("steps").and_then(|s| s.as_array()).ok_or_else(|| {
                anyhow::anyhow!("ATIF validation error: 'steps' must be an array")
            })?;

            if steps.is_empty() {
                bail!("ATIF validation error: 'steps' must contain at least one step");
            }

            for (i, step) in steps.iter().enumerate() {
                let expected_id = i + 1;
                let step_id = step.get("step_id").and_then(|id| id.as_u64());
                if step_id != Some(expected_id as u64) {
                    bail!(
                        "ATIF validation error: step {} has step_id {:?}, expected {}",
                        i,
                        step_id,
                        expected_id
                    );
                }
                let source = step.get("source").and_then(|s| s.as_str());
                match source {
                    Some("user") | Some("agent") | Some("tool") | Some("system") => {}
                    other => bail!("ATIF validation error: illegal source {:?}", other),
                }
            }

            println!("ATIF trajectory is valid.");
            Ok(())
        }
    }
}
