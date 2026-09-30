//! `[Enter] open` must open something.
//!
//! The History footer says `[j/k] navigate · [Enter] open · [Esc] back`, and
//! `Enter` called `state.open_task_from_history(...)` — which loads the task's
//! patch, report and artifacts — and then **navigated nowhere**. The user
//! pressed the key, the screen did not change, and the run they had just
//! loaded sat in state until they found a page that happened to read it.
//!
//! `Enter` now goes to the Diff page, which is the run's actual output, and
//! the footer says so. Both halves are asserted: the navigation, and that the
//! diff it lands on really is that run's — a test that only checked
//! `current_page` would pass if the content belonged to a different run.

use niki::config::types::NikiConfig;
use niki::display::pages::history::HistoryPage;
use niki::display::pages::{AppState, Page, PageId};
use niki::display::state::ViewMode;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A fixed UUID so the directory name and the record agree.
const TASK_ID: &str = "11111111-2222-3333-4444-555555555555";

/// A real `.niki/tasks/<id>/` with a `task.json`, a patch, a report and
/// artifacts.
fn task_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let niki = tmp.path().join(".niki").join("tasks").join(TASK_ID);
    std::fs::create_dir_all(niki.join("artifacts")).expect("create task dir");
    // Serialized from the real type, not hand-written: `TaskRecord` has ~20
    // fields, and a JSON literal here drifted the moment one was added — the
    // History page then found no entries and `Enter` never ran, so the test
    // was asserting the behaviour of a key pressed on an empty list.
    let mut record = niki::orchestrator::state::TaskRecord::new(
        uuid::Uuid::parse_str(TASK_ID).expect("a valid uuid"),
        "add a health endpoint",
    );
    record.status = niki::orchestrator::state::TaskStatus::Completed;
    record.branch = Some("niki/abc12345".to_string());
    std::fs::write(
        niki.join("task.json"),
        serde_json::to_string_pretty(&record).expect("serialize"),
    )
    .expect("write task.json");
    std::fs::write(
        niki.join("changes.patch"),
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,4 @@\n+THIS-RUN-ONLY-MARKER\n pub fn f() {}\n",
    )
    .expect("write patch");
    std::fs::write(niki.join("report.md"), "# THIS-RUN-ONLY-REPORT\n").expect("write report");
    std::fs::write(niki.join("artifacts").join("task_spec.json"), "{}\n").expect("write spec");
    (tmp, niki)
}

fn state(project: &std::path::Path) -> AppState {
    let mut st = AppState::new(
        "test task".into(),
        NikiConfig::default(),
        project.to_path_buf(),
    );
    st.current_page = PageId::History;
    st.view = ViewMode::Page(PageId::History);
    st
}

/// `Enter` must take the user somewhere, and the somewhere is the run.
#[test]
fn enter_opens_the_run_it_selected() {
    let (tmp, _niki) = task_dir();
    let mut st = state(tmp.path());
    let mut page = HistoryPage::new();

    assert!(
        page.handle_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
            &mut st
        ),
        "`Enter` must be handled on the History page"
    );

    assert_eq!(
        st.current_page,
        PageId::Diff,
        "`[Enter] open` must take the user to the run's diff. It used to load \
         the run and stay put, so the key visibly did nothing."
    );
    // `view` drives what is rendered; `current_page` alone left the app showing
    // History while the footer claimed otherwise — the same bug shape as the
    // chat/page toggle.
    assert!(
        matches!(st.view, ViewMode::Page(PageId::Diff)),
        "`view` must move with `current_page`, or the rendered page and the \
         footer disagree"
    );
    // And the content must be *that run's*.
    let diff = st.diff_content.clone().expect("the patch must be loaded");
    assert!(
        diff.contains("THIS-RUN-ONLY-MARKER"),
        "the diff shown must be the selected run's: {diff}"
    );
    assert!(st.report_content.is_some(), "the report must be loaded too");
    assert!(
        st.artifacts_dir.is_some(),
        "the artifacts must be loaded too"
    );
}

/// And the footer must say where `Enter` goes, now that it goes somewhere.
#[test]
fn the_footer_says_where_enter_goes() {
    let (tmp, _niki) = task_dir();
    let st = state(tmp.path());
    let page = &mut HistoryPage::new();
    let mut term =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(90, 20)).expect("terminal");
    term.draw(|f| page.render(f, f.area(), &st)).expect("draw");
    let buf = term.backend().buffer().clone();
    let text: String = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("Enter] open"),
        "the footer must still advertise `Enter`: {text}"
    );
    assert!(
        text.contains("diff"),
        "the footer must say `Enter` opens the diff, or it is describing a \
         destination it does not have: {text}"
    );
}

/// `Esc` must still go back, or there is no way out but the digits.
#[test]
fn escape_still_goes_back() {
    let (tmp, _niki) = task_dir();
    let mut st = state(tmp.path());
    let mut page = HistoryPage::new();
    assert!(page.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()), &mut st));
    assert_eq!(st.current_page, PageId::Run);
}
