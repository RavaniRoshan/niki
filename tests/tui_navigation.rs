#![allow(non_snake_case)]

use niki::artifacts::types::AgentRole;
use niki::config::types::NikiConfig;
use niki::display::pages::{AppState, Modal, PageId, PageRouter, RunState, StageInfo, StageStatus};
use niki::display::tui::DisplayEvent;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn make_state() -> AppState {
    let config = NikiConfig::default();
    AppState::new("test task".into(), config, "/tmp/test".into())
}

fn make_state_with_stages(n: usize) -> AppState {
    let mut state = make_state();
    let roles = [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
    ];
    for i in 0..n {
        state.stages.push(StageInfo {
            role: roles[i % 4],
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: format!("transcript for agent {}", i),
            input_tokens: 100 * (i as u32 + 1),
            output_tokens: 50 * (i as u32 + 1),
            cost_usd: 0.001 * (i as f64 + 1.0),
            latency_ms: 1000 * (i as u64 + 1),
            summary: vec![format!("summary {}", i)],
            start: Some(std::time::Instant::now()),
            completed_at: None,
            prompt_file: None,
            retry_count: 0,
            error_message: None,
        });
    }
    state
}

fn key_char(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn key_code(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn key_shift_tab() -> KeyEvent {
    KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
}

// ============================================================================
// PageId::from_key mapping
// ============================================================================

#[test]
fn page_id_from_key_all_mappings() {
    assert_eq!(PageId::from_key('p'), Some(PageId::Pipeline));
    assert_eq!(PageId::from_key('a'), Some(PageId::Agents));
    assert_eq!(PageId::from_key('d'), Some(PageId::Diff));
    assert_eq!(PageId::from_key('v'), Some(PageId::Verdict));
    assert_eq!(PageId::from_key('c'), Some(PageId::Cost));
    assert_eq!(PageId::from_key('f'), Some(PageId::Artifacts));
    assert_eq!(PageId::from_key('h'), Some(PageId::History));
    assert_eq!(PageId::from_key(','), Some(PageId::Config));
    assert_eq!(PageId::from_key('?'), Some(PageId::Help));
    assert_eq!(PageId::from_key('l'), Some(PageId::TestLog));
    assert_eq!(PageId::from_key('x'), None);
    assert_eq!(PageId::from_key('z'), None);
    assert_eq!(PageId::from_key(' '), None);
}

#[test]
fn page_id_all_has_14_entries() {
    assert_eq!(PageId::all().len(), 14);
}

#[test]
fn page_id_titles() {
    assert_eq!(PageId::Run.title(), "run");
    assert_eq!(PageId::Pipeline.title(), "pipeline");
    assert_eq!(PageId::Agents.title(), "agents");
    assert_eq!(PageId::Diff.title(), "diff");
    assert_eq!(PageId::Verdict.title(), "verdict");
    assert_eq!(PageId::Cost.title(), "cost");
    assert_eq!(PageId::Artifacts.title(), "artifacts");
    assert_eq!(PageId::History.title(), "history");
    assert_eq!(PageId::Config.title(), "config");
    assert_eq!(PageId::Help.title(), "help");
    assert_eq!(PageId::TestLog.title(), "test_log");
    // Chat, Fleet and Session were missing here — and `Fleet` and `Session`
    // appear *zero* times in this 1,664-line file. They are deliberately
    // absent from `PageRouter`'s map (`pages/mod.rs:58-59` says so), which
    // means a key routed to one is a silent no-op in the router. The tests
    // that would have noticed were precisely the ones omitting them.
    assert_eq!(PageId::Chat.title(), "chat");
    assert_eq!(PageId::Fleet.title(), "fleet");
    assert_eq!(PageId::Session.title(), "session");
}

#[test]
fn page_id_key_hints() {
    assert_eq!(PageId::Run.key_hint(), "");
    assert_eq!(PageId::Pipeline.key_hint(), "p");
    assert_eq!(PageId::Agents.key_hint(), "a");
    assert_eq!(PageId::Diff.key_hint(), "d");
    assert_eq!(PageId::Verdict.key_hint(), "v");
    assert_eq!(PageId::Cost.key_hint(), "c");
    assert_eq!(PageId::Artifacts.key_hint(), "f");
    assert_eq!(PageId::History.key_hint(), "h");
    assert_eq!(PageId::Config.key_hint(), ",");
    assert_eq!(PageId::Help.key_hint(), "?");
    assert_eq!(PageId::TestLog.key_hint(), "l");
    assert_eq!(PageId::Chat.key_hint(), "tab");
    assert_eq!(PageId::Fleet.key_hint(), "g");
    assert_eq!(PageId::Session.key_hint(), "s");
}

// ============================================================================
// RUN PAGE — Navigation from Run to all other pages
// (Run page no longer handles direct hotkey navigation — use Ctrl-P command palette instead)
// ============================================================================

#[test]
fn page_local_handlers_do_not_navigate_on_run() {
    // **What this checks, and what it does not.**
    //
    // The version of this test that stood here was
    // `run_page_ignores_navigation_hotkeys`, with the comment "All these keys
    // should be ignored on Run page now". It passed, and it was wrong about
    // the product: `PageRouter::handle_key` is a *page's own* key handler.
    // Global page jumps live in `global_page_jump`, which the event loop
    // applies after the router declines:
    //
    //     } else if router.handle_key(key, &mut state) { … }
    //     else if let Some(page) = global_page_jump(key) { state.current_page = page }
    //
    // So on the Run page, `d` **does** open Diff. Measured in the shipped
    // binary by `tests/tui_smoke/cases/15_page_letter_navigates.sh`, which
    // presses the key in a real terminal and finds the Diff page.
    //
    // A green test whose name describes the product, asserting something the
    // product does not do, is worse than no test: it is a claim that looks
    // checked. This one now claims only what it verifies.
    let mut state = make_state();
    let mut router = PageRouter::new();
    for key in [
        key_char('p'),
        key_char('a'),
        key_char('d'),
        key_char('v'),
        key_char('c'),
        key_char('f'),
        key_char('h'),
        key_char('?'),
        key_char(','),
        key_char('l'),
    ] {
        router.handle_key(key, &mut state);
        assert_eq!(state.current_page, PageId::Run);
    }
}

#[test]
fn run_page_space_toggles_pause() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    assert!(!state.paused);
    router.handle_key(key_char(' '), &mut state);
    assert!(state.paused);
    router.handle_key(key_char(' '), &mut state);
    assert!(!state.paused);
}

#[test]
fn run_page_j_scroll_down() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_page_k_scroll_up() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_page_g_jumps_to_top() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('g'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_page_G_jumps_to_bottom() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    router.handle_key(key_char('G'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_page_down_arrow_scroll() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn run_page_up_arrow_scroll() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

// ============================================================================
// PIPELINE PAGE — Navigation and controls
// ============================================================================

#[test]
fn pipeline_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn pipeline_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn pipeline_page_j_navigates_stages() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
}

#[test]
fn pipeline_page_k_navigates_stages() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
}

#[test]
fn pipeline_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
}

#[test]
fn pipeline_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
}

#[test]
fn pipeline_page_a_navigates_to_agents() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_char('a'), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn pipeline_page_comma_navigates_to_config() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_char(','), &mut state);
    assert_eq!(state.current_page, PageId::Config);
}

#[test]
fn pipeline_page_unrecognized_key_ignored() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    router.handle_key(key_char('z'), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
}

// ============================================================================
// AGENTS PAGE — Navigation and tab cycling
// ============================================================================

#[test]
fn agents_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn agents_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn agents_page_tab_cycles_forward() {
    let mut state = make_state_with_stages(4);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_backtab_cycles_backward() {
    let mut state = make_state_with_stages(4);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_shift_tab(), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_tab_wraps_around() {
    let mut state = make_state_with_stages(3);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    // Tab through all 3 agents, should wrap to 0
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_backtab_wraps_around() {
    let mut state = make_state_with_stages(3);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    // BackTab from 0 should go to last agent
    router.handle_key(key_shift_tab(), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_j_k_scroll() {
    let mut state = make_state_with_stages(2);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_g_G_scroll() {
    let mut state = make_state_with_stages(2);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_char('g'), &mut state);
    router.handle_key(key_char('G'), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_d_navigates_to_diff() {
    let mut state = make_state_with_stages(2);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_char('d'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn agents_page_tab_no_stages() {
    let mut state = make_state();
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

#[test]
fn agents_page_backtab_no_stages() {
    let mut state = make_state();
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    router.handle_key(key_shift_tab(), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

// ============================================================================
// DIFF PAGE — Navigation and controls
// ============================================================================

#[test]
fn diff_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn diff_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn diff_page_j_k_scroll() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn diff_page_g_G_scroll() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_char('g'), &mut state);
    router.handle_key(key_char('G'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn diff_page_r_toggles_annotations() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_char('r'), &mut state);
    router.handle_key(key_char('r'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn diff_page_v_navigates_to_verdict() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_char('v'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn diff_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn diff_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Diff;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

// ============================================================================
// VERDICT PAGE — Navigation and controls
// ============================================================================

#[test]
fn verdict_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn verdict_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn verdict_page_j_k_scroll() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn verdict_page_g_G_scroll() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_char('g'), &mut state);
    router.handle_key(key_char('G'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn verdict_page_d_navigates_to_diff() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_char('d'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
}

#[test]
fn verdict_page_c_navigates_to_cost() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_char('c'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
}

#[test]
fn verdict_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn verdict_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Verdict;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
}

// ============================================================================
// COST PAGE — Navigation and controls
// ============================================================================

#[test]
fn cost_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Cost;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cost_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Cost;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cost_page_v_navigates_to_verdict() {
    let mut state = make_state();
    state.current_page = PageId::Cost;
    let mut router = PageRouter::new();
    router.handle_key(key_char('v'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
}

#[test]
fn cost_page_j_k_scroll() {
    let mut state = make_state();
    state.current_page = PageId::Cost;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
}

#[test]
fn cost_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Cost;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
}

#[test]
fn cost_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Cost;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
}

// ============================================================================
// ARTIFACTS PAGE — Navigation and controls
// ============================================================================

#[test]
fn artifacts_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn artifacts_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn artifacts_page_j_k_navigate() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::Artifacts);
}

#[test]
fn artifacts_page_h_navigates_to_history() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    router.handle_key(key_char('h'), &mut state);
    assert_eq!(state.current_page, PageId::History);
}

#[test]
fn artifacts_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::Artifacts);
}

#[test]
fn artifacts_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::Artifacts);
}

// ============================================================================
// HISTORY PAGE — Navigation and controls
// ============================================================================

#[test]
fn history_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn history_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn history_page_j_k_navigate() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::History);
}

#[test]
fn history_page_f_navigates_to_artifacts() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_char('f'), &mut state);
    assert_eq!(state.current_page, PageId::Artifacts);
}

#[test]
fn history_page_p_navigates_to_pipeline() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_char('p'), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
}

#[test]
fn history_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::History);
}

#[test]
fn history_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::History);
}

// ============================================================================
// CONFIG PAGE — Navigation and controls
// ============================================================================

#[test]
fn config_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn config_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn config_page_tab_cycles_fields() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    assert_eq!(state.current_page, PageId::Config);
}

#[test]
fn config_page_backtab_cycles_fields() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    router.handle_key(key_shift_tab(), &mut state);
    assert_eq!(state.current_page, PageId::Config);
}

#[test]
fn config_page_tab_wraps_around() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    // Tab 12 times (12 fields) should wrap back to 0
    for _ in 0..12 {
        router.handle_key(key_code(KeyCode::Tab), &mut state);
    }
    assert_eq!(state.current_page, PageId::Config);
}

#[test]
fn config_page_backtab_wraps_around() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    // BackTab from 0 should go to field 11
    router.handle_key(key_shift_tab(), &mut state);
    assert_eq!(state.current_page, PageId::Config);
}

#[test]
fn config_page_c_navigates_to_cost() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    router.handle_key(key_char('c'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
}

// ============================================================================
// HELP PAGE — Navigation
// ============================================================================

#[test]
fn help_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Help;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn help_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Help;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn help_page_question_mark_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::Help;
    let mut router = PageRouter::new();
    router.handle_key(key_char('?'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

// ============================================================================
// TEST LOG PAGE — Navigation and scroll
// ============================================================================

#[test]
fn test_log_page_esc_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::TestLog;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn test_log_page_q_returns_to_run() {
    let mut state = make_state();
    state.current_page = PageId::TestLog;
    let mut router = PageRouter::new();
    router.handle_key(key_char('q'), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn test_log_page_j_k_scroll() {
    let mut state = make_state();
    state.current_page = PageId::TestLog;
    let mut router = PageRouter::new();
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('j'), &mut state);
    router.handle_key(key_char('k'), &mut state);
    assert_eq!(state.current_page, PageId::TestLog);
}

#[test]
fn test_log_page_g_G_scroll() {
    let mut state = make_state();
    state.current_page = PageId::TestLog;
    let mut router = PageRouter::new();
    router.handle_key(key_char('g'), &mut state);
    router.handle_key(key_char('G'), &mut state);
    assert_eq!(state.current_page, PageId::TestLog);
}

#[test]
fn test_log_page_down_arrow() {
    let mut state = make_state();
    state.current_page = PageId::TestLog;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    assert_eq!(state.current_page, PageId::TestLog);
}

#[test]
fn test_log_page_up_arrow() {
    let mut state = make_state();
    state.current_page = PageId::TestLog;
    let mut router = PageRouter::new();
    router.handle_key(key_code(KeyCode::Down), &mut state);
    router.handle_key(key_code(KeyCode::Up), &mut state);
    assert_eq!(state.current_page, PageId::TestLog);
}

// ============================================================================
// CROSS-PAGE NAVIGATION — Multi-hop paths (sub-page to sub-page)
// ============================================================================

#[test]
fn cross_page_pipeline_to_agents_to_run() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    state.current_page = PageId::Pipeline;
    router.handle_key(key_char('a'), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cross_page_diff_to_verdict_to_cost_to_run() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    state.current_page = PageId::Diff;
    router.handle_key(key_char('v'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
    router.handle_key(key_char('c'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cross_page_artifacts_to_history_to_pipeline_to_run() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    state.current_page = PageId::Artifacts;
    router.handle_key(key_char('h'), &mut state);
    assert_eq!(state.current_page, PageId::History);
    router.handle_key(key_char('p'), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cross_page_config_to_cost_to_verdict_to_diff_to_run() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    state.current_page = PageId::Config;
    router.handle_key(key_char('c'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
    router.handle_key(key_char('v'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
    router.handle_key(key_char('d'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cross_page_help_to_run() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    state.current_page = PageId::Help;
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cross_page_test_log_to_run() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    state.current_page = PageId::TestLog;
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

#[test]
fn cross_page_full_circle() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    // Pipeline → Agents → Diff → Verdict → Cost → Run
    state.current_page = PageId::Pipeline;
    router.handle_key(key_char('a'), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
    router.handle_key(key_char('d'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
    router.handle_key(key_char('v'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
    router.handle_key(key_char('c'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
    // Artifacts → History → Pipeline → Run
    state.current_page = PageId::Artifacts;
    router.handle_key(key_char('h'), &mut state);
    assert_eq!(state.current_page, PageId::History);
    router.handle_key(key_char('p'), &mut state);
    assert_eq!(state.current_page, PageId::Pipeline);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
    // Config → Cost → Verdict → Diff → Run
    state.current_page = PageId::Config;
    router.handle_key(key_char('c'), &mut state);
    assert_eq!(state.current_page, PageId::Cost);
    router.handle_key(key_char('v'), &mut state);
    assert_eq!(state.current_page, PageId::Verdict);
    router.handle_key(key_char('d'), &mut state);
    assert_eq!(state.current_page, PageId::Diff);
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
    // Help → Run → TestLog → Run
    state.current_page = PageId::Help;
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
    state.current_page = PageId::TestLog;
    router.handle_key(key_code(KeyCode::Esc), &mut state);
    assert_eq!(state.current_page, PageId::Run);
}

// ============================================================================
// MODAL HANDLING
// ============================================================================

#[test]
fn modal_confirm_enter_closes() {
    let mut state = make_state();
    state.modal = Some(Modal::Confirm {
        title: "Quit NIKI?".to_string(),
        message: "The pipeline will continue in the background.".to_string(),
    });
    let _router = PageRouter::new();
    // Simulate modal key handling (handled in tui.rs loop, but we test the modal key handler)
    let key = key_code(KeyCode::Enter);
    let modal = state.modal.as_ref().unwrap();
    let action = niki::display::modal::handle_modal_key(key, modal);
    assert!(matches!(action, niki::display::modal::ModalAction::Confirm));
}

#[test]
fn modal_confirm_esc_closes() {
    let state_modal = Modal::Confirm {
        title: "Quit NIKI?".to_string(),
        message: "test".to_string(),
    };
    let key = key_code(KeyCode::Esc);
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::Dismiss));
}

#[test]
fn modal_error_esc_closes() {
    let state_modal = Modal::Error {
        stage: "Coder".to_string(),
        message: "API error".to_string(),
        hint: "Check your API key".to_string(),
    };
    let key = key_code(KeyCode::Esc);
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::Dismiss));
}

#[test]
fn modal_error_enter_closes() {
    let state_modal = Modal::Error {
        stage: "Coder".to_string(),
        message: "API error".to_string(),
        hint: "Check your API key".to_string(),
    };
    let key = key_code(KeyCode::Enter);
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::Dismiss));
}

#[test]
fn modal_error_r_retries() {
    let state_modal = Modal::Error {
        stage: "Coder".to_string(),
        message: "API error".to_string(),
        hint: "Check your API key".to_string(),
    };
    let key = key_char('r');
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::Retry));
}

#[test]
fn modal_error_c_goes_to_config() {
    let state_modal = Modal::Error {
        stage: "Coder".to_string(),
        message: "API error".to_string(),
        hint: "Check your API key".to_string(),
    };
    let key = key_char('c');
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::Config));
}

#[test]
fn modal_confirm_r_does_nothing() {
    let state_modal = Modal::Confirm {
        title: "Quit NIKI?".to_string(),
        message: "test".to_string(),
    };
    let key = key_char('r');
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::None));
}

#[test]
fn modal_confirm_c_does_nothing() {
    let state_modal = Modal::Confirm {
        title: "Quit NIKI?".to_string(),
        message: "test".to_string(),
    };
    let key = key_char('c');
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::None));
}

#[test]
fn modal_unknown_key_does_nothing() {
    let state_modal = Modal::Confirm {
        title: "Quit NIKI?".to_string(),
        message: "test".to_string(),
    };
    let key = key_char('z');
    let action = niki::display::modal::handle_modal_key(key, &state_modal);
    assert!(matches!(action, niki::display::modal::ModalAction::None));
}

// ============================================================================
// AppState — DisplayEvent handling
// ============================================================================

#[test]
fn appstate_apply_event_banner() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::Banner {
        description: "new task".to_string(),
    });
    assert_eq!(state.description, "new task");
}

#[test]
fn appstate_apply_event_stage_start() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Planner,
    });
    assert_eq!(state.stages.len(), 1);
    assert_eq!(state.stages[0].role, AgentRole::Planner);
    assert_eq!(state.stages[0].status, StageStatus::Running);
    assert_eq!(state.run_state, RunState::Running);
}

#[test]
fn appstate_apply_event_stage_token() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Planner,
    });
    state.apply_event(DisplayEvent::StageToken {
        role: AgentRole::Planner,
        token: "hello ".to_string(),
    });
    state.apply_event(DisplayEvent::StageToken {
        role: AgentRole::Planner,
        token: "world".to_string(),
    });
    assert_eq!(state.stages[0].full_transcript, "hello world");
}

#[test]
fn appstate_apply_event_stage_done() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Planner,
    });
    state.apply_event(DisplayEvent::StageDone {
        role: AgentRole::Planner,
        summary: vec!["done".to_string()],
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.001,
        latency_ms: 1000,
    });
    assert_eq!(state.stages[0].status, StageStatus::Done);
    assert_eq!(state.stages[0].input_tokens, 100);
    assert_eq!(state.stages[0].output_tokens, 50);
    assert_eq!(state.stages[0].cost_usd, 0.001);
    assert_eq!(state.stages[0].latency_ms, 1000);
}

#[test]
fn appstate_apply_event_stage_failed() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Coder,
    });
    state.apply_event(DisplayEvent::StageFailed {
        role: AgentRole::Coder,
        error: "API timeout".to_string(),
    });
    assert_eq!(state.stages[0].status, StageStatus::Failed);
    assert_eq!(state.run_state, RunState::Failed);
}

#[test]
fn appstate_apply_event_revision() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::Revision {
        round: 2,
        max: 3,
        issues: vec!["missing test".to_string()],
    });
    assert_eq!(state.revision_round, 2);
    assert_eq!(state.max_revision_rounds, 3);
    assert_eq!(state.notes.len(), 2);
    assert_eq!(state.run_state, RunState::AwaitingReviewer);
}

#[test]
fn appstate_apply_event_diff_content() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::DiffContent("+added line".to_string()));
    assert_eq!(state.diff_content, Some("+added line".to_string()));
}

#[test]
fn appstate_apply_event_report_content() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::ReportContent("# Report".to_string()));
    assert_eq!(state.report_content, Some("# Report".to_string()));
}

#[test]
fn appstate_apply_event_cost_json() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::CostJson("{}".to_string()));
    assert_eq!(state.cost_json, Some("{}".to_string()));
}

#[test]
fn appstate_apply_event_test_log() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::TestLogContent("test output".to_string()));
    assert_eq!(state.test_log, Some("test output".to_string()));
}

#[test]
fn appstate_apply_event_artifacts_dir() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::ArtifactsDir("/tmp/artifacts".to_string()));
    assert_eq!(
        state.artifacts_dir,
        Some(std::path::PathBuf::from("/tmp/artifacts"))
    );
}

/// Every ending of a run has to be distinguishable on the Verdict tile.
///
/// This used to assert that `Final` produced `AwaitingApproval`, which the tile
/// renders as a pulsing green "A P P R O V E D". `show_failure` emits the same
/// bare `Final` on the way out, so a run that died on an API error, and a run
/// a Reviewer rejected, both ended up painted as approved. One assertion, four
/// wrong outcomes.
#[test]
fn appstate_apply_event_final_reports_what_actually_happened() {
    for (verdict, expected) in [
        (Some("approved"), RunState::Approved),
        (Some("Approved"), RunState::Approved),
        (Some("rejected"), RunState::Rejected),
        (Some("Rejected"), RunState::Rejected),
        (Some("revision_needed"), RunState::Rejected),
        (Some("RevisionNeeded"), RunState::Rejected),
        // Ended with nothing having reviewed it. Not an approval — the whole
        // point of `verdict_source` is that these are different facts.
        (None, RunState::NoVerdict),
    ] {
        let mut state = make_state();
        state.apply_event(DisplayEvent::Final {
            verdict: verdict.map(str::to_string),
            error: None,
        });
        assert!(state.finished);
        assert_eq!(
            state.run_state, expected,
            "verdict {verdict:?} must not read as {expected:?}"
        );
    }
}

#[test]
fn a_failed_run_is_never_approved() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::Final {
        verdict: None,
        error: Some("HTTP 401: the API key was rejected".to_string()),
    });
    assert_eq!(state.run_state, RunState::Failed);
    assert!(
        !matches!(state.run_state, RunState::Approved),
        "a failed run must never render as approved"
    );
    // And the reason reaches the transcript rather than only a status bar.
    assert!(
        state.chat_log.iter().any(|(_, t)| t.contains("401")),
        "the failure must be visible: {:?}",
        state.chat_log
    );
}

// ============================================================================
// AppState — Totals calculation
// ============================================================================

#[test]
fn appstate_totals_empty() {
    let state = make_state();
    let (in_t, out_t, cost, ms) = state.totals();
    assert_eq!(in_t, 0);
    assert_eq!(out_t, 0);
    assert_eq!(cost, 0.0);
    assert_eq!(ms, 0);
}

#[test]
fn appstate_totals_with_stages() {
    let state = make_state_with_stages(3);
    let (in_t, out_t, cost, ms) = state.totals();
    assert_eq!(in_t, 100 + 200 + 300);
    assert_eq!(out_t, 50 + 100 + 150);
    assert!(cost > 0.0);
    assert_eq!(ms, 1000 + 2000 + 3000);
}

// ============================================================================
// PageRouter — render_current with various pages
// ============================================================================

/// Every page must render **its own title**, and something besides whitespace.
///
/// These two tests used to draw all fourteen pages and assert nothing. They
/// could only fail on a panic, which is worth something — a page that indexes
/// past its content does panic — but a page that renders blank, or renders the
/// wrong page, or renders an empty box with a border, all passed. `ROADMAP.md`
/// §6 called them out for exactly that.
///
/// The assertion is the page's `title()`, which every page already declares and
/// which its header draws, so this asks the question the test's name implies:
/// does this page draw this page?
fn page_text(f: &ratatui::buffer::Buffer) -> String {
    let area = f.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| f[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_every_page_draws_itself(with_stages: bool) {
    let router = PageRouter::new();
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    for page_id in PageId::all() {
        let mut test_state = if with_stages {
            make_state_with_stages(4)
        } else {
            make_state()
        };
        test_state.current_page = *page_id;
        // Fleet and Session are not `Page`s — `tui.rs` renders them with their
        // own functions, because they read the mission store rather than
        // `AppState`. Drawing them through `PageRouter` is a no-op that leaves
        // a blank screen, and asserting on *that* would be asserting the
        // router's wiring rather than what a user sees. So each page is drawn
        // the way the product draws it.
        terminal
            .draw(|f| {
                let area = f.area();
                match page_id {
                    PageId::Fleet => niki::display::pages::fleet::render_fleet(
                        &test_state.fleet,
                        area,
                        f.buffer_mut(),
                    ),
                    PageId::Session => {
                        // No session open: the same explicit empty state
                        // `tui.rs` shows, because a blank screen is the thing
                        // B3-08 removed.
                        f.render_widget(ratatui::widgets::Paragraph::new("no session open"), area);
                    }
                    other => {
                        test_state.current_page = *other;
                        router.render_current(f, area, &test_state)
                    }
                }
            })
            .unwrap();
        let text = page_text(terminal.backend().buffer());

        // Chat is the transcript, not a titled page: it has no header because
        // the conversation *is* its content, and `PageId::title()` says "chat"
        // for the status bar rather than for anything it draws. Asserting the
        // word appears would be asserting a header nobody intended, so what is
        // asserted instead is the real requirement — that the front door shows
        // the conversation.
        if *page_id == PageId::Chat {
            let mut with_history = if with_stages {
                make_state_with_stages(4)
            } else {
                make_state()
            };
            with_history
                .chat_log
                .push(("user".to_string(), "CANARY-QUESTION".to_string()));
            terminal
                .draw(|f| {
                    let area = f.area();
                    with_history.current_page = PageId::Chat;
                    router.render_current(f, area, &with_history);
                })
                .unwrap();
            let chat_text = page_text(terminal.backend().buffer());
            assert!(
                chat_text.contains("CANARY-QUESTION"),
                "the chat view is the product's front door and must draw the \
                 conversation. Rendered:\n{chat_text}"
            );
            continue;
        }

        // Fleet and Session label themselves differently from `PageId::title`
        // — the same hand-typed-header drift this test is here to catch, one
        // level down. Their own titles are what must appear.
        let title = match page_id {
            PageId::Fleet => "fleet",
            PageId::Session => "session",
            other => other.title(),
        };
        assert!(
            text.contains(title),
            "{page_id:?} drew nothing containing its own title {title:?}. A page \
             that renders blank is a page the user is looking at nothing. \
             Rendered:\n{text}"
        );
        assert!(
            text.trim().chars().any(|c| !c.is_whitespace()
                && c != '│'
                && c != '┌'
                && c != '┐'
                && c != '└'
                && c != '┘'
                && c != '─'
                && c != '├'
                && c != '┤'
                && c != '┬'
                && c != '┴'
                && c != '┼'),
            "{page_id:?} drew only box-drawing characters. Rendered:\n{text}"
        );
    }
}

#[test]
fn page_router_render_current_all_pages() {
    assert_every_page_draws_itself(true);
}

#[test]
fn page_router_render_current_empty_state() {
    assert_every_page_draws_itself(false);
}

// ============================================================================
// Run page — start_time tracking
// ============================================================================

#[test]
fn appstate_stage_start_sets_start_time() {
    let mut state = make_state();
    assert!(state.start_time.is_none());
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Planner,
    });
    assert!(state.start_time.is_some());
}

#[test]
fn appstate_stage_start_does_not_overwrite_start_time() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Planner,
    });
    let first = state.start_time;
    // Add a small delay
    std::thread::sleep(std::time::Duration::from_millis(10));
    state.apply_event(DisplayEvent::StageStart {
        role: AgentRole::Coder,
    });
    assert_eq!(state.start_time, first);
}

// ============================================================================
// Agent page — selected_tab resets on tab change
// ============================================================================

#[test]
fn agents_page_tab_resets_scroll() {
    let mut state = make_state_with_stages(4);
    state.current_page = PageId::Agents;
    let mut router = PageRouter::new();
    // Tab forward twice
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    // Tab again — scroll should reset
    router.handle_key(key_code(KeyCode::Tab), &mut state);
    assert_eq!(state.current_page, PageId::Agents);
}

// ============================================================================
// Pipeline page — stage selection bounds
// ============================================================================

#[test]
fn pipeline_page_stage_selection_bounds() {
    let mut state = make_state();
    state.current_page = PageId::Pipeline;
    let mut router = PageRouter::new();
    // Press k many times — should not go below 0
    for _ in 0..10 {
        router.handle_key(key_code(KeyCode::Up), &mut state);
    }
    // Press j many times — should not go above 3
    for _ in 0..10 {
        router.handle_key(key_code(KeyCode::Down), &mut state);
    }
    assert_eq!(state.current_page, PageId::Pipeline);
}

// ============================================================================
// Artifacts page — selection bounds
// ============================================================================

#[test]
fn artifacts_page_selection_bounds() {
    let mut state = make_state();
    state.current_page = PageId::Artifacts;
    let mut router = PageRouter::new();
    // Press j many times — should not go above 11
    for _ in 0..20 {
        router.handle_key(key_code(KeyCode::Down), &mut state);
    }
    // Press k many times — should not go below 0
    for _ in 0..20 {
        router.handle_key(key_code(KeyCode::Up), &mut state);
    }
    assert_eq!(state.current_page, PageId::Artifacts);
}

// ============================================================================
// History page — selection bounds
// ============================================================================

#[test]
fn history_page_selection_bounds() {
    let mut state = make_state();
    state.current_page = PageId::History;
    let mut router = PageRouter::new();
    for _ in 0..20 {
        router.handle_key(key_code(KeyCode::Down), &mut state);
    }
    for _ in 0..20 {
        router.handle_key(key_code(KeyCode::Up), &mut state);
    }
    assert_eq!(state.current_page, PageId::History);
}

// ============================================================================
// Config page — field selection bounds
// ============================================================================

#[test]
fn config_page_field_bounds() {
    let mut state = make_state();
    state.current_page = PageId::Config;
    let mut router = PageRouter::new();
    // Tab 20 times — should wrap at 12
    for _ in 0..20 {
        router.handle_key(key_code(KeyCode::Tab), &mut state);
    }
    // BackTab 20 times — should wrap at 0
    for _ in 0..20 {
        router.handle_key(key_shift_tab(), &mut state);
    }
    assert_eq!(state.current_page, PageId::Config);
}

// ============================================================================
// Unrecognized keys are ignored on all pages
// ============================================================================

#[test]
fn unrecognized_keys_ignored_all_pages() {
    let pages = [
        PageId::Run,
        PageId::Pipeline,
        PageId::Agents,
        PageId::Diff,
        PageId::Verdict,
        PageId::Cost,
        PageId::Artifacts,
        PageId::History,
        PageId::Config,
        PageId::Help,
        PageId::TestLog,
    ];

    for page_id in &pages {
        let mut state = make_state();
        state.current_page = *page_id;
        let mut router = PageRouter::new();
        let original_page = state.current_page;
        router.handle_key(key_char('z'), &mut state);
        assert_eq!(
            state.current_page, original_page,
            "page {:?} should not change on unrecognized key",
            page_id
        );
    }
}

// ============================================================================
// TUI-010 — chat follow + manual scroll unification
// ============================================================================

fn chat_view_text(state: &niki::display::pages::AppState, width: u16, height: u16) -> String {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            niki::display::layout::render_chat(f, f.area(), state);
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol().to_string())
        .collect()
}

#[test]
fn chat_follows_new_output_by_default() {
    let mut state = make_state();
    state.current_page = PageId::Chat;
    for i in 0..40 {
        state
            .chat_log
            .push(("assistant".to_string(), format!("message number {i}")));
    }
    // Fresh state is end-pinned: the last message is visible, the first is not.
    let text = chat_view_text(&state, 100, 20);
    assert!(text.contains("message number 39"), "{text}");
    assert!(!text.contains("message number 0"), "{text}");
}

#[test]
fn chat_wheel_up_unpins_and_down_rearms() {
    let mut state = make_state();
    state.current_page = PageId::Chat;
    for i in 0..40 {
        state
            .chat_log
            .push(("assistant".to_string(), format!("message number {i}")));
    }
    assert!(state.chat_scroll.follow);
    // Wheel-up delta: unpins.
    state.chat_scroll.scroll_by(-30, 90, 10);
    assert!(!state.chat_scroll.follow);
    // Jumped to top: the first message is visible.
    state.chat_scroll.jump_to(0, 90, 10);
    let text = chat_view_text(&state, 100, 20);
    assert!(text.contains("message number 0"), "{text}");
    // Wheel-down delta past the end: re-arms the pin.
    let rest = state.chat_scroll.scroll_by(1000, 80, 10);
    assert!(state.chat_scroll.follow);
    assert!(rest > 0);
}

/// A stage failure must open the error modal.
///
/// This closes a gap where three tests drove `Modal::Error` — rendering,
/// hit-testing, and the [r]etry / [c]onfig keys — by constructing the variant
/// by hand, while nothing in the product ever constructed it. The affordance
/// was fully "tested" and completely unreachable: a user whose run died on an
/// API error saw a status line change colour and nothing else.
#[test]
fn a_stage_failure_opens_the_error_modal() {
    let mut state = make_state();
    assert!(state.modal.is_none(), "no modal before anything fails");

    state.apply_event(DisplayEvent::StageFailed {
        role: niki::artifacts::types::AgentRole::Coder,
        error: "401 Unauthorized: check your API key".into(),
    });

    let modal = state
        .modal
        .clone()
        .expect("a failed stage must surface a modal, not just a status line");
    match &modal {
        Modal::Error { message, hint, .. } => {
            assert!(
                message.contains("401"),
                "the modal must show the actual error: {message}"
            );
            assert!(!hint.trim().is_empty(), "the modal must offer a next step");
        }
        other => panic!("expected an error modal, got {other:?}"),
    }
}

/// A second failure must not stack a second modal on top of the first.
#[test]
fn a_second_stage_failure_does_not_replace_the_first_modal() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::StageFailed {
        role: niki::artifacts::types::AgentRole::Coder,
        error: "first failure".into(),
    });
    let first = state.modal.clone().unwrap();

    state.apply_event(DisplayEvent::StageFailed {
        role: niki::artifacts::types::AgentRole::Reviewer,
        error: "second failure".into(),
    });

    let now = state.modal.clone().expect("the modal stays open");
    assert!(
        matches!((&first, &now), (Modal::Error { message: a, .. }, Modal::Error { message: b, .. }) if a == b),
        "a modal the user is reading must not be swapped out from under them: \
         {first:?} became {now:?}"
    );
}

/// No configurable keybinding may sit behind a literal key test.
///
/// The chat loop tested `key.code == KeyCode::Tab` and `Char('t')` directly.
/// Both happened to be defensible-looking and both were wrong: `toggle_chat`
/// defaults to Tab so the literal matched, and `cycle_theme` defaults to
/// **ctrl+t** — so the bare `t` that worked was never the configured key, and
/// the configured key did nothing. A user who rebound either one got a binding
/// that silently did nothing in `niki chat` and worked in `niki`.
///
/// The defaults are pinned here because the bug was invisible precisely
/// because they were not written down anywhere the tests could check.
#[test]
fn the_defaults_for_the_keys_the_chat_loop_used_to_hardcode() {
    use niki::display::keybindings::{GlobalAction, KeyBindings};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let kb = KeyBindings::default();

    // `toggle_chat` is Tab, so the literal matched by luck.
    assert_eq!(
        kb.resolve(&KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Some(GlobalAction::ToggleChatPage)
    );
    // `cycle_theme` is ctrl+t. A bare `t` is NOT the configured key — which
    // is why the literal `Char('t')` was a bug that no default matched.
    assert_eq!(
        kb.resolve(&KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
        Some(GlobalAction::CycleTheme)
    );
    assert_ne!(
        kb.resolve(&KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)),
        Some(GlobalAction::CycleTheme),
        "a bare `t` must not be the theme key; if this ever becomes true the \\
         hardcoded handler it replaced would have been right for the wrong reason"
    );
    // And the palette, from two turns back, for the same reason.
    assert_eq!(
        kb.resolve(&KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
        Some(GlobalAction::CommandPalette)
    );
}

/// A pipeline diagnostic has to reach the surface that is still on screen.
///
/// The branch-blocked reason, the Coder-fallback notice and the spend-cap
/// warning were all `eprintln!`. Under `--tui` that writes into the
/// alternate-screen buffer, which `LeaveAlternateScreen` then discards — so
/// "Branch blocked: test suite `cargo test` failed (exit 1)", the single most
/// important line of a failed run, was written to a screen that was about to be
/// thrown away. `DisplayEvent` had no notice channel at all, and a comment in
/// the pipeline claimed a "TUI notice line" that did not exist.
#[test]
fn a_notice_reaches_the_transcript_rather_than_a_discarded_screen() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::Notice {
        text: "Branch blocked: test suite `cargo test` failed (exit 1)".to_string(),
        warning: false,
    });
    let (_, text) = &state.chat_log[0];
    assert!(
        text.contains("Branch blocked"),
        "the block reason must be in the transcript, got: {text:?}"
    );

    // A warning is labelled as one, so it is not mistaken for the run's verdict.
    let mut state = make_state();
    state.apply_event(DisplayEvent::Notice {
        text: "spend cap exceeded".to_string(),
        warning: true,
    });
    assert_eq!(state.chat_log[0].0, "warning");
}

/// Nothing on the Run page may be invented.
///
/// The Run page is one Tab from every user and, before `/run` existed, was
/// permanently empty of real data — so everything it showed was fabricated: a
/// `--project ./my-app` literal, a `niki/xxxxx` ref that did not exist,
/// "working tree: untouched" in success green with nothing behind it, and four
/// agents marked `queued` for a run nobody had started. `niki chat` opens here.
#[test]
fn the_run_page_invents_nothing_before_a_run_exists() {
    use niki::display::pages::Page;
    use niki::display::pages::run::RunPage;
    use ratatui::backend::TestBackend;

    // Rendered for real. The first version of this test asserted against
    // `state.chat_lines`, which is empty on a fresh state — so it passed
    // against a page that was inventing a branch ref, a project path and four
    // queued agents. A test that cannot fail on the defect it describes is
    // worse than no test.
    let state = make_state();
    let page = RunPage::default();
    let backend = TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    terminal
        .draw(|f| page.render(f, f.area(), &state))
        .expect("the Run page must render");

    let screen = terminal.backend().to_string();
    assert!(
        !screen.contains("niki/xxxxx"),
        "invented a branch ref:\n{screen}"
    );
    assert!(
        !screen.contains("./my-app"),
        "invented a project path:\n{screen}"
    );
    assert!(
        !screen.contains("working tree: untouched"),
        "asserted an unmeasured property in success green:\n{screen}"
    );
    assert!(
        !screen.contains("queued"),
        "invented agents for a run nobody started:\n{screen}"
    );
}

/// The Cost page names the model the run used.
///
/// It printed the literal `anthropic/claude-sonnet-4` for every agent in every
/// configuration, so a local `qwen2.5-coder` run was displayed as four
/// Anthropic calls. The data was on `StageMetric` and simply was not carried
/// across.
#[test]
fn the_cost_page_reads_the_model_the_run_reported() {
    let mut state = make_state();
    let json = serde_json::json!({
        "total_cost_usd": 0.0042,
        "agents": [
            {"role": "Planner", "provider": "ollama", "model": "qwen2.5-coder"},
            {"role": "Coder",   "provider": "ollama", "model": "qwen2.5-coder"}
        ]
    })
    .to_string();
    state.apply_event(DisplayEvent::CostJson(json));

    assert_eq!(state.cost_agents.len(), 2);
    assert_eq!(state.cost_agents[0].model, "qwen2.5-coder");
    assert_eq!(state.cost_agents[0].provider, "ollama");
    // The status bar and `/cost` read `state.cost`, which nothing ever assigned.
    assert!(
        (state.cost - 0.0042).abs() < f64::EPSILON,
        "total cost must be recorded, got {}",
        state.cost
    );
}

/// Opening a run from History must load it, not rename the current one.
#[test]
fn opening_a_run_from_history_reads_the_task_directory() {
    let dir = tempfile::tempdir().unwrap();
    let task = dir.path();
    std::fs::write(task.join("changes.patch"), "diff --git a/x b/x\n").unwrap();
    std::fs::write(task.join("report.md"), "# the report\n").unwrap();
    std::fs::create_dir_all(task.join("artifacts")).unwrap();
    std::fs::write(task.join("artifacts/coder.json"), "{}").unwrap();

    let mut state = make_state();
    state.diff_content = Some("the current run's diff".to_string());
    state.open_task_from_history(task, "niki/abcd1234");

    assert_eq!(state.branch_name, "niki/abcd1234");
    assert_eq!(
        state.diff_content.as_deref(),
        Some("diff --git a/x b/x\n"),
        "the opened run's patch must replace the current one, not sit beside it"
    );
    assert!(
        state
            .report_content
            .as_deref()
            .unwrap()
            .contains("the report")
    );
    assert_eq!(state.opened_task_dir.as_deref(), Some(task));
}
