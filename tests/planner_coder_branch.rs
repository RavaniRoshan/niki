//! One Planner → Coder pass, headless, end to end, and the branch it leaves behind.
//!
//! `tests/run_lifecycle.rs` covers the full chain, the blocked-branch paths and the exit-code
//! contract. This file covers the smallest unit of work the product sells: one Planner spec,
//! one Coder edit, and a `niki/<id>` branch that actually carries that edit.
//!
//! The mock is the in-process `mock` provider with a scripted response file, not
//! `tests/integration/mock_llm.py`. The python server is the right tool when a test needs the
//! HTTP path or the tool-loop story; this one needs determinism and no port, and the provider
//! gives both without a background process that has to be reaped.
//!
//! The branch is asserted by reading the **committed blob** with `git show <branch>:<path>`.
//! A patch sidecar file describes a diff; only the blob is the branch.

mod common;

use std::path::Path;
use std::process::Command;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::{MockScriptBuilder, code_diff_json, task_spec_json};

/// What the Coder is scripted to do, and what the branch must therefore contain.
const BEFORE: &str = "let end = start + size - 1;";
const AFTER: &str = "let end = start + size;";
const TARGET: &str = "src/list.rs";

fn wrap_json(text: &str) -> String {
    format!("```json\n{text}\n```")
}

/// One Planner spec and one Coder edit. No Tester, no Reviewer: those are `run_lifecycle`'s
/// half, and a fixture that answered for them here would only add noise to what this asserts.
fn planner_coder_script(path: &Path) -> std::path::PathBuf {
    MockScriptBuilder::new()
        .add_response("mock-planner", &wrap_json(&task_spec_json()), 80, 120)
        .add_response(
            "mock-coder",
            &wrap_json(&code_diff_json(BEFORE, AFTER, TARGET)),
            200,
            80,
        )
        .write(&path.to_path_buf())
}

/// The smallest pipeline the product will run: the `SingleAgent` fast path is exactly
/// "Planner + solo Coder", with the Tester, Reviewer and Red collapsed away.
///
/// The topology is pinned rather than left to `auto`, because the deliverable under test *is*
/// "only the Planner and the Coder ran" — an auto-selection that happened to land on the other
/// side of the crossover would fail this file for a reason that has nothing to do with the
/// branch.
fn single_pass_toml(script_path: &Path) -> String {
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

fn niki_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

/// The branch exists, the commit is on it, and the committed file is the one the Coder wrote.
#[test]
fn a_planner_coder_pass_leaves_a_reviewable_branch() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    planner_coder_script(&script);
    std::fs::write(project.join("niki.toml"), single_pass_toml(&script)).expect("write niki.toml");

    // Precondition: the bug is committed, so a later `git show` that does not contain the fix
    // is a real absence rather than an artefact of reading an empty file.
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

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must be one JSON envelope: {e}\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert_eq!(envelope["status"], "completed", "{envelope}");

    let branch = envelope["branch"]
        .as_str()
        .unwrap_or_else(|| panic!("a completed run must report a branch: {envelope}"))
        .to_string();
    assert!(
        branch.starts_with("niki/"),
        "the deliverable is a `niki/<id>` branch, got {branch:?}"
    );

    // The branch is a ref, not a name: git has to resolve it.
    // `git rev-parse --verify` prints the resolved oid, so this reads captured output: a
    // `.status()` here would put a bare sha on the test harness's stdout.
    let resolved = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(&project)
        .output()
        .expect("git rev-parse runs");
    assert!(
        resolved.status.success(),
        "refs/heads/{branch} does not exist on disk: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );

    // The commit, read off the branch.
    let stat = git(&project, &["show", "--stat", "--oneline", &branch]);
    assert!(
        stat.contains(TARGET),
        "{branch} must carry a commit touching {TARGET}:\n{stat}"
    );

    // The change, read off the committed blob. The worktree backend deliberately leaves the
    // working tree edited so the user can review in place, so a working-tree read would pass
    // even if nothing were committed.
    let on_branch = git(&project, &["show", &format!("{branch}:{TARGET}")]);
    assert!(
        on_branch.contains(AFTER),
        "the Coder's edit must be committed on {branch}, but the blob reads:\n{on_branch}"
    );
    assert!(
        !on_branch.contains(BEFORE),
        "{branch} still carries the pre-change line:\n{on_branch}"
    );
}

/// And it really was one Planner and one Coder. A fast path that quietly ran the full chain
/// would satisfy every assertion above while being a different product.
#[test]
fn the_pass_meters_a_planner_and_a_coder_and_nothing_else() {
    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    planner_coder_script(&script);
    std::fs::write(project.join("niki.toml"), single_pass_toml(&script)).expect("write niki.toml");

    let out = Command::new(niki_bin())
        .args([
            "run",
            "--backend",
            "worktree",
            "--bare",
            "--quiet",
            "--project",
            project.to_str().expect("utf-8 project path"),
            "fix the pagination off-by-one",
        ])
        .output()
        .expect("niki runs");
    assert!(
        out.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let tasks_dir = project.join(".niki").join("tasks");
    let task_dir = std::fs::read_dir(&tasks_dir)
        .expect("a task dir")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("exactly one task dir");
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(task_dir.join("task.json")).expect("task.json is written"),
    )
    .expect("task.json is JSON");

    let roles: Vec<String> = record
        .get("agent_metrics")
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("role").and_then(|r| r.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    assert!(
        roles.iter().any(|r| r == "planner"),
        "the Planner must be metered: {roles:?} — {record:?}"
    );
    assert!(
        roles.iter().any(|r| r == "coder"),
        "the Coder must be metered: {roles:?} — {record:?}"
    );
    for absent in ["reviewer", "tester", "red"] {
        assert!(
            !roles.iter().any(|r| r == absent),
            "`singleagent` is Planner + solo Coder, so the {absent} must not have run: {roles:?}"
        );
    }
}

/// The same work, driven over the protocol instead of the CLI, so the two surfaces are proved
/// to reach the same engine.
///
/// No branch is asserted here, and the absence is deliberate: `turn.start` runs the pipeline
/// and streams it. Delivery — the `niki/<id>` branch — is `niki run`'s job, and the test above
/// is the one that holds it to that. What is asserted here is the streaming contract.
#[test]
fn the_same_pass_streams_over_the_protocol() {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    let repo = create_fixture_repo();
    let project = repo.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    planner_coder_script(&script);
    std::fs::write(project.join("niki.toml"), single_pass_toml(&script)).expect("write niki.toml");

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
    // Kill on every exit path: a protocol test that leaks a server leaks its pipes too.
    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    let (tx, rx) = mpsc::channel::<serde_json::Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let parsed: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    panic!("every line on stdout must be one JSON object: {e}\n{line}")
                }
            };
            if tx.send(parsed).is_err() {
                break;
            }
        }
    });
    // stderr is drained too; nothing asserts on it, but an unread pipe fills and blocks.
    let mut guard = ChildGuard(Some(child));
    let stderr = guard
        .0
        .as_mut()
        .expect("child alive")
        .stderr
        .take()
        .expect("stderr piped");
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

    let mut methods: Vec<String> = Vec::new();
    let mut final_verdict: Option<serde_json::Value> = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(240);
    while std::time::Instant::now() < deadline && final_verdict.is_none() {
        let frame = match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(f) => f,
            Err(e) => panic!("no frame from niki serve within 60s: {e}"),
        };
        if let Some(method) = frame.get("method").and_then(|m| m.as_str()) {
            methods.push(method.to_string());
            if method == "final" {
                final_verdict = frame.get("params").cloned();
            }
        }
    }
    let Some(verdict) = final_verdict else {
        guard.kill();
        panic!("the turn never sent `final`. Notifications seen: {methods:?}")
    };

    assert!(
        methods.iter().any(|m| m == "stage.start"),
        "a run that did no work reports no stage rows: {methods:?}"
    );
    assert!(
        methods.iter().any(|m| m == "stage.done"),
        "a stage that started and vanished reports no completion: {methods:?}"
    );
    assert!(
        methods.contains(&"turn.started".to_string()),
        "the turn must announce itself before it streams: {methods:?}"
    );
    assert!(
        verdict["error"].is_null(),
        "this run succeeds; a `final` that carries an error is not what happened: {verdict}"
    );
    assert!(
        verdict["verdict"].is_string(),
        "a finished run must say what happened: {verdict}"
    );

    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":3,"trace_id":"t1","method":"shutdown","params":{{"user_initiated":true}}}}"#
    )
    .expect("write shutdown");
    stdin.flush().expect("flush");
    drop(stdin);
    guard.kill();
}

/// Kills a child process however the test leaves — including on a panic, where nothing else
/// runs and a server holding an open stdin pipe would outlive the test binary.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn kill(&mut self) {
        if let Some(mut child) = self.0.take() {
            // Already-exited is the normal case on the shutdown path; killing a dead pid is
            // not an error worth failing a green test over.
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
