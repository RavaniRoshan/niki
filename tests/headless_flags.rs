//! W6 — the headless surface a harness actually calls.
//!
//! `niki run` is the product's scripting entry point, and a harness is not a person: it
//! spells budgets the way every other harness spells them (`--max-time`, `--max-cost`),
//! it pipes the task in rather than quoting it into an argv, and it needs a trajectory it can
//! validate. None of those worked. This file drives the **real binary** through each one.
//!
//! Every test here spawns `CARGO_BIN_EXE_niki` rather than calling `handle()` in process,
//! because the things being tested are the argument parser and the exit contract — neither of
//! which exists inside an in-process call.

mod common;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::{
    MockScriptBuilder, code_diff_json, review_verdict_approved_json, task_spec_json,
    test_report_json,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BEFORE: &str = "let end = start + size - 1;";
const AFTER: &str = "let end = start + size;";
const TARGET: &str = "src/list.rs";

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

fn chain_script(path: &Path) -> PathBuf {
    MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&task_spec_json()), 80, 120)
        .add_response(
            "mock-coder",
            &wrap_json(&code_diff_json(BEFORE, AFTER, TARGET)),
            200,
            80,
        )
        .add_response("mock-tester", &wrap_json(&test_report_json()), 100, 60)
        .add_response(
            "mock-reviewer",
            &wrap_json(&review_verdict_approved_json()),
            150,
            50,
        )
        .write(&path.to_path_buf())
}

fn toml(script_path: &Path) -> String {
    // `[providers.mock]` selects the engine's built-in scripted provider, so this file needs no
    // server and no port — the whole point is the CLI surface, not the model.
    r#"[pipeline]
topology = "multiagent"

[docker]
backend = "worktree"
extra_packages = []

[red_blue]
enabled = false

[security]
enabled = false

[providers.mock]
base_url = "{script}"
default_model = "mock-planner"

[agents.planner]
provider = "mock"
model = "mock-planner"

[agents.coder]
provider = "mock"
model = "mock-coder"

[agents.tester]
provider = "mock"
model = "mock-tester"
test_command = "true"

[agents.reviewer]
provider = "mock"
model = "mock-reviewer"
"#
    .replace("{script}", &script_path.display().to_string())
}

fn project() -> PathBuf {
    let repo = create_fixture_repo();
    let dir = repo.path().to_path_buf();
    chain_script(&dir.join(".niki-mock-script.json"));
    std::fs::write(
        dir.join("niki.toml"),
        toml(&dir.join(".niki-mock-script.json")),
    )
    .expect("write niki.toml");
    // Keep the tempdir alive for the whole test by leaking the handle; the process exits after.
    std::mem::forget(repo);
    dir
}

/// The three spellings every harness already uses must be advertised, or nobody finds them.
#[test]
fn the_help_advertises_the_harness_spellings() {
    let out = Command::new(niki_bin())
        .args(["run", "--help"])
        .output()
        .expect("niki runs");
    let help = String::from_utf8_lossy(&out.stdout);
    for flag in ["--max-time", "--max-cost", "--atif-out"] {
        assert!(
            help.contains(flag),
            "`niki run --help` does not mention {flag}; a harness cannot use a flag it cannot see.\n{help}"
        );
    }
}

/// The aliases must reach the same fields, not be a second parsing path.
#[test]
fn the_harness_spellings_parse_into_the_same_budget() {
    use clap::Parser;

    #[derive(clap::Parser)]
    struct Probe {
        #[command(flatten)]
        args: niki::cli::run::RunArgs,
    }

    let canonical = Probe::parse_from([
        "niki",
        "do a thing",
        "--max-wallclock-secs",
        "900",
        "--max-usd",
        "1.25",
    ]);
    let harness = Probe::parse_from([
        "niki",
        "do a thing",
        "--max-time",
        "900",
        "--max-cost",
        "1.25",
    ]);

    assert_eq!(canonical.args.max_wallclock_secs, Some(900));
    assert_eq!(harness.args.max_wallclock_secs, Some(900));
    assert_eq!(canonical.args.max_usd, Some(1.25));
    assert_eq!(harness.args.max_usd, Some(1.25));
}

/// A harness pipes the task in. It must not be told "no task given".
#[test]
fn a_task_can_be_piped_in_on_stdin() {
    let dir = project();
    let mut child = Command::new(niki_bin())
        .args([
            "run",
            "-",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--project",
            dir.to_str().expect("utf-8 path"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("niki starts");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(b"fix the pagination off-by-one\n")
        .expect("write the task");
    let out = child.wait_with_output().expect("niki finishes");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !combined.contains("No task given"),
        "the task arrived on stdin and was not read:\n{combined}"
    );
    assert!(
        combined.contains("niki/") || out.status.success(),
        "the piped task did not run the pipeline at all:\n{combined}"
    );
}

/// `-` with nothing behind it is still an empty task, and must be refused for free.
#[test]
fn an_empty_stdin_task_is_refused_before_anything_is_spent() {
    let dir = project();
    let mut child = Command::new(niki_bin())
        .args([
            "run",
            "-",
            "--backend",
            "worktree",
            "--quiet",
            "--project",
            dir.to_str().expect("utf-8 path"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("niki starts");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(b"   \n")
        .expect("write nothing");
    let out = child.wait_with_output().expect("niki finishes");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("No task given"),
        "whitespace on stdin must be refused like any other empty task:\n{combined}"
    );
}

/// `--atif-out` writes a trajectory an external validator can read.
#[test]
fn atif_out_writes_a_trajectory_that_validates() {
    let dir = project();
    let atif = dir.join("trajectory.json");
    let out = Command::new(niki_bin())
        .args([
            "run",
            "fix the pagination off-by-one",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--project",
            dir.to_str().expect("utf-8 path"),
            "--atif-out",
            atif.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("niki runs");
    assert!(
        out.status.success(),
        "the run failed, so the trajectory is unproven: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(&atif)
        .unwrap_or_else(|e| panic!("--atif-out wrote nothing to {}: {e}", atif.display()));
    let doc: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("the trajectory is not valid JSON: {e}"));

    assert_eq!(doc["agent"]["name"], "niki");
    assert!(
        doc["atif_version"]
            .as_str()
            .is_some_and(|v| v.starts_with("ATIF-")),
        "the trajectory must declare its schema version, or a validator cannot read it: {raw}"
    );

    let steps = doc["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("the trajectory has no steps array: {raw}"));
    assert!(
        !steps.is_empty(),
        "a trajectory with no steps is not a record of anything: {raw}"
    );
    for (i, step) in steps.iter().enumerate() {
        assert_eq!(
            step["step_id"].as_u64(),
            Some(i as u64 + 1),
            "step ids must be sequential from 1; the validator rejects a gap: {raw}"
        );
        let source = step["source"].as_str().unwrap_or("");
        assert!(
            ["system", "user", "agent"].contains(&source),
            "unknown step source {source:?}: {raw}"
        );
    }
    assert!(
        doc["final_metrics"]["total_steps"].as_u64().unwrap_or(0) > 0,
        "final_metrics.total_steps must count the steps: {raw}"
    );

    // The first user step has to carry the task, or the trajectory does not say what was asked.
    assert!(
        steps[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("pagination"),
        "the trajectory does not record the task it was given: {raw}"
    );

    // No partial file left behind for a harness to trip over.
    assert!(
        !atif.with_extension("json.partial").exists(),
        "a .partial file survived the write"
    );
}

/// A harness that only gets trajectories when runs work cannot tell a crash from a skip, so
/// the failing run has to produce one too.
#[test]
fn a_failed_run_still_writes_its_trajectory() {
    let dir = project();
    let failing = dir.join(".niki-fail-script.json");
    MockScriptBuilder::new()
        .add_error("mock-planner", "fatal", "planner exploded")
        .write(&failing);
    // The fixture reads one script; point the config's provider at the failing one by
    // overwriting the file the happy-path run would have used.
    std::fs::copy(&failing, dir.join(".niki-mock-script.json")).expect("swap the script");

    let atif = dir.join("failed-trajectory.json");
    let out = Command::new(niki_bin())
        .args([
            "run",
            "fix the pagination off-by-one",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--project",
            dir.to_str().expect("utf-8 path"),
            "--atif-out",
            atif.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("niki runs");

    assert!(
        !out.status.success(),
        "this test needs a run that fails; a green one proves nothing about the failure path"
    );
    assert!(
        atif.exists(),
        "the run failed and still wrote no trajectory — a harness would see a missing file, \
         which is indistinguishable from a crash"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&atif).expect("read the trajectory"))
            .expect("the failed run's trajectory is valid JSON");
    assert!(
        doc["steps"].as_array().is_some_and(|s| !s.is_empty()),
        "the failed run's trajectory has no steps"
    );
}

/// A path the process cannot write must not be silent, and must not fail the run.
#[test]
fn an_unwritable_atif_path_warns_and_leaves_the_run_alone() {
    let dir = project();
    // A **directory** as the target, not a file. An earlier version of this test wrote a file
    // over the target and expected a failure; `write_to` writes a sibling and renames, and
    // renaming a file onto a file succeeds, so the test was asserting that a working export
    // fails. The point is a path that genuinely cannot be written.
    let blocked = dir.join("blocked-atif-target");
    std::fs::create_dir_all(&blocked).expect("make the blocking directory");

    let out = Command::new(niki_bin())
        .args([
            "run",
            "fix the pagination off-by-one",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--project",
            dir.to_str().expect("utf-8 path"),
            "--atif-out",
            blocked.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("niki runs");

    assert!(
        out.status.success(),
        "a failed export must not turn a successful run into a failed one: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Warning") && stderr.contains("ATIF"),
        "an export that could not be written must say so on stderr:\n{stderr}"
    );
}
