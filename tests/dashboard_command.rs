//! `niki dashboard` had no test at all.
//!
//! It is the only one of the 28 CLI commands whose name appears in no test, and
//! it is a command that reads a run's artifacts and renders them into an
//! **HTML file** — so "no test" means nobody has checked which task it picks,
//! whether it fails honestly when there is nothing to show, or that the values
//! it embeds are escaped.
//!
//! Driven through the real binary, against a real `.niki/tasks/` on disk, so
//! the task-selection rule is exercised rather than described.

use std::path::Path;
use std::process::Command;

fn niki() -> Command {
    Command::new(env!("CARGO_BIN_EXE_niki"))
}

/// A real task directory with a real `task.json`, built through the real type.
fn seed_task(root: &Path, id: &str, created_at: &str, description: &str) {
    let dir = root.join(".niki").join("tasks").join(id);
    std::fs::create_dir_all(dir.join("artifacts")).expect("create task dir");
    let record = niki::orchestrator::state::TaskRecord::new(
        uuid::Uuid::parse_str(id).expect("a valid uuid"),
        description,
    );
    let mut record = record;
    // `created_at` drives "most recent", so it is the field the test turns.
    let json = serde_json::to_value(&record).expect("serialize");
    let mut json = json;
    json["created_at"] = serde_json::Value::String(created_at.to_string());
    json["status"] = serde_json::Value::String("Completed".into());
    std::fs::write(
        dir.join("task.json"),
        serde_json::to_string_pretty(&json).expect("serialize"),
    )
    .expect("write task.json");
    std::fs::write(dir.join("changes.patch"), "diff --git a/x b/x\n").expect("write patch");
    std::fs::write(dir.join("report.md"), "# report\n").expect("write report");
    let _ = &mut record;
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

/// With no tasks at all, the command must fail with something a person can act
/// on — G6's rule, applied to the one command nobody was checking.
#[test]
fn an_empty_project_fails_with_a_reason() {
    let dir = tmp();
    let out = niki()
        .args(["dashboard", "--project"])
        .arg(dir.path())
        .output()
        .expect("niki must run");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "a project with no runs must not exit 0: {all}"
    );
    assert!(
        all.to_lowercase().contains("no tasks"),
        "and must say what it could not find, not just fail: {all}"
    );
}

/// The default picks the **most recent** run, which is the whole of the
/// selection rule.
#[test]
fn the_most_recent_run_is_the_one_dashboarded() {
    let dir = tmp();
    let older = "11111111-1111-1111-1111-111111111111";
    let newer = "22222222-2222-2222-2222-222222222222";
    seed_task(dir.path(), older, "2026-01-01T00:00:00Z", "the older run");
    seed_task(dir.path(), newer, "2026-06-01T00:00:00Z", "the newer run");

    let out = niki()
        .args(["dashboard", "--project"])
        .arg(dir.path())
        .output()
        .expect("niki must run");
    assert!(
        out.status.success(),
        "dashboard must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let html = std::fs::read_to_string(
        dir.path()
            .join(".niki/tasks")
            .join(newer)
            .join("dashboard.html"),
    )
    .expect("the newer run's dashboard must exist");
    assert!(
        html.contains("the newer run"),
        "the most recent run must be the one rendered"
    );
    assert!(
        !html.contains("the older run"),
        "the older run must not be rendered instead"
    );
    // And the older run's directory must be untouched.
    assert!(
        !dir.path()
            .join(".niki/tasks")
            .join(older)
            .join("dashboard.html")
            .exists(),
        "only the selected run may be written"
    );
}

/// `--task` selects explicitly, and `--path-only` writes nothing.
#[test]
fn path_only_prints_and_writes_nothing() {
    let dir = tmp();
    let id = "33333333-3333-3333-3333-333333333333";
    seed_task(dir.path(), id, "2026-02-02T00:00:00Z", "an explicit run");

    let out = niki()
        .args(["dashboard", "--task", id, "--path-only", "--project"])
        .arg(dir.path())
        .output()
        .expect("niki must run");
    assert!(out.status.success(), "path-only must succeed");

    let printed = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let expected = dir
        .path()
        .join(".niki/tasks")
        .join(id)
        .join("dashboard.html");
    assert_eq!(
        printed,
        expected.display().to_string(),
        "`--path-only` must print the path it would write"
    );
    assert!(
        !expected.exists(),
        "`--path-only` must not generate the file — that is what it is for"
    );
}

/// A task that does not exist must fail, and name the id.
#[test]
fn a_missing_task_fails_by_name() {
    let dir = tmp();
    let out = niki()
        .args([
            "dashboard",
            "--task",
            "44444444-4444-4444-4444-444444444444",
            "--project",
        ])
        .arg(dir.path())
        .output()
        .expect("niki must run");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "a missing task must not exit 0: {all}"
    );
    assert!(
        all.contains("44444444-4444-4444-4444-444444444444"),
        "and must name the id it could not find: {all}"
    );
}

/// The rendered HTML must escape the run's own text. A task description is
/// user-supplied and lands in the page, so this is the difference between a
/// dashboard and a script-execution surface for anyone who opens one.
#[test]
fn the_dashboard_escapes_what_it_embeds() {
    let dir = tmp();
    let id = "55555555-5555-5555-5555-555555555555";
    // A description carrying markup, and a diff line carrying markup.
    seed_task(
        dir.path(),
        id,
        "2026-03-03T00:00:00Z",
        "<script>alert('xss')</script>",
    );
    let patch = dir
        .path()
        .join(".niki/tasks")
        .join(id)
        .join("changes.patch");
    std::fs::write(
        &patch,
        "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n+<img src=x onerror=alert(1)>\n",
    )
    .expect("write patch");

    let out = niki()
        .args(["dashboard", "--task", id, "--project"])
        .arg(dir.path())
        .output()
        .expect("niki must run");
    assert!(out.status.success(), "dashboard must succeed");

    let html = std::fs::read_to_string(
        dir.path()
            .join(".niki/tasks")
            .join(id)
            .join("dashboard.html"),
    )
    .expect("the dashboard must exist");

    assert!(
        !html.contains("<script>alert"),
        "the task description was embedded unescaped: {html}"
    );
    assert!(
        !html.contains("<img src=x onerror"),
        "a diff line was embedded unescaped: {html}"
    );
    // Escaped, not deleted: the text is still readable to the person.
    assert!(
        html.contains("&lt;script&gt;") || html.contains("&lt;img"),
        "the text must be escaped rather than dropped, or the dashboard hides \\
         what happened: {html}"
    );
}
