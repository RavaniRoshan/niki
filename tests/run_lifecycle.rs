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
    assert!(
        status_str.contains("BLOCKED by verification gate") || status_str.contains("failed"),
        "task.json status must record verification failure, got: {status_str}"
    );

    // Evidence (report.md, changes.patch) is still preserved
    assert!(entries[0].path().join("changes.patch").is_file());
    assert!(entries[0].path().join("report.md").is_file());
}
