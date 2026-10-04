//! C2 — the bounded revision loop, the adversarial turn, and the retry marker on the
//! `niki serve` seam.
//!
//! ## What actually turns the loop
//!
//! A stage's LLM-produced `TestReport` does **not** gate the pipeline. The gate is the
//! Reviewer's verdict (`pipeline.rs`, `apply_reviewer_verdict`), and the Reviewer's
//! prompt carries the Tester's report as a third input artifact — which is the real
//! causal chain: a failing test report reaches the Reviewer, the Reviewer asks for a
//! revision, the Coder runs again. That is what is scripted here, and the second
//! Tester report is scripted green so the loop can only end if the Tester genuinely
//! re-ran.
//!
//! Saying it explicitly because the first draft of this file assumed a failing Tester
//! spins the loop by itself. It does not, and a test written on that assumption would
//! have passed for the wrong reason.
//!
//! `tests/pipeline_guards.rs::test_always_revision_needed_stops_at_max_rounds` already
//! drives the Reviewer-only loop to its bound. What it does not check is that the
//! *Tester* re-ran in each round, that the second Tester report is the passing one, or
//! that a retry is visible to a shell. Those are here.
//!
//! ## The seam
//!
//! The third test drives `niki serve` with the same failing-then-passing script and
//! reads the `attempt` field off the emitted `stage.start` notifications. That field is
//! produced by the adapter from the engine's own per-role `StageStart` events
//! (`cli/serve.rs`, `AdapterSink::map`), so `attempt == 2` is the engine saying "this
//! role started a second time", not the test counting its own frames.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::{
    MockScriptBuilder, code_diff_json, review_verdict_approved_json, task_spec_json,
};
use serde_json::json;

const BEFORE: &str = "let end = start + size - 1;";
const TARGET: &str = "src/list.rs";

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

/// A Tester report in which the one written test **fails**.
///
/// The shape is the same as the shared passing fixture with `status`/`failed` flipped,
/// so the only difference between the two scripts in this file is the thing under test.
fn failing_test_report_json() -> String {
    json!({
        "tests_written": [
            {
                "name": "test_last_page_item",
                "file_path": "tests/list_test.rs",
                "description": "Verify the last item of a page is returned",
                "status": "failed",
                "error_message": "assertion failed: paginate dropped the final element"
            }
        ],
        "test_results": {
            "total": 1,
            "passed": 0,
            "failed": 1,
            "skipped": 0,
            "errors": 0
        },
        "coverage_summary": null,
        "edge_cases_found": ["Last page still drops its final element"],
        "tester_notes": "The fix did not take: the last item of the final page is still missing."
    })
    .to_string()
}

/// A passing Tester report, matching the shared fixture.
fn passing_test_report_json() -> String {
    common::mock_llm::test_report_json()
}

/// A Reviewer asking for a revision, naming a different problem on every round.
///
/// `description` has to differ per round: the harness deliberately refuses to pay for
/// a revision round that comes back with a byte-identical critique
/// (`revision_hold` → `UnchangedIssues`), so scripting one issue repeatedly would test
/// that early stop instead of the round bound.
fn revision_needed_json(issue: &str) -> String {
    let item = json!({
        "severity": "major",
        "category": "correctness",
        "file_path": "src/list.rs",
        "line_range": "1-3",
        "description": issue,
        "suggested_fix": null
    });
    json!({
        "verdict": "revision_needed",
        "overall_assessment": "The executed test report is red; the change is not done.",
        "quality_scores": {
            "correctness": 5,
            "code_quality": 6,
            "test_coverage": 4,
            "spec_adherence": 6
        },
        "issues": [item],
        "strengths": [],
        "feedback": {
            "critical_issues": [item],
            "guidance": "Fix the failing case before resubmitting.",
            "keep_unchanged": [],
            "revision_round": 0
        },
        "red_reconciliation": null
    })
    .to_string()
}

/// Coder responses that chain: each one's `search` is text the previous patch wrote.
///
/// Without this the second round's patch would not apply, and the pipeline would divert
/// into its patch-repair allowance — spending the test on the wrong mechanism.
fn chained_coder_responses() -> Vec<String> {
    [
        (BEFORE, "let end = start + size;"),
        ("let end = start + size;", "let end = start + size + 1;"),
        ("let end = start + size + 1;", "let end = start + size + 2;"),
    ]
    .iter()
    .map(|(search, replace)| wrap_json(&code_diff_json(search, replace, TARGET)))
    .collect()
}

/// Failing Tester → Reviewer asks → passing Tester → Reviewer approves.
///
/// Exactly one revision, and the second Tester report is green: the loop can only stop
/// here if the Tester really ran a second time.
fn fails_then_passes_script(path: &Path) -> PathBuf {
    let coders = chained_coder_responses();
    MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&task_spec_json()), 80, 120)
        .add_response("mock-coder", &coders[0], 200, 80)
        .add_response(
            "mock-tester",
            &wrap_json(&failing_test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&revision_needed_json(
                "round one: the last page item is still dropped",
            )),
            150,
            60,
        )
        .add_response("mock-coder", &coders[1], 200, 80)
        .add_response(
            "mock-tester",
            &wrap_json(&passing_test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&review_verdict_approved_json()),
            150,
            50,
        )
        .write(&path.to_path_buf())
}

/// A Tester that fails every round and a Reviewer that keeps finding something new.
///
/// `max_rounds` coder responses for `max_rounds + 1` rounds: the mock cycles its
/// responses modulo their count, so under-scripting would replay round 0's patch and
/// send the run down the patch-repair path instead of the round bound.
fn always_failing_script(path: &Path, rounds: usize) -> PathBuf {
    let coders = chained_coder_responses();
    let mut b = MockScriptBuilder::new().add_response(
        "mock-planner",
        &wrap_json(&task_spec_json()),
        80,
        120,
    );
    for i in 0..rounds {
        b = b
            .add_response("mock-coder", &coders[i % coders.len()], 200, 80)
            .add_response(
                "mock-tester",
                &wrap_json(&failing_test_report_json()),
                100,
                60,
            )
            .add_response(
                "mock-reviewer",
                &wrap_json(&revision_needed_json(&format!(
                    "round {i}: the failing case is still present"
                ))),
                150,
                60,
            );
    }
    b.write(&path.to_path_buf())
}

fn chain_toml(script_path: &Path, max_revision_rounds: Option<u32>) -> String {
    let rounds = match max_revision_rounds {
        Some(n) => format!("max_revision_rounds = {n}"),
        None => String::new(),
    };
    format!(
        r#"[pipeline]
topology = "multiagent"
{rounds}

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

[agents.reviewer]
provider = "mock"
model = "mock-reviewer"
"#,
        script_path.display(),
        rounds = if rounds.is_empty() {
            String::new()
        } else {
            format!("{rounds}\n")
        }
    )
}

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

fn artifact_names(task_dir: &Path) -> Vec<String> {
    let dir = task_dir.join("artifacts");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("artifacts/ must exist after a run: {e}"))
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    names
}

fn read_artifact(task_dir: &Path, name: &str) -> serde_json::Value {
    let path = task_dir.join("artifacts").join(name);
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name} must exist: {e}"));
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{name} must be valid JSON: {e}"))
}

struct Run {
    envelope: serde_json::Value,
    task_dir: PathBuf,
}

fn niki_run(project: &Path, script: &Path, max_revision_rounds: Option<u32>) -> Run {
    std::fs::write(
        project.join("niki.toml"),
        chain_toml(script, max_revision_rounds),
    )
    .expect("write niki.toml");

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
        "run must succeed, got {:?}. stderr:\n{stderr}",
        out.status.code()
    );
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must be one JSON envelope: {e}\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    let task_id = envelope["task_id"]
        .as_str()
        .unwrap_or_else(|| panic!("task_id missing: {envelope}"))
        .to_string();
    Run {
        envelope,
        task_dir: project.join(".niki").join("tasks").join(task_id),
    }
}

/// A failing Tester report that reaches the Reviewer, gets a revision requested, and a
/// second pass that is green.
///
/// The assertions are on the *second* Tester report specifically: if the loop had not
/// turned, `tester-2.json` would not exist, and `coder-2.json` would not either.
#[test]
fn a_failing_tester_turns_the_loop_and_the_coder_runs_twice() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    fails_then_passes_script(&script);

    let run = niki_run(&project, &script, None);
    assert_eq!(run.envelope["status"], "completed", "{}", run.envelope);

    // ── the loop turned exactly once ─────────────────────────────────
    assert_eq!(
        run.envelope["revision_rounds"],
        serde_json::json!(1),
        "one failing Tester report must cost exactly one revision round: {}",
        run.envelope
    );
    assert_eq!(
        run.envelope["verdict"], "Approved",
        "the second pass is green and approved: {}",
        run.envelope
    );

    // ── the Coder ran twice ───────────────────────────────────────────
    let names = artifact_names(&run.task_dir);
    assert!(
        names.contains(&"coder.json".to_string()),
        "round 0's Coder artifact is missing: {names:?}"
    );
    assert!(
        names.contains(&"coder-2.json".to_string()),
        "the Coder must have run a second time after the revision request: {names:?}"
    );
    assert!(
        !names.contains(&"coder-3.json".to_string()),
        "a third Coder pass would mean the loop turned twice: {names:?}"
    );
    assert!(
        names.contains(&"reviewer-2.json".to_string()),
        "the Reviewer must be consulted again after the revision: {names:?}"
    );

    // ── and it was the Tester that was red, then green ───────────────
    let first = read_artifact(&run.task_dir, "tester.json");
    assert_eq!(
        first["test_results"]["failed"],
        serde_json::json!(1),
        "round 0's Tester report must record the failure: {first}"
    );
    let second = read_artifact(&run.task_dir, "tester-2.json");
    assert_eq!(
        second["test_results"]["failed"],
        serde_json::json!(0),
        "round 1's Tester report must record no failures: {second}"
    );
    assert_eq!(
        second["test_results"]["passed"],
        serde_json::json!(1),
        "round 1's Tester report must record the pass: {second}"
    );

    // The revision was real, not a duplicated round: the two Tester artifacts differ.
    assert_ne!(
        first, second,
        "the second Tester pass must have re-run, not repeated the first"
    );

    // Both Coder passes really happened, per the metered record too.
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(run.task_dir.join("task.json")).expect("task.json is written"),
    )
    .expect("task.json is JSON");
    let coder_passes = record["agent_metrics"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("coder"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(
        coder_passes, 2,
        "exactly two Coder stage boundaries must be metered: {record}"
    );
    std::mem::forget(repo);
}

/// The bound. A Tester that fails every round, a Reviewer that keeps finding something
/// new, and a loop that must stop at `max_revision_rounds` — not spin, not stop early.
///
/// `max_revision_rounds = 2` means rounds 0, 1 and 2 run and then the loop exits with
/// `round == 2`: three rounds of work, two revisions spent.
#[test]
fn a_tester_that_always_fails_stops_at_max_revision_rounds() {
    const MAX_ROUNDS: u32 = 2;
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    always_failing_script(&script, MAX_ROUNDS as usize);

    let run = niki_run(&project, &script, Some(MAX_ROUNDS));

    assert_eq!(
        run.envelope["revision_rounds"],
        serde_json::json!(MAX_ROUNDS),
        "the loop must stop at exactly max_revision_rounds: {}",
        run.envelope
    );
    assert_eq!(
        run.envelope["verdict"], "RevisionNeeded",
        "a run that ran out of rounds has not been approved: {}",
        run.envelope
    );

    // Exactly `MAX_ROUNDS` rounds of work. The counter is 0-based inside the loop and
    // incremented after each round that asked for another, so `max_revision_rounds =
    // 2` buys rounds 0 and 1 and then exits with `round == 2` — two rounds spent,
    // which is what `revision_rounds` reports.
    //
    // The count is read off the artifacts the run persisted, not off a counter here,
    // so a loop that spun would be caught by the artifact that should not exist.
    let names = artifact_names(&run.task_dir);
    for round in 0..MAX_ROUNDS as usize {
        let suffix = if round == 0 {
            String::new()
        } else {
            format!("-{}", round + 1)
        };
        assert!(
            names.contains(&format!("coder{suffix}.json")),
            "round {round} must have a Coder artifact: {names:?}"
        );
        assert!(
            names.contains(&format!("tester{suffix}.json")),
            "round {round} must have a Tester artifact: {names:?}"
        );
        assert!(
            names.contains(&format!("reviewer{suffix}.json")),
            "round {round} must have a Reviewer artifact: {names:?}"
        );
    }
    for past in ["coder", "tester", "reviewer"] {
        assert!(
            !names.contains(&format!("{past}-{}.json", MAX_ROUNDS + 1)),
            "the loop ran past its bound into another {past} pass: {names:?}"
        );
    }

    // Every round's Tester really did report red, so this is a bound and not a run
    // that converged early on an accidentally-green Tester.
    for round in 0..MAX_ROUNDS as usize {
        let suffix = if round == 0 {
            String::new()
        } else {
            format!("-{}", round + 1)
        };
        let report = read_artifact(&run.task_dir, &format!("tester{suffix}.json"));
        assert_eq!(
            report["test_results"]["failed"],
            serde_json::json!(1),
            "round {round} must still be red: {report}"
        );
    }
    std::mem::forget(repo);
}

/// The seam: the same failing-then-passing run, driven over `niki serve`, with the
/// retry visible in the emitted notifications.
///
/// `attempt` is the engine's own per-role pass counter, computed by the adapter from
/// the `StageStart` events `execute_pipeline` emits (`AdapterSink::attempts`). Reading
/// `attempt >= 2` off the wire is reading engine state; it is not this test counting
/// its own frames, which is why the assertion is on the field rather than on how many
/// `stage.start` frames arrived.
#[test]
fn the_seam_carries_the_retry_as_a_visible_marker() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    fails_then_passes_script(&script);
    std::fs::write(project.join("niki.toml"), chain_toml(&script, None)).expect("write niki.toml");

    let mut child = Command::new(niki_bin())
        .args([
            "serve",
            "--project",
            project.to_str().expect("utf-8 project path"),
            "--backend",
            "worktree",
            "--bare",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("niki serve starts");
    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let mut guard = ChildGuard(Some(child));
    let stderr = guard
        .0
        .as_mut()
        .expect("child alive")
        .stderr
        .take()
        .expect("stderr piped");

    let (tx, rx) = mpsc::channel::<serde_json::Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match serde_json::from_str(&line) {
                Ok(v) => {
                    if tx.send(v).is_err() {
                        break;
                    }
                }
                Err(e) => panic!("every line on stdout must be one JSON object: {e}\n{line}"),
            }
        }
    });
    // Nothing asserts on stderr, but an unread pipe fills and blocks the server.
    std::thread::spawn(move || {
        let _ = BufReader::new(stderr).lines().map_while(Result::ok).count();
    });

    let canonical = project.canonicalize().expect("resolves");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"session.load","params":{{"session_id":null,"project_path":{:?}}}}}"#,
        canonical.to_string_lossy()
    )
    .expect("write session.load");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"trace_id":"t1","method":"turn.start","params":{{"prompt":"fix the pagination off-by-one","permission_mode":"manual"}}}}"#
    )
    .expect("write turn.start");
    stdin.flush().expect("flush");

    let mut frames: Vec<serde_json::Value> = Vec::new();
    let mut final_verdict: Option<serde_json::Value> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(240);
    while std::time::Instant::now() < deadline && final_verdict.is_none() {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(f) => {
                let is_final = f.get("method").and_then(|m| m.as_str()) == Some("final");
                if is_final {
                    final_verdict = f.get("params").cloned();
                }
                frames.push(f);
            }
            Err(e) => panic!("no frame from niki serve within 60s: {e}"),
        }
    }
    let Some(verdict) = final_verdict else {
        guard.kill();
        panic!("the turn never sent `final`")
    };

    let notifications: Vec<&serde_json::Value> = frames
        .iter()
        .filter(|f| f.get("method").is_some())
        .collect();

    // The retry marker, read off the wire.
    let coder_attempts: Vec<u64> = notifications
        .iter()
        .filter(|f| f["method"] == "stage.start" && f["params"]["role"] == "coder")
        .map(|f| {
            f["params"]["attempt"]
                .as_u64()
                .unwrap_or_else(|| panic!("stage.start must carry an attempt: {f}"))
        })
        .collect();
    assert!(
        coder_attempts.contains(&2),
        "the second Coder pass must be announced with attempt > 1; the wire carried {coder_attempts:?}"
    );
    assert_eq!(
        coder_attempts,
        vec![1, 2],
        "exactly two Coder passes, numbered from 1: {coder_attempts:?}"
    );

    // The rest of the chain turns with it, which is what makes this a *loop* rather
    // than a Coder that happened to be called twice.
    for role in ["tester", "reviewer"] {
        let attempts: Vec<u64> = notifications
            .iter()
            .filter(|f| f["method"] == "stage.start" && f["params"]["role"] == role)
            .map(|f| f["params"]["attempt"].as_u64().unwrap_or(0))
            .collect();
        assert_eq!(
            attempts,
            vec![1, 2],
            "the {role} must be re-run in the revision round: {attempts:?}"
        );
    }

    // `stage.done` carries the protocol's own retry field, and it is present on the
    // wire even when the stage needed no LLM-level retry — which is the point: a
    // consumer can read it without knowing the value in advance.
    let coder_done: Vec<&serde_json::Value> = notifications
        .iter()
        .filter(|f| f["method"] == "stage.done" && f["params"]["role"] == "coder")
        .copied()
        .collect();
    assert_eq!(
        coder_done.len(),
        2,
        "both Coder passes must report completion: {coder_done:?}"
    );
    for done in &coder_done {
        assert!(
            done["params"]["retry_count"].is_number(),
            "stage.done must carry a machine-readable retry_count: {done}"
        );
    }

    // The revision itself is announced, so the shell is told something happened before
    // any verdict exists.
    assert!(
        notifications.iter().any(|f| f["method"] == "notice"
            && f["params"]["text"]
                .as_str()
                .is_some_and(|t| t.contains("revision round"))),
        "the revision must be announced on the wire: {:?}",
        notifications
            .iter()
            .map(|f| f["method"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>()
    );

    assert!(verdict["error"].is_null(), "{verdict}");
    assert!(verdict["verdict"].is_string(), "{verdict}");

    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":3,"trace_id":"t1","method":"shutdown","params":{{"user_initiated":true}}}}"#
    )
    .expect("write shutdown");
    stdin.flush().expect("flush");
    drop(stdin);
    guard.kill();
    std::mem::forget(repo);
}

/// Kills a child however the test leaves — including on a panic, where nothing else
/// runs and a server holding an open stdin pipe would outlive the test binary.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn kill(&mut self) {
        if let Some(mut child) = self.0.take() {
            // Already-exited is the normal case on the shutdown path; killing a dead
            // pid is not an error worth failing a green test over.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill();
    }
}
