//! AppState state machine tests.

use niki::display::pages::{PageId, RunState, StageStatus};
use niki::artifacts::types::AgentRole;
use niki::display::tui::DisplayEvent;

mod helpers;
use helpers::*;

// ═══════════════════════════════════════════════════════════════════════════
// AppState::new
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn new_state_defaults() {
    let state = test_state();
    assert_eq!(state.current_page, PageId::Run);
    assert_eq!(state.run_state, RunState::Idle);
    assert!(state.task_id.is_none());
    assert_eq!(state.description, "test task");
    assert!(state.stages.is_empty());
    assert!(!state.finished);
    assert!(!state.paused);
    assert!(state.modal.is_none());
    assert!(state.start_time.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: StageStart
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stage_start_adds_stage_and_sets_running() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    assert_eq!(state.stages.len(), 1);
    assert_eq!(state.stages[0].role, AgentRole::Planner);
    assert_eq!(state.stages[0].status, StageStatus::Running);
    assert_eq!(state.run_state, RunState::Running);
    assert!(state.start_time.is_some());
}

#[test]
fn stage_start_does_not_overwrite_existing_start_time() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    let first = state.start_time;
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Coder });
    assert_eq!(state.start_time, first);
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: StageToken
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stage_token_appends_to_stream() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    state.apply_event(DisplayEvent::StageToken { role: AgentRole::Planner, token: "hello ".into() });
    state.apply_event(DisplayEvent::StageToken { role: AgentRole::Planner, token: "world".into() });
    assert_eq!(state.stages[0].stream, "hello world");
    assert_eq!(state.stages[0].full_transcript, "hello world");
}

#[test]
fn stage_token_truncates_stream_at_2000() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    let big = "x".repeat(2500);
    state.apply_event(DisplayEvent::StageToken { role: AgentRole::Planner, token: big });
    assert!(state.stages[0].stream.len() <= 2000);
    assert_eq!(state.stages[0].full_transcript.len(), 2500);
}

#[test]
fn stage_token_ignores_unknown_role() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    state.apply_event(DisplayEvent::StageToken { role: AgentRole::Coder, token: "test".into() });
    assert_eq!(state.stages[0].stream, "");
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: StageDone
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stage_done_marks_done_and_clears_stream() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    state.apply_event(DisplayEvent::StageToken { role: AgentRole::Planner, token: "work".into() });
    state.apply_event(DisplayEvent::StageDone {
        role: AgentRole::Planner,
        summary: vec!["spec created".into()],
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.01,
        latency_ms: 2000,
    });
    assert_eq!(state.stages[0].status, StageStatus::Done);
    assert_eq!(state.stages[0].summary, vec!["spec created"]);
    assert_eq!(state.stages[0].input_tokens, 1000);
    assert_eq!(state.stages[0].output_tokens, 500);
    assert!((state.stages[0].cost_usd - 0.01).abs() < f64::EPSILON);
    assert_eq!(state.stages[0].latency_ms, 2000);
    assert!(state.stages[0].stream.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: StageFailed
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stage_failed_marks_failed_and_sets_run_state() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Coder });
    state.apply_event(DisplayEvent::StageFailed {
        role: AgentRole::Coder,
        error: "compilation error".into(),
    });
    assert_eq!(state.stages[0].status, StageStatus::Failed);
    assert_eq!(state.stages[0].summary, vec!["compilation error"]);
    assert_eq!(state.run_state, RunState::Failed);
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: Revision
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn revision_updates_rounds_and_notes() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::Revision {
        round: 2,
        max: 3,
        issues: vec!["missing test".into(), "no error handling".into()],
    });
    assert_eq!(state.revision_round, 2);
    assert_eq!(state.max_revision_rounds, 3);
    assert_eq!(state.run_state, RunState::AwaitingReviewer);
    assert_eq!(state.notes.len(), 3);
    assert!(state.notes[0].0.contains("Revision 2 of 3"));
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: Final
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn final_sets_finished_and_awaiting_approval() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::Final);
    assert!(state.finished);
    assert_eq!(state.run_state, RunState::AwaitingApproval);
}

// ═══════════════════════════════════════════════════════════════════════════
// apply_event: Banner
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn banner_updates_description() {
    let mut state = test_state();
    state.apply_event(DisplayEvent::Banner { description: "new task".into() });
    assert_eq!(state.description, "new task");
}

// ═══════════════════════════════════════════════════════════════════════════
// totals()
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn totals_empty_stages() {
    let state = test_state();
    let (i, o, c, ms) = state.totals();
    assert_eq!(i, 0);
    assert_eq!(o, 0);
    assert!((c - 0.0).abs() < f64::EPSILON);
    assert_eq!(ms, 0);
}

#[test]
fn totals_sums_across_stages() {
    let state = state_finished();
    let (i, o, c, ms) = state.totals();
    assert_eq!(i, 1200 + 3000 + 1500 + 800);
    assert_eq!(o, 800 + 2500 + 400 + 300);
    assert!((c - 0.09).abs() < 0.001);
    assert!(ms > 0);
}

#[test]
fn totals_running_stage_included() {
    let state = state_with_stages();
    let (i, o, _, _) = state.totals();
    assert_eq!(i, 100 + 300);
    assert_eq!(o, 200 + 500);
}

// ═══════════════════════════════════════════════════════════════════════════
// active_stage()
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn active_stage_none_when_empty() {
    let state = test_state();
    assert!(state.active_stage().is_none());
}

#[test]
fn active_stage_none_when_all_done() {
    let state = state_finished();
    assert!(state.active_stage().is_none());
}

#[test]
fn active_stage_returns_running() {
    let state = state_with_stages();
    let active = state.active_stage();
    assert!(active.is_some());
    assert_eq!(active.unwrap().role, AgentRole::Coder);
}

#[test]
fn active_stage_first_running_when_multiple() {
    let mut state = test_state();
    state.stages = vec![
        make_stage(AgentRole::Planner, StageStatus::Running, 0, 0, 0.0),
        make_stage(AgentRole::Coder, StageStatus::Running, 0, 0, 0.0),
    ];
    let active = state.active_stage().unwrap();
    assert_eq!(active.role, AgentRole::Planner);
}

// ═══════════════════════════════════════════════════════════════════════════
// Full pipeline lifecycle
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn full_lifecycle_planner_to_final() {
    let mut state = test_state();

    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
    assert_eq!(state.run_state, RunState::Running);

    state.apply_event(DisplayEvent::StageToken { role: AgentRole::Planner, token: "plan...".into() });

    state.apply_event(DisplayEvent::StageDone {
        role: AgentRole::Planner,
        summary: vec!["spec: 2 files".into()],
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.005,
        latency_ms: 1500,
    });

    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Coder });
    state.apply_event(DisplayEvent::StageDone {
        role: AgentRole::Coder,
        summary: vec!["2 files changed".into()],
        input_tokens: 3000,
        output_tokens: 2000,
        cost_usd: 0.04,
        latency_ms: 8000,
    });

    state.apply_event(DisplayEvent::Revision {
        round: 2,
        max: 3,
        issues: vec!["missing edge case".into()],
    });
    assert_eq!(state.run_state, RunState::AwaitingReviewer);

    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Tester });
    state.apply_event(DisplayEvent::StageDone {
        role: AgentRole::Tester,
        summary: vec!["12 tests passed".into()],
        input_tokens: 1500,
        output_tokens: 400,
        cost_usd: 0.015,
        latency_ms: 4000,
    });

    state.apply_event(DisplayEvent::StageStart { role: AgentRole::Reviewer });
    state.apply_event(DisplayEvent::StageDone {
        role: AgentRole::Reviewer,
        summary: vec!["approved".into()],
        input_tokens: 800,
        output_tokens: 200,
        cost_usd: 0.008,
        latency_ms: 2000,
    });

    state.apply_event(DisplayEvent::Final);
    assert!(state.finished);
    assert_eq!(state.run_state, RunState::AwaitingApproval);
    assert_eq!(state.stages.len(), 4); // Planner + Coder + Tester + Reviewer

    let (i, o, c, _) = state.totals();
    assert!(i > 0);
    assert!(o > 0);
    assert!(c > 0.0);
}
