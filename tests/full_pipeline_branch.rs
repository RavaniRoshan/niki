//! C1 — Planner → Coder → Tester → Reviewer, end to end, headless, with a
//! `niki/<id>` branch that carries the change.
//!
//! `tests/planner_coder_branch.rs` proves the *smallest* thing the product sells:
//! one Planner and one Coder, on the `singleagent` fast path. `tests/run_lifecycle.rs`
//! proves the failure contract and the branch/blob relationship. Neither asserts
//! that **all four** stages ran in one run and that each one's output is on disk —
//! and a test that passes with three stages missing is worthless, because every
//! downstream assertion (the branch, the blob, the verdict) can be satisfied by a
//! pipeline that silently dropped a stage.
//!
//! So this file names all four and checks each one twice: once as an artifact the run
//! actually persisted, and once in `task.json`'s `agent_metrics`, which is written
//! from a different place in the engine (the stage boundary, not `deliver`). A stage
//! that ran but was never persisted, or persisted but never metered, fails here.
//!
//! Headless is part of the deliverable, so it is asserted rather than assumed: the run
//! is driven through the `niki` binary with `--output-format json`, stdout is parsed
//! as exactly one JSON envelope, and every diagnostic goes to stderr. A TUI on that
//! path would have written escape sequences into the envelope's stdout.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::{
    MockScriptBuilder, code_diff_json, review_verdict_approved_json, task_spec_json,
    test_report_json,
};
use niki::artifacts::types::{ReviewVerdict, TestReport, Verdict};

/// What the Coder is scripted to do, and what the branch must therefore contain.
const BEFORE: &str = "let end = start + size - 1;";
const AFTER: &str = "let end = start + size;";
const TARGET: &str = "src/list.rs";

/// The four roles this file claims ran, in the order the pipeline runs them.
const CHAIN: [&str; 4] = ["planner", "coder", "tester", "reviewer"];

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

/// One response per role of the full chain, each a valid typed artifact.
///
/// Every role must be scripted. A role with no scripted response fails its stage,
/// which ends the run before the Reviewer — so a missing entry here does not produce
/// a "three stages ran" result that some other assertion might tolerate; it produces a
/// hard failure.
fn full_chain_script(path: &Path) -> PathBuf {
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

/// The full multi-agent chain, on the worktree backend (no container runtime), with
/// the security auditor and the Red/Blue pair off so the stage list is exactly the
/// four this file claims.
///
/// `test_command = "true"` is the executed-verification hook: it resolves, it runs, it
/// exits 0. Without it the fixture repo (no `Cargo.toml`, no `package.json`) resolves
/// no test command at all, which is the `unverified` case `tests/verifier_verdict.rs`
/// covers — right for that file, wrong here, where the point is a *completed* run.
fn full_chain_toml(script_path: &Path) -> String {
    format!(
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
base_url = "{}"
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
"#,
        script_path.display()
    )
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

/// Names of the `*.json` artifacts a run persisted, sorted.
fn artifact_names(task_dir: &Path) -> Vec<String> {
    let dir = task_dir.join("artifacts");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("artifacts/ must exist after a completed run: {e}"))
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read_artifact(task_dir: &Path, name: &str) -> serde_json::Value {
    let path = task_dir.join("artifacts").join(name);
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name} must exist after a completed run: {e}"));
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{name} must be valid JSON: {e}"))
}

/// One run of the full chain, returning the parsed envelope and the task dir.
struct Run {
    envelope: serde_json::Value,
    task_dir: PathBuf,
    project: PathBuf,
}

fn run_full_chain() -> Run {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    full_chain_script(&script);
    std::fs::write(project.join("niki.toml"), full_chain_toml(&script)).expect("write niki.toml");

    // Precondition. A later `git show` that does not contain the fix is a real
    // absence rather than an artefact of reading a file that never had the bug.
    assert!(
        git(&project, &["show", &format!("HEAD:{TARGET}")]).contains(BEFORE),
        "the fixture must start with the off-by-one line, or this test proves nothing"
    );

    let out = Command::new(niki_bin())
        .args([
            "run",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--output-format",
            "json",
            "--project",
            project.to_str().expect("utf-8 project path"),
            "fix the pagination off-by-one",
        ])
        .output()
        .expect("niki runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a run that delivered a branch must exit zero, got {:?}. stderr:\n{stderr}",
        out.status.code()
    );

    // Headless means stdout carries exactly one machine-readable envelope and
    // nothing else. A TUI on this path would have written its escape sequences
    // into this same stream and the parse below would fail.
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must be exactly one JSON envelope, which is what 'headless' means here: \
                 {e}\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(envelope["status"], "completed", "{envelope}");

    let task_id = envelope["task_id"]
        .as_str()
        .unwrap_or_else(|| panic!("a completed run must report a task_id: {envelope}"))
        .to_string();
    let task_dir = project.join(".niki").join("tasks").join(task_id);

    // Keep the fixture alive until the assertions that need it have run.
    std::mem::forget(repo);
    Run {
        envelope,
        task_dir,
        project,
    }
}

/// The central claim: the whole chain runs, nobody is watching, and the deliverable is
/// a branch a human can review.
#[test]
fn the_full_chain_runs_headless_and_leaves_a_reviewable_branch() {
    let run = run_full_chain();

    // ── every stage actually ran ──────────────────────────────────────
    //
    // Checked twice per stage, from two different records. `artifacts/` is written
    // by `deliver`; `agent_metrics` is accrued at the stage boundary inside the
    // pipeline. A stage that ran but was never persisted fails the first check; a
    // stage that was persisted but never metered fails the second.
    let names = artifact_names(&run.task_dir);
    for role in CHAIN {
        assert!(
            names.contains(&format!("{role}.json")),
            "the {role} stage must have run and persisted its artifact; found {names:?}"
        );
        // Exactly one round: a `coder-2.json` here would mean the loop turned, which
        // this script never asks for, and the file would be the one carrying round 1.
        assert!(
            !names.contains(&format!("{role}-2.json")),
            "this run is a single pass, but {role} ran more than once: {names:?}"
        );
    }

    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(run.task_dir.join("task.json")).expect("task.json is written"),
    )
    .expect("task.json is JSON");
    let metered: Vec<String> = record["agent_metrics"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("role").and_then(|r| r.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        metered, CHAIN,
        "the pipeline must meter Planner → Coder → Tester → Reviewer, in that order"
    );

    // ── the Reviewer's verdict was recorded, not merely implied ───────
    //
    // Read back as the typed artifact, so a run that wrote `{}` or a schema-invalid
    // body cannot satisfy this by having the file.
    let reviewer: ReviewVerdict =
        serde_json::from_value(read_artifact(&run.task_dir, "reviewer.json"))
            .expect("reviewer.json must deserialize as a ReviewVerdict");
    assert_eq!(
        reviewer.verdict,
        Verdict::Approved,
        "the scripted reviewer approved, and that is what must be on disk"
    );
    assert!(
        !reviewer.overall_assessment.trim().is_empty(),
        "a ReviewVerdict with no assessment is a stage that returned nothing"
    );
    // And the Tester's, so "Tester ran" is more than a filename.
    let tester: TestReport = serde_json::from_value(read_artifact(&run.task_dir, "tester.json"))
        .expect("tester.json must deserialize as a TestReport");
    assert!(
        tester.test_results.total > 0,
        "the Tester ran but reported no tests: {:?}",
        tester.test_results
    );

    assert_eq!(
        run.envelope["verdict"], "Approved",
        "the envelope must carry the verdict the Reviewer produced: {}",
        run.envelope
    );
    assert_eq!(
        run.envelope["independently_reviewed"],
        serde_json::json!(true),
        "an Approved verdict with no independent reviewer is a fabricated pass: {}",
        run.envelope
    );
    assert_eq!(
        run.envelope["outcome"]["outcome"], "reviewed",
        "the outcome must name who approved: {}",
        run.envelope["outcome"]
    );
    assert_eq!(
        run.envelope["outcome"]["by"], "reviewer",
        "{}",
        run.envelope["outcome"]
    );

    // ── the deliverable: a branch that resolves and carries the change ─
    let branch = run.envelope["branch"]
        .as_str()
        .unwrap_or_else(|| panic!("a completed run must report a branch: {}", run.envelope))
        .to_string();
    assert!(
        branch.starts_with("niki/"),
        "the deliverable is a `niki/<id>` branch, got {branch:?}"
    );

    // The branch is a ref, not a name: git has to resolve it. `rev-parse --verify`
    // prints the resolved oid, so this reads captured output rather than letting a
    // bare sha onto the test harness's own stdout.
    let resolved = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(&run.project)
        .output()
        .expect("git rev-parse runs");
    assert!(
        resolved.status.success(),
        "refs/heads/{branch} does not exist on disk: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );

    let stat = git(&run.project, &["show", "--stat", "--oneline", &branch]);
    assert!(
        stat.contains(TARGET),
        "{branch} must carry a commit touching {TARGET}:\n{stat}"
    );

    // The committed blob, not the working tree. The worktree backend deliberately
    // leaves the working tree edited so the user can review in place, so a
    // working-tree read would pass even if nothing were committed.
    let on_branch = git(&run.project, &["show", &format!("{branch}:{TARGET}")]);
    assert!(
        on_branch.contains(AFTER),
        "the Coder's edit must be committed on {branch}, but the blob reads:\n{on_branch}"
    );
    assert!(
        !on_branch.contains(BEFORE),
        "{branch} still carries the pre-change line:\n{on_branch}"
    );

    // The executed verification is recorded as evidence, not as a claim.
    assert_eq!(
        run.envelope["verification"], "passed",
        "the real test command (`true`) ran and exited 0, and that is what must be recorded: {}",
        run.envelope
    );
    assert_eq!(
        run.envelope["tests_passed"],
        serde_json::json!(true),
        "{}",
        run.envelope
    );
    assert_eq!(
        run.envelope["revision_rounds"],
        serde_json::json!(0),
        "{}",
        run.envelope
    );
}

/// The negative half of "every stage ran": a chain with no Reviewer response must not
/// be able to report the same thing.
///
/// Without this, every assertion above is satisfied by a pipeline that stopped after
/// the Tester — because the envelope's `verdict` defaults to `Approved` when nothing
/// overrode it, which is exactly the fabrication `tests/run_lifecycle.rs` also pins.
/// The branch gate is the second half: a run that never got evaluated must not deliver
/// a branch either.
#[test]
fn a_chain_that_never_reached_the_reviewer_reports_no_branch() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    // Identical to `full_chain_script` minus the reviewer response.
    MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&task_spec_json()), 80, 120)
        .add_response(
            "mock-coder",
            &wrap_json(&code_diff_json(BEFORE, AFTER, TARGET)),
            200,
            80,
        )
        .add_response("mock-tester", &wrap_json(&test_report_json()), 100, 60)
        .write(&script);
    std::fs::write(project.join("niki.toml"), full_chain_toml(&script)).expect("write niki.toml");

    let out = Command::new(niki_bin())
        .args([
            "run",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--output-format",
            "json",
            "--project",
            project.to_str().expect("utf-8 project path"),
            "fix the pagination off-by-one",
        ])
        .output()
        .expect("niki runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a run that could not review its own work must not exit zero. stderr:\n{stderr}"
    );
    assert!(
        git(&project, &["branch", "--list", "niki/*"])
            .trim()
            .is_empty(),
        "a run with no Reviewer verdict must not deliver a branch"
    );
}
