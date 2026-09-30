//! A task typed in the TUI must produce a real branch.
//!
//! This is clause **C-J2** — the core promise, on the surface every user lands
//! on. `src/main.rs` routes a bare `niki` on a terminal straight into the chat,
//! and before this existed `src/cli/chat.rs` contained no reference to the
//! orchestrator at all: the front door could talk and never act. Every TUI page
//! that shows a pipeline rendered fabricated content because nothing ever
//! populated it, and `grep 'orchestrator' src/cli/chat.rs` returned nothing.
//!
//! The test drives the real pipeline through the real chat entry point, against
//! the in-process `mock` provider, and then reads the committed blob with
//! `git show` — the same assertion `run_lifecycle` uses for `niki run`, so the
//! two entry points are held to one standard rather than two.
//!
//! Note the shape: `/run <task>`. A plain message stays a conversation turn.
//! A chat that silently starts a four-agent run — spending money and writing to
//! git — on a message the user meant as a question is a chat that does things
//! nobody asked for, and this product's character is being honest about what
//! it did.

mod common;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use common::fixture_repo::create_fixture_repo;
use common::mock_llm::MockScriptBuilder;
use niki::config::NikiConfig;
use niki::display::tui::DisplayEvent;

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A repo, a mock script and a `niki.toml` wired to it.
fn fixture() -> (common::fixture_repo::FixtureRepo, std::path::PathBuf) {
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let script = project.join(".niki-mock-script.json");
    MockScriptBuilder::new()
        .add_response(
            "mock-planner",
            &wrap(&common::mock_llm::task_spec_json()),
            100,
            100,
        )
        .add_response(
            "mock-coder",
            &wrap(&common::mock_llm::code_diff_json(
                "let end = start + size - 1;",
                "let end = start + size;",
                "src/list.rs",
            )),
            200,
            100,
        )
        .add_response(
            "mock-tester",
            &wrap(&common::mock_llm::test_report_json()),
            100,
            60,
        )
        .add_response(
            "mock-reviewer",
            &wrap(&common::mock_llm::review_verdict_approved_json()),
            150,
            50,
        )
        .write(&script);
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
    .expect("niki.toml");
    (repo, project)
}

fn wrap(t: &str) -> String {
    format!(
        "```json
{t}
```"
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_typed_in_the_chat_produces_a_branch_carrying_the_change() {
    let (_repo, project) = fixture();
    let config = NikiConfig::load(&project).expect("the fixture config parses");

    let (tx, rx) = mpsc::channel::<DisplayEvent>();
    niki::cli::chat::run_task_to_sink(
        &tx,
        &config,
        &project,
        "fix pagination".to_string(),
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .expect("the run completes");

    // The stages must have reached the sink, or every TUI page that shows a
    // pipeline stays empty while claiming to show one.
    let events: Vec<DisplayEvent> = rx.try_iter().collect();
    let stage_names: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            DisplayEvent::StageStart { role, .. } => Some(format!("{role:?}")),
            _ => None,
        })
        .collect();
    assert!(
        stage_names.len() >= 3,
        "the chat must receive the pipeline's stages, got: {stage_names:?}"
    );

    // And the deliverable must exist, carrying the Coder's change.
    let branches = git(&project, &["branch", "--list", "niki/*"]);
    assert!(
        !branches.trim().is_empty(),
        "a task typed in the chat must produce a branch, got: {branches:?}"
    );
    let branch = branches
        .lines()
        .next()
        .unwrap()
        .trim()
        .trim_start_matches('*')
        .trim()
        .to_string();

    let on_branch = git(&project, &["show", &format!("{branch}:src/list.rs")]);
    assert!(
        on_branch.contains("let end = start + size;"),
        "the Coder's change must be committed on {branch}, but `git show` returned:\n{on_branch}"
    );
    assert!(
        !on_branch.contains("size - 1"),
        "the pre-change line must be gone:\n{on_branch}"
    );

    // The same four artefacts `niki run` produces.
    let task_dirs: Vec<String> = std::fs::read_dir(project.join(".niki/tasks"))
        .expect("task dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path().display().to_string())
        .collect();
    assert_eq!(task_dirs.len(), 1, "one task per /run");
    let dir = std::path::Path::new(&task_dirs[0]);
    for artefact in ["report.md", "changes.patch", "task.json", "artifacts"] {
        assert!(
            dir.join(artefact).exists(),
            "a chat run must leave {artefact} behind, as `niki run` does"
        );
    }
    let artifacts: Vec<String> = std::fs::read_dir(dir.join("artifacts"))
        .expect("artifacts dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        artifacts.iter().any(|a| a.starts_with("coder")),
        "the Coder's artifact must be recorded, got: {artifacts:?}"
    );
}
