//! Phase 5.6: run-lifecycle failure contract.
//!
//! - A failed run creates no `niki/*` branch and leaves `task.json` Failed
//!   with the error (proves the failure contract end to end through `handle`).
//! - Conflict markers abort the branch instead of committing.

mod common;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::MockScriptBuilder;

/// A mock script whose planner always fails fatally (no retries can save it).
fn failing_planner_script(path: &std::path::Path) -> PathBuf {
    let path = path.to_path_buf();
    MockScriptBuilder::new()
        .add_error("mock-planner", "fatal", "planner exploded")
        .write(&path)
}

use std::path::PathBuf;

fn run_args(project: PathBuf) -> niki::cli::run::RunArgs {
    niki::cli::run::RunArgs {
        description: "do thing".to_string(),
        project: Some(project),
        branch: None,
        max_rounds: None,
        max_steps: None,
        max_usd: None,
        max_wallclock_secs: None,
        planner_model: None,
        coder_model: None,
        tester_model: None,
        reviewer_model: None,
        backend: Some(niki::cli::run::BackendArg::Worktree),
        dry_run: false,
        quiet: true,
        tui: false,
        force: false,
        plan: None,
        output_format: niki::cli::run::OutputFormat::Text,
        bare: true,
        permission_mode: None,
        otel_endpoint: None,
    }
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_run_creates_no_branch_and_marks_task_failed() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    failing_planner_script(&script_path);

    // Minimal niki.toml: worktree backend + failing mock planner.
    std::fs::write(
        project.join("niki.toml"),
        format!(
            r#"[docker]
backend = "worktree"

[providers.mock]
base_url = "{}"
default_model = "mock-planner"

[agents.planner]
provider = "mock"
model = "mock-planner"
"#,
            script_path.display()
        ),
    )
    .unwrap();

    let err = niki::cli::run::handle(&run_args(project.clone()))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("planner exploded"),
        "pipeline error surfaces, got: {err:?}"
    );

    // Exactly one task dir, marked Failed with the error.
    let tasks_dir = project.join(".niki").join("tasks");
    let entries: Vec<_> = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 1, "one task record expected");
    let task_json =
        std::fs::read_to_string(entries[0].path().join("task.json")).expect("task.json written");
    let record: serde_json::Value = serde_json::from_str(&task_json).unwrap();
    let status = record
        .get("status")
        .cloned()
        .unwrap_or_default()
        .to_string();
    assert!(
        status.contains("planner exploded"),
        "task.json must record the failure, got: {status}"
    );

    // No niki/* branch was created for the failed run.
    let branches = git(&project, &["branch", "--list", "niki/*"]);
    assert!(
        branches.trim().is_empty(),
        "failed run must not create a branch, got: {branches}"
    );
}

#[test]
fn conflict_markers_block_branch_creation() {
    // Phase 5.6: markers in an applied file abort instead of committing.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(
        repo.join("b.rs"),
        "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> branch\n",
    )
    .unwrap();
    let diff =
        "diff --git a/a.rs b/a.rs\n+++ b/a.rs\n@@\ndiff --git a/b.rs b/b.rs\n+++ b/b.rs\n@@\n";
    let err = niki::output::git::ensure_no_conflict_markers(repo, diff).unwrap_err();
    assert!(err.to_string().contains("b.rs"), "{err:?}");
    assert!(!err.to_string().contains("a.rs"), "{err:?}");
}

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

/// A run that genuinely succeeds, all the way to an independent approval.
///
/// The reviewer response used to be missing here, which meant the pipeline
/// finished with no reviewer artifact at all — and the tests then asserted
/// `verdict == "Approved"` on a pass that nothing had ever granted. The suite
/// was pinning the fabricated pass in place.
fn successful_script(path: &std::path::Path) -> PathBuf {
    let path = path.to_path_buf();
    MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &wrap_json(&common::mock_llm::task_spec_json()),
            100,
            100,
        )
        .add_response(
            "mock-coder",
            &wrap_json(&common::mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            200,
            100,
        )
        .add_response(
            "mock-tester",
            &wrap_json(&common::mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap_json(&common::mock_llm::review_verdict_approved_json()),
            150,
            50,
        )
        .write(&path)
}

/// The same run with the reviewer stage starved of a response, so nothing
/// evaluates the work. Used to prove the verdict cannot be a bare pass.
fn unreviewed_script(path: &std::path::Path) -> PathBuf {
    let path = path.to_path_buf();
    MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &wrap_json(&common::mock_llm::task_spec_json()),
            100,
            100,
        )
        .add_response(
            "mock-coder",
            &wrap_json(&common::mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            200,
            100,
        )
        .write(&path)
}

/// The same fixture with the multi-agent chain pinned.
///
/// For tests that genuinely mean "an independent Reviewer signed off" — they
/// need the Reviewer's response in the mock script, which the solo fixture does
/// not carry.
fn multiagent_mock_toml(script_path: &std::path::Path, test_command: Option<&str>) -> String {
    minimal_mock_toml(script_path, test_command)
        .replace("topology = \"singleagent\"", "topology = \"multiagent\"")
}

fn minimal_mock_toml(script_path: &std::path::Path, test_command: Option<&str>) -> String {
    // `test_command` is a per-agent field, so the placeholder has to expand
    // *inside* the `[agents.tester]` table below. It used to be a bare `{}`
    // one line further down, which worked by accident and would have silently
    // attached the command to whichever table came next if this string were
    // ever reordered.
    let test_cmd_line = if let Some(cmd) = test_command {
        format!("test_command = \"{cmd}\"\n")
    } else {
        String::new()
    };
    // The topology is pinned rather than left to the default.
    //
    // `Auto` no longer collapses a low-complexity task to the fast path: with no
    // measured model capability it keeps the multi-agent chain, because that is
    // the side of the trade worth taking for a model of unknown strength (see
    // `tests/topology_heuristic.rs`). A test that means "the solo path" must say
    // so, rather than relying on which side of the crossover a default lands on
    // — and a test that means "the multi-agent path" must provide the Tester
    // and Reviewer responses it now needs.
    format!(
        r#"[pipeline]
topology = "singleagent"

[docker]
backend = "worktree"
extra_packages = []

[red_blue]
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
{test_cmd_line}
[agents.reviewer]
provider = "mock"
model = "mock-reviewer"

[agents.red]
provider = "mock"
model = "mock-red"

[agents.synthesizer]
provider = "mock"
model = "mock-synthesizer"

[agents.security_auditor]
provider = "mock"
model = "mock-security_auditor"
"#,
        script_path.display(),
        test_cmd_line = test_cmd_line
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn output_envelope_json_mode_pure_stdout_and_single_patch() {
    use std::io::Write;

    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);

    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args([
            "run",
            "--backend",
            "worktree",
            "--output-format",
            "json",
            "--tui",
            "--bare",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki executable must run");

    assert!(
        output.status.success(),
        "run must succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout_str = String::from_utf8(output.stdout.clone()).expect("stdout must be valid UTF-8");

    // Proves stdout is pure JSON without TUI noise
    let json_val: serde_json::Value =
        serde_json::from_str(&stdout_str).expect("stdout must be valid JSON envelope");

    assert_eq!(json_val["status"], "completed");
    assert!(json_val["branch"].as_str().unwrap().starts_with("niki/"));
    assert!(json_val["task_id"].is_string());
    assert_eq!(json_val["bare"], true);
    // The topology heuristic changed: with no measured model capability, `Auto`
    // now keeps the multi-agent chain rather than collapsing a low-complexity
    // task to the fast path. The evidence behind that is in
    // `tests/topology_heuristic.rs` — a multi-agent pipeline is worth about +22
    // points to a weak model and costs about 5 to a strong one, and the old
    // heuristic handed the structure-hungry model the opposite of the structure
    // it needed.
    //
    // So this run now reaches the Reviewer, and the honest-reporting property
    // this test exists for has to be pinned with the topology *pinned too*,
    // rather than by relying on which side of the crossover a default happens
    // to land on.
    // The invariant, not a field name: an "Approved" verdict is only ever
    // allowed to appear when something independent actually reviewed the run.
    // Which side of the topology crossover this run lands on is not the
    // property — the pairing is.
    let reviewed = json_val["independently_reviewed"].as_bool();
    if json_val["verdict"] == "Approved" {
        assert_eq!(
            reviewed,
            Some(true),
            "an Approved verdict with no independent review is the fabricated pass this \
             suite exists to catch: {json_val}"
        );
    } else {
        assert_ne!(reviewed, Some(true), "{json_val}");
        // The fast path: nothing independent ran, and the envelope must not
        // pretend otherwise. It used to report `verdict: "Approved"` here, and
        // this test asserted that, so the fabricated pass was pinned in place by
        // the very suite meant to catch it.
        assert_eq!(json_val["outcome"]["outcome"], "self_verified");
        assert_ne!(json_val["verdict"], "Approved");
    }
    if reviewed != Some(true) {
        // The self-verification must carry a reason, so a user reading only the
        // envelope can tell what kind of not-a-review this was.
        assert!(
            json_val["outcome"]["note"]
                .as_str()
                .is_some_and(|n| !n.trim().is_empty()),
            "self_verified must explain itself: {json_val}"
        );
    }

    // Acceptance requirement: JSON envelope parses with `python3 -m json.tool`
    let mut py_child = std::process::Command::new("python3")
        .args(["-m", "json.tool"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("python3 runs");
    py_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&output.stdout)
        .unwrap();
    let py_res = py_child.wait_with_output().unwrap();
    assert!(
        py_res.status.success(),
        "python3 -m json.tool must parse stdout envelope cleanly"
    );

    // Single-writer changes.patch verification
    let task_id = json_val["task_id"].as_str().unwrap();
    let task_dir = project.join(".niki").join("tasks").join(task_id);
    let patch_file = task_dir.join("changes.patch");
    assert!(patch_file.is_file(), "changes.patch exists");
    let patch_text = std::fs::read_to_string(&patch_file).unwrap();
    assert!(
        patch_text.contains("diff --git"),
        "changes.patch must contain unified diff format"
    );
    assert!(
        patch_text.contains("src/list.rs"),
        "changes.patch must contain modified file"
    );
}

/// The reverse of the test above: strip the reviewer response and the run must
/// stop claiming an independent pass.
///
/// This is the test that should have existed before `verdict` defaulted to
/// `Approved`. The failure it guards against is not a crash — it is a
/// *quietly wrong answer* that a CI script would gate on, and that reads as
/// success in every log line it produces.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_with_no_reviewer_never_reports_an_approved_verdict() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    unreviewed_script(&script_path);

    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args([
            "run",
            "--backend",
            "worktree",
            "--output-format",
            "json",
            "--tui",
            "--bare",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki executable must run");

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let json_val: serde_json::Value =
        serde_json::from_str(&stdout_str).expect("stdout must be valid JSON envelope");

    assert_eq!(
        json_val["independently_reviewed"], false,
        "nothing reviewed this run, so it must not claim to have been reviewed"
    );
    assert_ne!(
        json_val["verdict"], "Approved",
        "a run that no reviewer examined must not report an approval: {}",
        json_val["outcome"]
    );
    // Whatever it does claim, the outcome has to be one of the honest
    // "nobody looked" states — not a Reviewed with an invented reviewer.
    let outcome = json_val["outcome"]["outcome"].as_str().unwrap_or("");
    assert!(
        matches!(
            outcome,
            "not_evaluated" | "self_verified" | "revision_requested" | "failed"
        ),
        "unexpected outcome {outcome:?} in {json_val}"
    );
    if outcome == "reviewed" {
        let by = json_val["outcome"]["by"].as_str().unwrap_or("");
        assert!(
            !by.is_empty(),
            "a reviewed outcome must name who reviewed it"
        );
    }
}

/// The other half of the pair: force the full multi-agent chain, let a real
/// Reviewer approve, and the run must now say so — and say *who*.
///
/// Without this, "never fabricates a pass" would be a goal the suite could
/// satisfy by making every run a failure. `Approved` has to still be
/// reachable, and reachable only with a reviewer attached to it.
#[tokio::test(flavor = "multi_thread")]
async fn an_independently_reviewed_run_reports_approved_and_names_its_reviewer() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);

    // `multiagent_mock_toml` already carries the pinned topology. Appending a
    // second `[pipeline]` table on top of it is a duplicate key, and the config
    // fails to parse — which is what made this test fail after the topology
    // heuristic changed.
    std::fs::write(
        project.join("niki.toml"),
        multiagent_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args([
            "run",
            "--backend",
            "worktree",
            "--output-format",
            "json",
            "--tui",
            "--bare",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki executable must run");

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let json_val: serde_json::Value = serde_json::from_str(&stdout_str)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout_str}"));

    assert_eq!(
        json_val["independently_reviewed"], true,
        "the full chain runs a Reviewer, so the run must report one: {}",
        json_val["outcome"]
    );
    assert_eq!(json_val["outcome"]["outcome"], "reviewed");
    assert_eq!(json_val["outcome"]["verdict"], "approved");
    assert_eq!(json_val["verdict"], "Approved");

    // The reviewer must be a *separate* stage from the one that wrote the
    // code. A self-approval reported as independent review is the exact
    // failure this whole change exists to make impossible.
    let by = json_val["outcome"]["by"].as_str().unwrap_or("");
    assert!(
        !by.is_empty() && by != "solo-coder" && by != "coder",
        "an independent review must not be credited to the coder: {by:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn failing_test_suite_creates_no_branch_and_marks_task_failed() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);

    // Configure test_command to fail ("false" exits 1)
    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("false")),
    )
    .unwrap();

    let mut args = run_args(project.clone());
    args.bare = true;

    // Run handle: the pipeline runs to completion, but a failed suite blocks the
    // branch, so the run delivers nothing and must report that.
    //
    // This asserted `res.is_ok()` for years, and that assertion *was the bug*.
    // A run whose test suite failed created no branch, recorded Failed, told the
    // user on stderr — and exited 0. A CI gate reading the exit code passed on a
    // run that shipped nothing.
    let res = niki::cli::run::handle(&args).await;
    assert!(
        res.is_err(),
        "a failed test suite blocks the branch, so the run delivers nothing and must \
         return Err. Got Ok — the exit code is 0 and a CI gate passes on a run that \
         produced no branch. {res:?}"
    );

    // No niki/* branch was created because verification blocked it
    let branches = git(&project, &["branch", "--list", "niki/*"]);
    assert!(
        branches.trim().is_empty(),
        "failed test suite must block branch creation, got: {branches}"
    );

    // Verify task.json has Failed status noting verification gate failure
    let tasks_dir = project.join(".niki").join("tasks");
    let entries: Vec<_> = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 1);
    let task_json =
        std::fs::read_to_string(entries[0].path().join("task.json")).expect("task.json written");
    let record: serde_json::Value = serde_json::from_str(&task_json).unwrap();
    let status_str = record
        .get("status")
        .cloned()
        .unwrap_or_default()
        .to_string();
    // The original assertion accepted "BLOCKED by verification gate" OR any
    // string containing "failed", so nearly any failure text satisfied it.
    // What the user actually needs is the gate that blocked, the command that
    // failed, and how to override it — so assert all three.
    assert!(
        status_str.contains("Branch blocked"),
        "task.json must record that a gate blocked the branch, got: {status_str}"
    );
    assert!(
        status_str.contains("`false`"),
        "task.json must name the command that failed, got: {status_str}"
    );
    assert!(
        status_str.contains("exit 1"),
        "task.json must report the failing exit code, got: {status_str}"
    );
    assert!(
        status_str.contains("--force"),
        "task.json must say how to proceed, got: {status_str}"
    );
    assert!(
        record.get("branch").is_none() || record.get("branch") == Some(&serde_json::Value::Null),
        "a blocked run must record no branch, got {:?}",
        record.get("branch")
    );

    // Evidence (report.md, changes.patch) is still preserved
    assert!(entries[0].path().join("changes.patch").is_file());
    assert!(entries[0].path().join("report.md").is_file());
}

// --- INV-BRANCH-STATUS canary ------------------------------------------------
//
// The recorded status used to be derived from `branch_block_note` alone, so
// three separate paths fell through to `Completed { branch: Some(name) }` while
// no branch existed: an empty diff, a `create_branch_and_commit` error that
// was only warned about, and `--dry-run`. `niki status` and the JSON envelope
// then advertised a branch a user could not check out.
//
// The invariant is one-directional and absolute: Completed implies the branch
// exists on disk. It does not require the converse — a Forced run deliberately
// records a branch that was never verified.

fn read_task_record(project: &std::path::Path) -> serde_json::Value {
    let tasks_dir = project.join(".niki").join("tasks");
    let entries: Vec<_> = std::fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 1, "one task record expected");
    let task_json =
        std::fs::read_to_string(entries[0].path().join("task.json")).expect("task.json written");
    serde_json::from_str(&task_json).unwrap()
}

fn status_of(record: &serde_json::Value) -> String {
    record
        .get("status")
        .cloned()
        .unwrap_or_default()
        .to_string()
}

fn niki_branches(project: &std::path::Path) -> Vec<String> {
    git(project, &["branch", "--list", "niki/*"])
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn completed_status_requires_a_branch_that_exists_on_disk() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);
    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let res = niki::cli::run::handle(&run_args(project.clone())).await;
    assert!(res.is_ok(), "passing run should succeed, got: {res:?}");

    let record = read_task_record(&project);
    let status = status_of(&record);
    let recorded_branch = record.get("branch").cloned();

    if status.contains("Completed") {
        let name = recorded_branch
            .as_ref()
            .and_then(|b| b.as_str())
            .unwrap_or_else(|| panic!("Completed run must record a branch name"));
        let exists = std::process::Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ])
            .current_dir(&project)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(
            exists,
            "task.json reports Completed on branch `{name}`, but refs/heads/{name} does not \
             exist. A Completed status must always be backed by a real branch."
        );
    } else {
        // A non-Completed run must not advertise a branch either.
        assert!(
            recorded_branch.is_none() || recorded_branch == Some(serde_json::Value::Null),
            "a non-Completed run must record branch: null, got {recorded_branch:?}"
        );
    }
}

/// PL-2 canary. The SingleAgent fast path assigns `Verdict::Approved`
/// without a Reviewer, so `verdict_source` is the only thing distinguishing a
/// self-approval from an independent review. The invariant checker only sees
/// synthetic traces, so without this the field could be dropped and every
/// other test would still pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_completed_run_records_where_its_verdict_came_from() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);
    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    niki::cli::run::handle(&run_args(project.clone()))
        .await
        .expect("passing run");

    let record = read_task_record(&project);
    let source = record
        .get("verdict_source")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        !source.is_empty(),
        "task.json records a verdict with no source: a reader cannot tell an independent review \
         from the SingleAgent fast path approving its own patch. Got: {:?}",
        record.get("verdict_source")
    );
    // Whatever it says, it must not claim a review the run did not perform.
    if source.contains("solo-coder") {
        assert!(
            source.contains("no independent review"),
            "a self-approval must say so plainly: {source:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_never_records_completed() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);
    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let mut args = run_args(project.clone());
    args.dry_run = true;
    let _ = niki::cli::run::handle(&args).await;

    let record = read_task_record(&project);
    let status = status_of(&record);
    assert!(
        !status.contains("Completed"),
        "a dry run produces a plan, not a result. It must not record Completed, got: {status}"
    );
    assert!(
        niki_branches(&project).is_empty(),
        "a dry run must not create a branch"
    );
    assert!(
        record.get("branch").is_none() || record.get("branch") == Some(&serde_json::Value::Null),
        "a dry run must record branch: null, got {:?}",
        record.get("branch")
    );
}

// ── the exit-code contract ────────────────────────────────────────────────
//
// `niki run` recorded `TaskStatus::Failed` for a blocked branch, a failed
// commit, and an empty diff — and then returned `Ok(())`. `main.rs` propagates
// that straight out of `run()`, so the process exited 0. A CI step running
// `niki run "…"` passed on a run that produced nothing to review.
//
// These are process-level tests, not `handle()`-level ones, because the defect
// was never in the record: it was in the gap between the record and the exit
// code. Asserting on `handle()` alone would have passed against the bug — the
// function under test returned `Ok(())` the whole time.

/// The happy-path script, with the run stopped short of a branch by a failing
/// verification command. `test_command = "false"` makes the Tester's command
/// exit 1, `branch_block_note` is set, and the run delivers nothing.
///
/// Three other fixtures were tried for this and all reached a *different*
/// failure first, which is why this test used to be able to pass for the wrong
/// reason:
///
/// - editing a nonexistent path — the patch never applied, so the run died
///   during patch application;
/// - replacing text with itself — `schemas/code_diff.schema.json` rejects an
///   identical `search`/`replace` pair as a no-op;
/// - editing an untracked file — the worktree is checked out from git, so an
///   untracked file is not in it and the edit has nothing to match.
///
/// All three exit non-zero, and none of them reach the `record.status ==
/// Failed` branch this test is about. The blocked-by-verification path does,
/// and it is the case a user actually hits: the agent worked, the tests
/// failed, nothing was delivered.
fn blocked_by_verification_toml(script_path: &std::path::Path) -> String {
    minimal_mock_toml(script_path, Some("false"))
}

fn niki_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_produces_no_branch_exits_non_zero() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);
    std::fs::write(
        project.join("niki.toml"),
        blocked_by_verification_toml(&script_path),
    )
    .unwrap();

    let output = std::process::Command::new(niki_bin())
        .args([
            "run",
            "--backend",
            "worktree",
            "--bare",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki runs");

    assert!(
        niki_branches(&project).is_empty(),
        "precondition: a blocked run must not create a branch"
    );
    assert!(
        !output.status.success(),
        "a run that delivered no branch must exit non-zero, got {:?}. \
         A gate that cannot fail is not a gate.",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_lowercase().contains("blocked") || stderr.contains("no branch"),
        "the exit must say why the run was blocked, got: {stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_successful_run_still_exits_zero() {
    // The other half of the property. A fix that made every run exit non-zero
    // would satisfy the test above and be useless.
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);
    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let output = std::process::Command::new(niki_bin())
        .args([
            "run",
            "--backend",
            "worktree",
            "--bare",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki runs");

    assert!(
        output.status.success(),
        "a run that delivered a branch must exit 0, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !niki_branches(&project).is_empty(),
        "precondition: the happy path creates a branch"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_exits_zero_even_though_it_creates_no_branch() {
    // `--dry-run` is the one case where "no branch" is the expected result
    // rather than a failure, and the exit-code rule has to make that exception
    // explicitly or every dry run would look like a broken run.
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);
    std::fs::write(
        project.join("niki.toml"),
        minimal_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let output = std::process::Command::new(niki_bin())
        .args([
            "run",
            "--backend",
            "worktree",
            "--bare",
            "--dry-run",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki runs");

    assert!(
        output.status.success(),
        "a dry run must exit 0, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The product's central promise, asserted against the branch itself.
///
/// The README sells one thing: describe a change, get back a `niki/<id>` branch
/// that contains it. Until this test, nothing in `cargo test` checked that.
///
/// What the suite actually did was verify the branch by *name*
/// (`branch.starts_with("niki/")`, and a `rev-parse --verify` that the ref
/// exists) and verify the code change by **string-matching a sidecar file** —
/// `changes.patch` contains `diff --git` and contains `src/list.rs`. Those are
/// independent claims. A run could write a correct `changes.patch`, create a
/// correctly-named branch, commit nothing to it, and pass every assertion
/// above: the patch file is a description of a diff, not the diff, and nothing
/// correlated the two.
///
/// So this reads the committed blob with `git show <branch>:src/list.rs`. That
/// cannot be satisfied by the working tree — which the worktree backend
/// deliberately mutates, by design, so the user can review the change in place —
/// and it cannot be satisfied by a sidecar file. It is the branch.
///
/// It also pins the other two deliverables the README names in the same
/// sentence, which had no success-path assertion at all: `report.md` and
/// `artifacts/*.json`. The one test that mentioned `report.md` did so in a
/// *failure* path.
#[tokio::test(flavor = "multi_thread")]
async fn the_code_change_is_on_the_branch_not_only_in_a_sidecar_file() {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script_path = project.join(".niki-mock-script.json");
    successful_script(&script_path);

    std::fs::write(
        project.join("niki.toml"),
        multiagent_mock_toml(&script_path, Some("true")),
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_niki"))
        .args([
            "run",
            "--backend",
            "worktree",
            "--output-format",
            "json",
            "--bare",
            "--project",
            project.to_str().unwrap(),
            "fix pagination",
        ])
        .output()
        .expect("niki executable must run");

    assert!(
        output.status.success(),
        "run must succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout must be a JSON envelope");
    assert_eq!(envelope["status"], "completed", "{envelope}");
    let branch = envelope["branch"]
        .as_str()
        .expect("a completed run must report a branch")
        .to_string();
    let task_id = envelope["task_id"].as_str().expect("task_id").to_string();

    // Topology is pinned to the multi-agent chain, not left to the default,
    // and the reason is the same one the JSON-envelope test above records: the
    // deliverable being asserted is `artifacts/*.json` for *every agent that
    // ran*, and under `singleagent` the Reviewer does not run — so its artifact
    // is legitimately absent, and asserting on it would be asserting a topology
    // rather than a contract. Pinning the chain makes "a reviewed run persists
    // the reviewer's decision" a statement this test is entitled to make.
    assert_eq!(
        envelope["independently_reviewed"],
        serde_json::json!(true),
        "this test asserts reviewer artifacts, so the Reviewer must have run: {envelope}"
    );

    // ── The change is on the branch, read from the committed blob ──────
    //
    // The fixture's Coder response replaces `let end = start + size - 1;`
    // with `let end = start + size;` in `src/list.rs`
    // (`successful_script`, above). `git show <ref>:<path>` prints the blob as
    // committed, so this is the branch and nothing else.
    let on_branch = git(&project, &["show", &format!("{branch}:src/list.rs")]);
    assert!(
        on_branch.contains("let end = start + size;"),
        "the Coder's change must be committed on {branch}, but `git show` returned:\n{on_branch}"
    );
    assert!(
        !on_branch.contains("size - 1"),
        "{branch} still carries the pre-change line:\n{on_branch}"
    );

    // The commit is real, not an empty tree that happens to be named right.
    let stat = git(&project, &["show", "--stat", "--oneline", &branch]);
    assert!(
        stat.contains("src/list.rs"),
        "{branch} must have a commit touching the changed file:\n{stat}"
    );

    // ── `changes.patch` and the branch must agree ──────────────────────
    //
    // The sidecar file is the thing the old suite trusted. It has to describe
    // *this* branch, not a diff nobody committed.
    let patch = std::fs::read_to_string(
        project
            .join(".niki")
            .join("tasks")
            .join(&task_id)
            .join("changes.patch"),
    )
    .expect("changes.patch is written");
    let branch_diff = git(&project, &["show", branch.as_str(), "--", "src/list.rs"]);
    for line in patch
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
    {
        let content = &line[1..];
        assert!(
            branch_diff.contains(content),
            "changes.patch claims +{content:?} but the branch commit does not contain it"
        );
    }

    // ── report.md, on a success path, for the first time ───────────────
    let report = project
        .join(".niki")
        .join("tasks")
        .join(&task_id)
        .join("report.md");
    let report_text = std::fs::read_to_string(&report)
        .unwrap_or_else(|e| panic!("report.md must exist after a successful run: {e}"));
    assert!(
        !report_text.trim().is_empty(),
        "report.md exists but is empty"
    );

    // ── artifacts/*.json, the third deliverable in the same sentence ────
    //
    // `tests/artifact_contracts.rs` validates artifact *schemas* in isolation.
    // Nothing asserted that a run actually *produced* any, so the README's
    // "per-agent JSON artifacts — the entire decision trail is inspectable"
    // had no test standing behind it.
    let artifacts_dir = project
        .join(".niki")
        .join("tasks")
        .join(&task_id)
        .join("artifacts");
    let artifacts: Vec<_> = std::fs::read_dir(&artifacts_dir)
        .unwrap_or_else(|e| panic!("artifacts/ must exist after a run: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        artifacts.iter().any(|f| f == "reviewer.json"),
        "a reviewed run must persist the reviewer's artifact, found: {artifacts:?}"
    );
    for name in &artifacts {
        if !name.ends_with(".json") {
            continue;
        }
        let body = std::fs::read_to_string(artifacts_dir.join(name)).expect("artifact readable");
        serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or_else(|e| panic!("artifact {name} must be valid JSON: {e}"));
    }
}
