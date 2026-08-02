//! Page transition and navigation tests.

use ratatui::crossterm::event::KeyCode;
use niki::display::pages::{PageId, PageRouter};

mod helpers;
use helpers::*;

// ═══════════════════════════════════════════════════════════════════════════
// Run → All sub-pages → Run (round-trip)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn run_to_pipeline_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Pipeline);
    assert_eq!(state.current_page, PageId::Pipeline);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_agents_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Agents);
    assert_eq!(state.current_page, PageId::Agents);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_diff_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Diff);
    assert_eq!(state.current_page, PageId::Diff);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_verdict_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Verdict);
    assert_eq!(state.current_page, PageId::Verdict);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_cost_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Cost);
    assert_eq!(state.current_page, PageId::Cost);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_artifacts_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Artifacts);
    assert_eq!(state.current_page, PageId::Artifacts);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_history_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::History);
    assert_eq!(state.current_page, PageId::History);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_config_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    // Config is accessed from Pipeline page, not directly from Run
    goto_page_from_run(&mut router, &mut state, PageId::Pipeline);
    state.current_page = PageId::Pipeline;
    press(&mut router, &mut state, key_char(','));
    assert_eq!(state.current_page, PageId::Config);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_to_help_and_back() {
    let mut router = PageRouter::new();
    let mut state = test_state();
    goto_page_from_run(&mut router, &mut state, PageId::Help);
    assert_eq!(state.current_page, PageId::Help);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

// ═══════════════════════════════════════════════════════════════════════════
// Cross-page navigation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pipeline_to_agents_to_diff_to_verdict_to_cost() {
    let mut router = PageRouter::new();
    let mut state = test_state();

    goto_page_from_run(&mut router, &mut state, PageId::Pipeline);
    press(&mut router, &mut state, key_char('a'));
    assert_eq!(state.current_page, PageId::Agents);
    press(&mut router, &mut state, key_char('d'));
    assert_eq!(state.current_page, PageId::Diff);
    press(&mut router, &mut state, key_char('v'));
    assert_eq!(state.current_page, PageId::Verdict);
    press(&mut router, &mut state, key_char('c'));
    assert_eq!(state.current_page, PageId::Cost);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn artifacts_to_history_to_pipeline_roundtrip() {
    let mut router = PageRouter::new();
    let mut state = test_state();

    goto_page_from_run(&mut router, &mut state, PageId::Artifacts);
    press(&mut router, &mut state, key_char('h'));
    assert_eq!(state.current_page, PageId::History);
    press(&mut router, &mut state, key_char('p'));
    assert_eq!(state.current_page, PageId::Pipeline);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn config_to_cost_to_verdict_to_diff() {
    let mut router = PageRouter::new();
    let mut state = test_state();

    goto_page_from_run(&mut router, &mut state, PageId::Config);
    press(&mut router, &mut state, key_char('c'));
    assert_eq!(state.current_page, PageId::Cost);
    press(&mut router, &mut state, key_char('v'));
    assert_eq!(state.current_page, PageId::Verdict);
    press(&mut router, &mut state, key_char('d'));
    assert_eq!(state.current_page, PageId::Diff);
    press(&mut router, &mut state, key_code(KeyCode::Esc));
    assert_eq!(state.current_page, PageId::Run);
}

// ═══════════════════════════════════════════════════════════════════════════
// All pages return to Run via 'q'
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_subpages_return_to_run_via_q() {
    let subpages = &[
        PageId::Pipeline,
        PageId::Agents,
        PageId::Diff,
        PageId::Verdict,
        PageId::Cost,
        PageId::Artifacts,
        PageId::History,
        PageId::Config,
        PageId::Help,
    ];
    for &page in subpages {
        let mut router = PageRouter::new();
        let mut state = test_state();
        state.current_page = page;
        let consumed = press(&mut router, &mut state, key_char('q'));
        assert!(consumed, "{page:?} should consume 'q'");
        assert_eq!(state.current_page, PageId::Run, "{page:?} 'q' should go to Run");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PageId properties
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn page_id_all_contains_10_pages() {
    assert_eq!(PageId::all().len(), 10);
}

#[test]
fn page_id_index_covers_all() {
    for (i, page) in PageId::all().iter().enumerate() {
        assert_eq!(page.index(), i);
    }
}

#[test]
fn page_id_title_covers_all() {
    for page in PageId::all() {
        let title = page.title();
        assert!(!title.is_empty(), "{page:?} has empty title");
    }
}

#[test]
fn page_id_key_hint_covers_all() {
    for page in PageId::all() {
        let _hint = page.key_hint();
    }
}

#[test]
fn page_id_from_key_roundtrip() {
    let mappings: &[(char, PageId)] = &[
        ('p', PageId::Pipeline),
        ('a', PageId::Agents),
        ('d', PageId::Diff),
        ('v', PageId::Verdict),
        ('c', PageId::Cost),
        ('f', PageId::Artifacts),
        ('h', PageId::History),
        (',', PageId::Config),
        ('?', PageId::Help),
    ];
    for &(ch, expected) in mappings {
        assert_eq!(PageId::from_key(ch), Some(expected), "from_key('{ch}')");
    }
    assert_eq!(PageId::from_key('x'), None);
    assert_eq!(PageId::from_key(' '), None);
    assert_eq!(PageId::from_key('\n'), None);
}
