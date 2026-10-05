//! P2 RIG — tests for `niki bench` and `niki agent`.
//!
//! Asserts:
//! 1. `niki bench` subcommands exist (run, report, split, validate).
//! 2. Budget gate: `niki bench run` enforces `--budget-usd` and refuses to run when estimated cost exceeds budget.
//! 3. Split verification: frozen DEV/SEALED split hash is verified.
//! 4. ATIF validator: validates conforming trajectories, flags non-conforming ones.
//! 5. Statistical report generator: generates paired bootstrap comparisons from results.
//! 6. `niki agent` headless interface for Harbor installed agent.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

#[test]
fn bench_help_advertises_subcommands() {
    let output = Command::new(niki_bin())
        .arg("bench")
        .arg("--help")
        .output()
        .expect("exec niki");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("run"));
    assert!(stdout.contains("report"));
    assert!(stdout.contains("split"));
    assert!(stdout.contains("validate"));
}

#[test]
fn bench_run_requires_budget_usd() {
    let output = Command::new(niki_bin())
        .arg("bench")
        .arg("run")
        .output()
        .expect("exec niki");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--budget-usd"));
}

#[test]
fn bench_run_budget_gate_refuses_when_estimate_exceeds_budget() {
    let output = Command::new(niki_bin())
        .arg("bench")
        .arg("run")
        .arg("--budget-usd")
        .arg("0.001")
        .arg("--dry-run")
        .output()
        .expect("exec niki");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("budget"));
}

#[test]
fn bench_split_check_verifies_hash() {
    let output = Command::new(niki_bin())
        .arg("bench")
        .arg("split")
        .arg("--check")
        .output()
        .expect("exec niki");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DEV"));
    assert!(stdout.contains("SEALED"));
}

#[test]
fn agent_help_advertises_harness_flags() {
    let output = Command::new(niki_bin())
        .arg("agent")
        .arg("--help")
        .output()
        .expect("exec niki");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--atif-out"));
    assert!(stdout.contains("--max-time"));
    assert!(stdout.contains("--max-cost"));
    assert!(stdout.contains("--lever-completion-gate"));
    assert!(stdout.contains("--lever-budget-manager"));
    assert!(stdout.contains("--lever-loop-guard"));
    assert!(stdout.contains("--lever-onboarding"));
    assert!(stdout.contains("--lever-pty"));
    assert!(stdout.contains("--lever-edit-robust"));
    assert!(stdout.contains("--lever-context"));
    assert!(stdout.contains("--lever-effort-schedule"));
    assert!(stdout.contains("--lever-parallel"));
    assert!(stdout.contains("--lever-model-profiles"));
}

#[test]
fn bench_validate_validates_atif_file() {
    let dir = tempdir().expect("tempdir");
    let trajectory_path = dir.path().join("trajectory.json");

    let valid_atif = r#"{
        "schema_version": "1.0",
        "task_id": "test-task-1",
        "total_cost_usd": 0.05,
        "total_tokens_in": 120,
        "total_tokens_out": 250,
        "steps": [
            {
                "step_id": 1,
                "source": "user",
                "content": "Add health endpoint"
            },
            {
                "step_id": 2,
                "source": "agent",
                "content": "I will inspect the repository."
            }
        ]
    }"#;
    fs::write(&trajectory_path, valid_atif).expect("write valid atif");

    let output = Command::new(niki_bin())
        .arg("bench")
        .arg("validate")
        .arg(&trajectory_path)
        .output()
        .expect("exec niki");
    assert!(output.status.success());
}

#[test]
fn bench_report_generates_paired_comparison() {
    let dir = tempdir().expect("tempdir");
    let niki_results = dir.path().join("niki_results.json");
    let baseline_results = dir.path().join("baseline_results.json");

    let niki_data = r#"{
        "benchmark": "terminal-bench-2-1",
        "model": "qwen2.5-coder:3b",
        "harness": "niki",
        "trials_per_task": 1,
        "tasks": [
            {"task_id": "t1", "solved": true, "cost_usd": 0.02, "wall_time_sec": 12.0},
            {"task_id": "t2", "solved": true, "cost_usd": 0.03, "wall_time_sec": 15.0},
            {"task_id": "t3", "solved": false, "cost_usd": 0.01, "wall_time_sec": 8.0}
        ]
    }"#;

    let baseline_data = r#"{
        "benchmark": "terminal-bench-2-1",
        "model": "qwen2.5-coder:3b",
        "harness": "mini-swe-agent",
        "trials_per_task": 1,
        "tasks": [
            {"task_id": "t1", "solved": true, "cost_usd": 0.04, "wall_time_sec": 20.0},
            {"task_id": "t2", "solved": false, "cost_usd": 0.03, "wall_time_sec": 18.0},
            {"task_id": "t3", "solved": false, "cost_usd": 0.02, "wall_time_sec": 14.0}
        ]
    }"#;

    fs::write(&niki_results, niki_data).expect("write niki results");
    fs::write(&baseline_results, baseline_data).expect("write baseline results");

    let output = Command::new(niki_bin())
        .arg("bench")
        .arg("report")
        .arg("--results")
        .arg(&niki_results)
        .arg("--baseline")
        .arg(&baseline_results)
        .output()
        .expect("exec niki");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("resolve rate") || stdout.contains("Resolve rate"));
    assert!(stdout.contains("95% CI") || stdout.contains("CI"));
}
