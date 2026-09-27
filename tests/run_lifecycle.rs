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
        .write(&path)
}

fn minimal_mock_toml(script_path: &std::path::Path, test_command: Option<&str>) -> String {
    let test_cmd_line = if let Some(cmd) = test_command {
        format!("test_command = \"{cmd}\"\n")
    } else {
        String::new()
    };
    format!(
        r#"[docker]
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
{}
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
        test_cmd_line
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
    assert_eq!(json_val["verdict"], "Approved");

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

    // Run handle: it should succeed in running the pipeline, but record the verification failure
    let res = niki::cli::run::handle(&args).await;
    assert!(
        res.is_ok(),
        "handle returns Ok even when suite fails (recording failure in record)"
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
