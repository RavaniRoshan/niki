//! Visual rendering tests.

use niki::display::pages::{Modal, PageId, StageStatus};
use niki::artifacts::types::AgentRole;

mod helpers;
use helpers::*;

// ═══════════════════════════════════════════════════════════════════════════
// Logo rendering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn logo_renders_in_buffer() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    // FIGlet "big" font uses block characters, check that logo area has content
    let mut has_content = false;
    for y in 0..8u16 {
        for x in 0..buf.area.width {
            let sym = buf[(x, y)].symbol();
            if sym != " " && !sym.is_empty() {
                has_content = true;
                break;
            }
        }
        if has_content { break; }
    }
    assert!(has_content, "logo area should have content");
}

// ═══════════════════════════════════════════════════════════════════════════
// Home page rendering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn home_page_renders_task_card() {
    let mut state = test_state();
    state.description = "fix login bug".into();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "Build");
    assert_buffer_contains(buf, "fix login bug");
}

#[test]
fn home_page_renders_tags() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "sandbox");
    assert_buffer_contains(buf, "docker");
}

#[test]
fn home_page_renders_command_line() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "niki run");
    assert_buffer_contains(buf, "--project");
}

#[test]
fn home_page_renders_footer() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "pause");
    assert_buffer_contains(buf, "scroll");
    assert_buffer_contains(buf, "quit");
}

#[test]
fn home_page_renders_queued_agents() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "queued");
}

#[test]
fn home_page_renders_pipeline_stages() {
    let state = state_with_stages();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "Planner");
    assert_buffer_contains(buf, "Coder");
    assert_buffer_contains(buf, "Tester");
    assert_buffer_contains(buf, "Reviewer");
}

#[test]
fn home_page_renders_status_bar() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    let last_y = buf.area.height - 1;
    let row: String = (0..buf.area.width)
        .map(|x| buf[(x, last_y)].symbol().to_string())
        .collect();
    assert!(row.contains("niki"), "status bar should contain 'niki'");
}

// ═══════════════════════════════════════════════════════════════════════════
// Page-specific rendering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_page_renders_flowchart() {
    let mut state = test_state();
    state.current_page = PageId::Pipeline;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "FLOW");
    assert_buffer_contains(buf, "Planner");
    assert_buffer_contains(buf, "Coder");
    assert_buffer_contains(buf, "MODELS");
}

#[test]
fn help_page_renders_keybindings() {
    let mut state = test_state();
    state.current_page = PageId::Help;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    // Help page should render something (may not contain "Help" as literal text)
    let mut has_content = false;
    for y in 10..40u16 {
        for x in 0..buf.area.width {
            let sym = buf[(x, y)].symbol();
            if sym != " " && !sym.is_empty() {
                has_content = true;
                break;
            }
        }
        if has_content { break; }
    }
    assert!(has_content, "help page should have content");
}

#[test]
fn diff_page_renders() {
    let mut state = test_state();
    state.diff_content = Some("+added\n-removed".into());
    state.current_page = PageId::Diff;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "diff");
}

#[test]
fn verdict_page_renders() {
    let mut state = test_state();
    state.current_page = PageId::Verdict;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "verdict");
}

#[test]
fn cost_page_renders() {
    let mut state = test_state();
    state.current_page = PageId::Cost;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "cost");
}

#[test]
fn artifacts_page_renders() {
    let mut state = test_state();
    state.current_page = PageId::Artifacts;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "artifacts");
}

#[test]
fn history_page_renders() {
    let mut state = test_state();
    state.current_page = PageId::History;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "history");
}

#[test]
fn config_page_renders() {
    let mut state = test_state();
    state.current_page = PageId::Config;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "config");
}

#[test]
fn agents_page_renders() {
    let mut state = state_with_stages();
    state.current_page = PageId::Agents;
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "agents");
}

// ═══════════════════════════════════════════════════════════════════════════
// Modal rendering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn confirm_modal_renders_over_page() {
    let mut state = test_state();
    state.modal = Some(Modal::Confirm {
        title: "Quit NIKI?".into(),
        message: "The pipeline will continue.".into(),
    });
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "Quit NIKI?");
    assert_buffer_contains(buf, "confirm");
    assert_buffer_contains(buf, "cancel");
}

#[test]
fn error_modal_renders_over_page() {
    let mut state = test_state();
    state.modal = Some(Modal::Error {
        stage: "Coder".into(),
        message: "API timeout".into(),
        hint: "Check network".into(),
    });
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    // Error modal should render with title "Coder" and message content
    assert_buffer_contains(buf, "Coder");
    assert_buffer_contains(buf, "API timeout");
}

// ═══════════════════════════════════════════════════════════════════════════
// Token display in status bar
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn status_bar_shows_tokens_when_present() {
    let mut state = test_state();
    state.stages = vec![make_stage(AgentRole::Planner, StageStatus::Done, 1500, 2500, 0.01)];
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    let last_y = buf.area.height - 1;
    let row: String = (0..buf.area.width)
        .map(|x| buf[(x, last_y)].symbol().to_string())
        .collect();
    assert!(row.contains("tok"), "status bar should show token count");
    assert!(row.contains("$"), "status bar should show cost");
}

#[test]
fn status_bar_hides_cost_when_zero() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    let last_y = buf.area.height - 1;
    let row: String = (0..buf.area.width)
        .map(|x| buf[(x, last_y)].symbol().to_string())
        .collect();
    assert!(!row.contains("$0"), "status bar should not show $0 cost");
}

// ═══════════════════════════════════════════════════════════════════════════
// Page content rendering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn home_page_shows_branch_name() {
    let mut state = test_state();
    state.branch_name = "niki/abc123".into();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "niki/abc123");
}

#[test]
fn home_page_shows_working_tree() {
    let state = test_state();
    let terminal = render_full(&state);
    let buf = terminal.backend().buffer();
    assert_buffer_contains(buf, "working tree");
    assert_buffer_contains(buf, "untouched");
}
