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
}

// ============================================================================
// RUN PAGE — Navigation from Run to all other pages
// (Run page no longer handles direct hotkey navigation — use Ctrl-P command palette instead)
// ============================================================================

#[test]
fn run_page_ignores_navigation_hotkeys() {
    let mut state = make_state();
    let mut router = PageRouter::new();
    // All these keys should be ignored on Run page now
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

#[test]
fn appstate_apply_event_final() {
    let mut state = make_state();
    state.apply_event(DisplayEvent::Final);
    assert!(state.finished);
    assert_eq!(state.run_state, RunState::AwaitingApproval);
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

#[test]
fn page_router_render_current_all_pages() {
    let router = PageRouter::new();

    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    for page_id in PageId::all() {
        let mut test_state = make_state_with_stages(4);
        test_state.current_page = *page_id;
        terminal
            .draw(|f| {
                let area = f.area();
                router.render_current(f, area, &test_state);
            })
            .unwrap();
    }
}

#[test]
fn page_router_render_current_empty_state() {
    let router = PageRouter::new();

    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    for page_id in PageId::all() {
        let mut test_state = make_state();
        test_state.current_page = *page_id;
        terminal
            .draw(|f| {
                let area = f.area();
                router.render_current(f, area, &test_state);
            })
            .unwrap();
    }
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
