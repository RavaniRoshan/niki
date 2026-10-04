//! C4 — the verifier runs the project's real test/build and records a verdict a
//! machine can check.
//!
//! ## Which verifier
//!
//! `niki verify` (`src/cli/verify.rs`) is the *visual* surface: it captures a screenshot
//! and appends a row to `verify-manifest.json`. It does not run a test suite, so it is
//! not the engine behind this deliverable. The engine that runs the real test/build is
//! `niki::agents::tester::run_tests`, which resolves the project's test command
//! (configured, or auto-detected from the project layout), executes it inside the
//! sandbox, and records the command, the exit code and the captured output as
//! `PipelineResult::test_execution` / `artifacts/test_execution.json`. That is the one
//! these tests drive, so this file adds no second verifier.
//!
//! ## What was extended, and why
//!
//! `run_tests` returned `Option<TestExecution>` and returned `None` when no test command
//! could be resolved. `None` is an absence, and every consumer had to invent its own
//! meaning for it: the report omitted the section, `render_verification_line` printed
//! nothing, `deliver.rs` read `!te.passed` as "the suite failed" once a record existed.
//! The dangerous half is the last one — with a record in hand, `passed == false` and
//! "nothing ran" are the same value.
//!
//! So `TestExecution` now carries `status: VerificationStatus` — `Unverified` /
//! `Passed` / `Failed` / `Errored` — and `run_tests` always returns a record. The
//! consumers that gate on it (`deliver.rs`, `report.rs`, `artifact_render.rs`,
//! `reflect.rs`, `run.rs`'s JSON envelope) read that field instead of `passed` alone.
//! `niki run`'s observable behaviour is unchanged: an unverified project still produces a
//! branch and still reports no pass, because there is no failure to act on — only the
//! absence of one.
//!
//! ## Why these are real runs
//!
//! The two crates below are built by `cargo` and their tests really execute; the verdict
//! is read off the returned `TestExecution` fields, never off stdout text.

mod common;

use std::path::Path;
use std::process::Command;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::{
    MockScriptBuilder, code_diff_json, review_verdict_approved_json, task_spec_json,
    test_report_json,
};
use niki::agents::tester::{TestExecution, VerificationStatus, run_tests};
use niki::artifacts::types::AgentRole;
use niki::config::types::default_tester_policy;
use niki::config::{DockerConfig, NikiConfig};
use niki::sandbox::Sandbox;
use niki::sandbox::worktree::WorktreeSandbox;

fn git(repo: &Path, args: &[&str]) {
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
}

/// A committed, dependency-free crate carrying one integration test.
///
/// The test body lands in `tests/`, which is where `cargo test` actually collects it
/// from. (Writing it into the crate root would compile a file cargo never runs, and the
/// "failing" fixture would then report green — a gate that passes because it looked
/// nowhere.)
fn commit_crate(repo: &Path, test_body: &str) {
    std::fs::create_dir_all(repo.join("src")).expect("src");
    std::fs::create_dir_all(repo.join("tests")).expect("tests");
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"niki-verify-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [lib]\npath = \"src/lib.rs\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        repo.join("src/lib.rs"),
        "pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n",
    )
    .expect("lib.rs");
    std::fs::write(repo.join("tests").join("addition.rs"), test_body).expect("tests/addition.rs");
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "fixture"]);
}

/// The verifier's own verdict, as a small readable summary. Everything asserted below
/// comes from the fields, never from a rendered line.
fn verdict_of(te: &TestExecution) -> (VerificationStatus, bool, i64, bool, bool) {
    (
        te.status,
        te.passed,
        te.exit_code,
        te.status.is_verified(),
        te.status.blocks_delivery(),
    )
}

/// Run the real verifier against `repo`, through the real worktree sandbox.
async fn verify(repo: &Path) -> TestExecution {
    let config = NikiConfig::default();
    // The receiver is dropped immediately, which is exactly what a headless
    // `niki run` leaves behind: `execute_pipeline` builds its `event_tx` from
    // `display.tui_tx()`, and with no sink attached that channel has no receiver.
    // The sandbox's `Ask` verdict then has nobody to answer it and — with
    // `fail_closed_headless` off by default — auto-approves. Keeping the receiver
    // alive instead would park every command on a permission prompt until it timed
    // out, which tests the prompt and not the verifier.
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    let sandbox = WorktreeSandbox::create(
        AgentRole::Tester,
        repo,
        &uuid::Uuid::new_v4(),
        &DockerConfig::default(),
        &config,
        default_tester_policy(),
        tx,
    )
    .await
    .expect("a worktree sandbox for the fixture repo");

    // `cargo test` without `--locked`: the fixture has no dependencies and no committed
    // lockfile, and `--locked` failing would be a claim about the harness rather than
    // about the verifier.
    let mut config = config;
    config.agents.tester.test_command = Some("cargo test 2>&1".to_string());

    let te = run_tests(&sandbox, &config, repo)
        .await
        .expect("run_tests always returns a record");
    sandbox.destroy().await.expect("teardown");
    te
}

/// A crate whose test passes: the verdict must be `Passed`, with the real exit code and
/// the captured output to back it.
#[tokio::test(flavor = "multi_thread")]
async fn a_green_suite_is_verified_as_passed() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    commit_crate(
        &project,
        "#[test]\nfn adds() { assert_eq!(niki_verify_fixture::add(2, 3), 5); }\n",
    );

    let te = verify(&project).await;
    let (status, passed, exit_code, is_verified, blocks) = verdict_of(&te);

    assert_eq!(
        (status, passed, exit_code, is_verified, blocks),
        (VerificationStatus::Passed, true, 0, true, false),
        "a suite that ran and exited 0 is `passed`, with evidence; stdout was:\n{}",
        te.stdout
    );
    // The evidence itself, so a verdict that is `Passed` without a captured run fails.
    assert!(
        te.stdout.contains("test result: ok"),
        "the verdict must be backed by the suite's own output:\n{}",
        te.stdout
    );
    assert!(
        te.command.contains("cargo test"),
        "the recorded command must be the one that ran: {:?}",
        te.command
    );
    assert!(
        te.note.is_none(),
        "a green suite has nothing to explain: {:?}",
        te.note
    );
}

/// The same crate with an assertion that does not hold: the verdict must be `Failed`,
/// with a non-zero exit code, and it must block delivery.
#[tokio::test(flavor = "multi_thread")]
async fn a_red_suite_is_verified_as_failed_and_blocks_delivery() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    // Same crate shape, a deliberately false assertion. `passing` is not read — the
    // body *is* the test — so this cannot drift from what actually runs.
    commit_crate(
        &project,
        "#[test]\nfn adds() { assert_eq!(niki_verify_fixture::add(2, 3), 6); }\n",
    );

    let te = verify(&project).await;
    let (status, passed, exit_code, is_verified, blocks) = verdict_of(&te);

    assert_eq!(
        (status, passed, is_verified, blocks),
        (VerificationStatus::Failed, false, true, true),
        "a suite that ran and failed is `failed` and stops delivery; stdout was:\n{}",
        te.stdout
    );
    assert_ne!(
        exit_code, 0,
        "a failing suite must carry a non-zero exit code, not a default"
    );
    assert!(
        te.stdout.contains("test result: FAILED"),
        "the verdict must be backed by the suite's own output:\n{}",
        te.stdout
    );
    assert!(te.note.is_some(), "a red suite must say why in the record");
}

/// The case the deliverable names: a project with no test command must produce an
/// explicit *unverified* outcome, never a pass.
///
/// Before this extension `run_tests` returned `None` here, and the distinction between
/// "no evidence" and "evidence of failure" lived in the shape of an `Option` that four
/// different call sites interpreted differently.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_with_no_test_command_is_unverified_and_never_a_pass() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    // The default fixture: `src/list.rs`, `src/main.rs`, `README.md`. No `Cargo.toml`,
    // no `package.json`, no `go.mod`, and no configured `test_command` — so the
    // auto-detector has nothing to resolve.
    for manifest in [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "setup.py",
        "requirements.txt",
        "go.mod",
        "Gemfile",
    ] {
        assert!(
            !project.join(manifest).exists(),
            "precondition: the unverified fixture must not contain {manifest}"
        );
    }

    let config = NikiConfig::default();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    let sandbox = WorktreeSandbox::create(
        AgentRole::Tester,
        &project,
        &uuid::Uuid::new_v4(),
        &DockerConfig::default(),
        &config,
        default_tester_policy(),
        tx,
    )
    .await
    .expect("worktree sandbox");
    let te = run_tests(&sandbox, &config, &project).await;
    sandbox.destroy().await.expect("teardown");

    let te = te.expect("run_tests always returns a record — absence is a status, not an Option");
    assert_eq!(
        te.status,
        VerificationStatus::Unverified,
        "nothing ran, so the verdict is `unverified`"
    );
    assert!(!te.passed, "`passed` must not be true: no command ran");
    assert!(
        !te.status.is_verified(),
        "`is_verified` must say no — this is the check a report or a gate reads"
    );
    assert!(
        !te.status.blocks_delivery(),
        "absence of evidence is not evidence of failure; a project with no manifest must \
         still get its branch"
    );
    assert!(
        te.command.is_empty(),
        "no command ran, so no command may be recorded as one that did: {:?}",
        te.command
    );
    assert!(
        te.stdout.is_empty() && te.stderr.is_empty(),
        "nothing ran, so nothing may be reported as captured output"
    );
    let note = te.note.as_deref().unwrap_or_default();
    assert!(
        note.contains("no test command"),
        "the record must say what was missing: {note:?}"
    );
}

/// The verdict survives being written and read back, which is the whole point of
/// "machine-checkable": `artifacts/test_execution.json` is what a consumer parses.
#[test]
fn the_verdict_survives_serialisation_into_the_artifact() {
    for (status, note) in [
        (
            VerificationStatus::Unverified,
            Some("no test command could be resolved"),
        ),
        (VerificationStatus::Passed, None),
        (
            VerificationStatus::Failed,
            Some("test suite reported failures (non-zero exit)"),
        ),
        (
            VerificationStatus::Errored,
            Some("test command could not be executed"),
        ),
    ] {
        let te = TestExecution {
            command: "cargo test 2>&1".to_string(),
            exit_code: 1,
            passed: matches!(status, VerificationStatus::Passed),
            status,
            note: note.map(str::to_string),
            ..Default::default()
        };
        let json = serde_json::to_value(&te).expect("serialise");
        assert_eq!(
            json["status"],
            serde_json::json!(status.as_str()),
            "the artifact must carry the verdict as a readable word: {json}"
        );
        let back: TestExecution = serde_json::from_value(json).expect("round-trip");
        assert_eq!(
            back.status, status,
            "{status:?} must survive the round trip"
        );
    }

    // An artifact written before the field existed reads as "no evidence" rather than
    // as a silent pass, which is what `#[serde(default)]` on the field is for.
    let legacy: TestExecution = serde_json::from_str(
        r#"{"command":"cargo test","exit_code":0,"passed":true,"stdout":"","stderr":"","truncated":false}"#,
    )
    .expect("a pre-existing artifact must still parse");
    assert_eq!(
        legacy.status,
        VerificationStatus::Unverified,
        "an old artifact has no verdict, and must not acquire one by defaulting to pass"
    );
}

/// End to end: `niki run` on a project with no manifest must publish `unverified` in its
/// JSON envelope and must still hand back a branch.
///
/// The report still omits its Verification section in this case, unchanged from before
/// the extension — what changed is that the machine-readable half now says so in a word
/// instead of leaving a reader to infer it from `tests_passed == null`.
#[test]
fn a_run_against_a_project_with_no_manifest_reports_unverified() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &format!("```json\n{}\n```", task_spec_json()),
            80,
            120,
        )
        .add_response(
            "mock-coder",
            &format!(
                "```json\n{}\n```",
                code_diff_json(
                    "let end = start + size - 1;",
                    "let end = start + size;",
                    "src/list.rs"
                )
            ),
            200,
            80,
        )
        .add_response(
            "mock-tester",
            &format!("```json\n{}\n```", test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &format!("```json\n{}\n```", review_verdict_approved_json()),
            150,
            50,
        )
        .write(&script);

    // `topology = "multiagent"` so the Reviewer actually runs: a run whose suite was
    // never verified must not be able to look approved on the strength of a stage that
    // did not happen.
    std::fs::write(
        project.join("niki.toml"),
        format!(
            r#"[pipeline]
topology = "multiagent"

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

[agents.reviewer]
provider = "mock"
model = "mock-reviewer"
"#,
            script.display()
        ),
    )
    .expect("write niki.toml");

    let out = Command::new(Path::new(env!("CARGO_BIN_EXE_niki")))
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
        "an unverified project still delivers a branch; got {:?}. stderr:\n{stderr}",
        out.status.code()
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must be one JSON envelope: {e}\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(envelope["verification"], "unverified", "{envelope}");
    assert!(
        envelope["tests_passed"].is_null(),
        "no suite ran, so `tests_passed` must be null rather than false: {envelope}"
    );
    // The Reviewer did approve; the point is that the two facts are distinguishable.
    assert_eq!(envelope["verdict"], "Approved", "{envelope}");

    // And the same verdict is on disk, in the artifact a consumer would parse.
    let task_id = envelope["task_id"].as_str().expect("task_id");
    let artifact = project
        .join(".niki")
        .join("tasks")
        .join(task_id)
        .join("artifacts")
        .join("test_execution.json");
    let recorded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&artifact).expect("artifact written"))
            .expect("artifact is JSON");
    assert_eq!(recorded["status"], "unverified", "{recorded}");
    assert_eq!(
        recorded["passed"], false,
        "`passed` is false because nothing ran — `status` is what says why: {recorded}"
    );
}
