//! Tests for all 10 page `handle_key()` implementations.

use niki::display::pages::{Page, PageId, PageRouter};
use ratatui::crossterm::event::KeyCode;

mod helpers;
use helpers::*;

// ═══════════════════════════════════════════════════════════════════════════
// RunPage (home)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn run_page_navigates_to_all_pages() {
    let mut router = PageRouter::new();
    let cases: &[(char, PageId)] = &[
        ('p', PageId::Pipeline),
        ('a', PageId::Agents),
        ('d', PageId::Diff),
        ('v', PageId::Verdict),
        ('c', PageId::Cost),
        ('f', PageId::Artifacts),
        ('h', PageId::History),
        ('?', PageId::Help),
    ];
    for &(ch, target) in cases {
        let mut state = test_state();
        state.current_page = PageId::Run;
        let consumed = press(&mut router, &mut state, key_char(ch));
        assert!(consumed, "key '{ch}' should be consumed on Run page");
        assert_eq!(
            state.current_page, target,
            "key '{ch}' should navigate to {target:?}"
        );
    }
}

#[test]
fn run_page_esc_opens_quit_modal() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;
    let consumed = press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert!(consumed);
    assert!(state.modal.is_some(), "Esc should open modal");
}

#[test]
fn run_page_q_opens_quit_modal() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;
    let consumed = press(&mut router, &mut state, key_char('q'));
    assert!(consumed);
    assert!(state.modal.is_some(), "'q' should open modal");
}

#[test]
fn run_page_space_toggles_pause() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;
    assert!(!state.paused);
    press(&mut router, &mut state, key_char(' '));
    assert!(state.paused);
    press(&mut router, &mut state, key_char(' '));
    assert!(!state.paused);
}

#[test]
fn run_page_scroll_keys() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;

    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('g'));
    press(&mut router, &mut state, key_char('G'));
    state.current_page = PageId::Run;
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('k'));
}

#[test]
fn run_page_unknown_key_ignored() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Run;
    let consumed = press(&mut router, &mut state, key_char('z'));
    assert!(!consumed);
}

#[test]
fn run_page_title() {
    let page = niki::display::pages::run::RunPage::new();
    assert_eq!(page.title(), "home");
}

// ═══════════════════════════════════════════════════════════════════════════
// PipelinePage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Pipeline;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn pipeline_page_q_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Pipeline;
    press(&mut router, &mut state, key_char('q'));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn pipeline_page_j_k_select_stage() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Pipeline;

    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('k'));
}

#[test]
fn pipeline_page_navigates_to_agents_and_config() {
    let mut router = PageRouter::new();
    let mut state = test_state();

    state.current_page = PageId::Pipeline;
    press(&mut router, &mut state, key_char('a'));
    assert_eq!(state.current_page, PageId::Agents);

    state.current_page = PageId::Pipeline;
    press(&mut router, &mut state, key_char(','));
    assert_eq!(state.current_page, PageId::Config);
}

#[test]
fn pipeline_page_title() {
    let page = niki::display::pages::pipeline::PipelinePage::new();
    assert_eq!(page.title(), "pipeline");
}

// ═══════════════════════════════════════════════════════════════════════════
// AgentsPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn agents_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = state_with_stages();
    state.current_page = PageId::Agents;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn agents_page_tab_cycles_forward() {
    let mut router = PageRouter::new();
    let mut state = state_with_stages();
    state.current_page = PageId::Agents;

    press(&mut router, &mut state, key_code(KeyCode::Tab));
    press(&mut router, &mut state, key_code(KeyCode::Tab));
    press(&mut router, &mut state, key_code(KeyCode::Tab));
}

#[test]
fn agents_page_backtab_cycles_backward() {
    let mut router = PageRouter::new();
    let mut state = state_with_stages();
    state.current_page = PageId::Agents;

    press(&mut router, &mut state, key_code(KeyCode::BackTab));
}

#[test]
fn agents_page_scroll() {
    let mut router = PageRouter::new();
    let mut state = state_with_stages();
    state.current_page = PageId::Agents;

    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('g'));
    press(&mut router, &mut state, key_char('G'));
}

#[test]
fn agents_page_navigates_to_diff() {
    let mut router = PageRouter::new();
    let mut state = state_with_stages();
    state.current_page = PageId::Agents;
    press(&mut router, &mut state, key_char('d'));
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn agents_page_title() {
    let page = niki::display::pages::agents::AgentsPage::new();
    assert_eq!(page.title(), "agents");
}

// ═══════════════════════════════════════════════════════════════════════════
// DiffPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn diff_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Diff;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn diff_page_scroll() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Diff;

    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('g'));
    press(&mut router, &mut state, key_char('G'));
}

#[test]
fn diff_page_r_toggles_annotations() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Diff;

    let consumed = press(&mut router, &mut state, key_char('r'));
    assert!(consumed);
}

#[test]
fn diff_page_navigates_to_verdict() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Diff;
    press(&mut router, &mut state, key_char('v'));
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn diff_page_title() {
    let page = niki::display::pages::diff::DiffPage::new();
    assert_eq!(page.title(), "diff");
}

// ═══════════════════════════════════════════════════════════════════════════
// VerdictPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn verdict_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Verdict;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn verdict_page_scroll() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Verdict;

    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('k'));
    press(&mut router, &mut state, key_char('g'));
    press(&mut router, &mut state, key_char('G'));
}

#[test]
fn verdict_page_navigates_to_diff_and_cost() {
    let mut router = PageRouter::new();
    let mut state = test_state();

    state.current_page = PageId::Verdict;
    press(&mut router, &mut state, key_char('d'));
    assert_eq!(state.current_page, PageId::Diff);

    state.current_page = PageId::Verdict;
    press(&mut router, &mut state, key_char('c'));
    assert_eq!(state.current_page, PageId::Cost);
}

#[test]
fn verdict_page_title() {
    let page = niki::display::pages::verdict::VerdictPage::new();
    assert_eq!(page.title(), "verdict");
}

// ═══════════════════════════════════════════════════════════════════════════
// CostPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cost_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Cost;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cost_page_scroll() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Cost;

    press(&mut router, &mut state, key_char('j'));
    press(&mut router, &mut state, key_char('k'));
}

#[test]
fn cost_page_navigates_to_verdict() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Cost;
    press(&mut router, &mut state, key_char('v'));
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn cost_page_title() {
    let page = niki::display::pages::cost::CostPage::new();
    assert_eq!(page.title(), "cost");
}

// ═══════════════════════════════════════════════════════════════════════════
// ArtifactsPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn artifacts_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Artifacts;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn artifacts_page_j_k_selection() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Artifacts;

    for _ in 0..15 {
        press(&mut router, &mut state, key_char('j'));
    }
    for _ in 0..15 {
        press(&mut router, &mut state, key_char('k'));
    }
}

#[test]
fn artifacts_page_navigates_to_history() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Artifacts;
    press(&mut router, &mut state, key_char('h'));
    assert_eq!(state.current_page, PageId::History);
}

#[test]
fn artifacts_page_title() {
    let page = niki::display::pages::artifacts::ArtifactsPage::new();
    assert_eq!(page.title(), "artifacts");
}

// ═══════════════════════════════════════════════════════════════════════════
// HistoryPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn history_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::History;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn history_page_j_k_selection() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::History;

    for _ in 0..10 {
        press(&mut router, &mut state, key_char('j'));
    }
    for _ in 0..10 {
        press(&mut router, &mut state, key_char('k'));
    }
}

#[test]
fn history_page_navigates_to_artifacts_and_pipeline() {
    let mut router = PageRouter::new();
    let mut state = test_state();

    state.current_page = PageId::History;
    press(&mut router, &mut state, key_char('f'));
    assert_eq!(state.current_page, PageId::Artifacts);

    state.current_page = PageId::History;
    press(&mut router, &mut state, key_char('p'));
    assert_eq!(state.current_page, PageId::Pipeline);
}

#[test]
fn history_page_title() {
    let page = niki::display::pages::history::HistoryPage::new();
    assert_eq!(page.title(), "history");
}

// ═══════════════════════════════════════════════════════════════════════════
// ConfigPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn config_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Config;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn config_page_tab_cycles_fields() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Config;

    for _ in 0..14 {
        press(&mut router, &mut state, key_code(KeyCode::Tab));
    }
    press(&mut router, &mut state, key_code(KeyCode::BackTab));
}

#[test]
fn config_page_navigates_to_cost() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Config;
    press(&mut router, &mut state, key_char('c'));
    assert_eq!(state.current_page, PageId::Cost);
}

#[test]
fn config_page_title() {
    let page = niki::display::pages::config::ConfigPage::new();
    assert_eq!(page.title(), "config");
}

// ═══════════════════════════════════════════════════════════════════════════
// HelpPage
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn help_page_esc_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Help;
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn help_page_q_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Help;
    press(&mut router, &mut state, key_char('q'));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn help_page_question_mark_returns_to_run() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    state.current_page = PageId::Help;
    press(&mut router, &mut state, key_char('?'));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn help_page_title() {
    let page = niki::display::pages::help::HelpPage;
    assert_eq!(page.title(), "help");
}
