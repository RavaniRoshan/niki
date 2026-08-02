// Shared test helpers for the NIKI UI test suite.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use niki::artifacts::types::AgentRole;
use niki::config::NikiConfig;
use niki::display::pages::{AppState, PageId, PageRouter, RunState, StageInfo, StageStatus};

// ── AppState builder ────────────────────────────────────────────────────

pub fn test_state() -> AppState {
    AppState::new(
        "test task".into(),
        NikiConfig::default(),
        PathBuf::from("/tmp/niki-test"),
    )
}

pub fn state_with_stages() -> AppState {
    let mut s = test_state();
    s.stages = vec![
        make_stage(AgentRole::Planner, StageStatus::Done, 100, 200, 0.005),
        make_stage(AgentRole::Coder, StageStatus::Running, 300, 500, 0.01),
        make_stage(AgentRole::Tester, StageStatus::Queued, 0, 0, 0.0),
        make_stage(AgentRole::Reviewer, StageStatus::Queued, 0, 0, 0.0),
    ];
    s
}

pub fn state_finished() -> AppState {
    let mut s = test_state();
    s.stages = vec![
        make_stage(AgentRole::Planner, StageStatus::Done, 1200, 800, 0.01),
        make_stage(AgentRole::Coder, StageStatus::Done, 3000, 2500, 0.05),
        make_stage(AgentRole::Tester, StageStatus::Done, 1500, 400, 0.02),
        make_stage(AgentRole::Reviewer, StageStatus::Done, 800, 300, 0.01),
    ];
    s.finished = true;
    s.run_state = RunState::AwaitingApproval;
    s.branch_name = "niki/a7f3c2".into();
    s.report_content = Some("# Report\n\nAll tests passed.".into());
    s.diff_content = Some("+added line\n-removed line".into());
    s
}

pub fn make_stage(
    role: AgentRole,
    status: StageStatus,
    input_tokens: u32,
    output_tokens: u32,
    cost_usd: f64,
) -> StageInfo {
    StageInfo {
        role,
        status,
        stream: String::new(),
        full_transcript: "sample transcript".into(),
        input_tokens,
        output_tokens,
        cost_usd,
        latency_ms: 1000,
        summary: vec!["done".into()],
        start: Some(std::time::Instant::now()),
    }
}

// ── Key simulation ──────────────────────────────────────────────────────

pub fn key_char(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
        kind: ratatui::crossterm::event::KeyEventKind::Press,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    }
}

pub fn key_code(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: ratatui::crossterm::event::KeyEventKind::Press,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    }
}

// ── Page navigation helper ──────────────────────────────────────────────

pub fn press(router: &mut PageRouter, state: &mut AppState, key: KeyEvent) -> bool {
    router.handle_key(key, state)
}

pub fn goto_page_from_run(router: &mut PageRouter, state: &mut AppState, target: PageId) {
    let key = match target {
        PageId::Pipeline => key_char('p'),
        PageId::Agents => key_char('a'),
        PageId::Diff => key_char('d'),
        PageId::Verdict => key_char('v'),
        PageId::Cost => key_char('c'),
        PageId::Artifacts => key_char('f'),
        PageId::History => key_char('h'),
        PageId::Config => key_char(','),
        PageId::Help => key_char('?'),
        PageId::Run => return,
    };
    state.current_page = PageId::Run;
    let _ = press(router, state, key);
}

// ── Rendering helpers ───────────────────────────────────────────────────

fn render_full_tui(frame: &mut Frame, area: Rect, state: &AppState, router: &PageRouter) {
    if area.height < 10 {
        return;
    }
    let bg_block =
        ratatui::widgets::Block::default().style(Style::default().bg(niki::display::theme::BG));
    frame.render_widget(bg_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    niki::display::logo::render_logo(frame, chunks[0]);
    router.render_current(frame, chunks[1], state);

    let (tot_in, tot_out, tot_cost, _ms) = state.totals();
    let cost_str = if tot_cost > 0.0 {
        format!(" ${:.4}", tot_cost)
    } else {
        String::new()
    };
    let status = Line::from(vec![
        Span::styled(
            " niki ",
            Style::default()
                .fg(niki::display::theme::BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {}/{} tok{}", tot_in, tot_out, cost_str),
            Style::default().fg(niki::display::theme::FG_DIM),
        ),
        Span::styled(
            format!(" · {} ", state.current_page.title()),
            Style::default().fg(niki::display::theme::BORDER_ACTIVE),
        ),
    ]);
    frame.render_widget(Paragraph::new(status), chunks[2]);

    if let Some(ref modal) = state.modal {
        niki::display::modal::render_modal(frame, modal, area);
    }
}

pub fn render_full(state: &AppState) -> Terminal<TestBackend> {
    let backend = TestBackend::new(120, 42);
    let mut terminal = Terminal::new(backend).unwrap();
    let router = PageRouter::new();
    terminal
        .draw(|f| render_full_tui(f, f.area(), state, &router))
        .unwrap();
    terminal
}

pub fn assert_row_contains(buffer: &ratatui::buffer::Buffer, y: u16, text: &str) {
    let row: String = (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect();
    assert!(
        row.contains(text),
        "row {y} does not contain '{text}': got '{row}'"
    );
}

pub fn assert_buffer_contains(buffer: &ratatui::buffer::Buffer, text: &str) {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect();
        if row.contains(text) {
            return;
        }
    }
    panic!("buffer does not contain '{text}' anywhere");
}
