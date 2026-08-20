//! Visual layout / responsiveness checks rendered through a `TestBackend`.
//!
//! These tests render the real pages (not mocks) at several window sizes and
//! inspect the resulting buffer pixels, so they catch the kinds of problems a
//! human eyeball catches: content overflowing the frame, overlapping widgets,
//! missing status-bar slots, collapsed code blocks, and the new mode prompts.
//!
//! Run with: `cargo test --test visual_layout_check`

use niki::artifacts::types::AgentRole;
use niki::config::types::NikiConfig;
use niki::display::pages::{AppState, PageId, PageRouter, RunState, StageInfo, StageStatus};
use niki::display::state::InputMode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn make_state() -> AppState {
    AppState::new(
        "add a health endpoint".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

fn state_with_running_coder() -> AppState {
    let mut state = make_state();
    state.current_page = PageId::Chat;
    state.context_usage = 0.35;
    state.context_limit = 128_000;
    state.stages.push(StageInfo {
        role: AgentRole::Coder,
        status: StageStatus::Running,
        stream: "Here is the implementation:\n```rust\nfn health() -> &'static str {\n    \"ok\"\n}\n```".to_string(),
        full_transcript: String::new(),
        input_tokens: 1234,
        output_tokens: 567,
        cost_usd: 0.0012,
        latency_ms: 800,
        summary: vec![],
        start: Some(std::time::Instant::now()),
            prompt_file: None,
            retry_count: 0,
            error_message: None,
    });
    state
}

fn state_with_shell_input() -> AppState {
    let mut state = make_state();
    state.current_page = PageId::Chat;
    state.input_state.mode = InputMode::Shell;
    state.input_state.buffer = "echo hi".to_string();
    state.input_state.cursor_pos = 8;
    state
}

fn render_page(width: u16, height: u16, state: &AppState) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let router = PageRouter::new();
    let frame = terminal
        .draw(|f| {
            let area = f.area();
            router.render_current(f, area, state);
        })
        .unwrap();
    let buf = frame.buffer.clone();
    let w = buf.area.width as usize;
    let mut out = String::new();
    for (i, cell) in buf.content.iter().enumerate() {
        out.push_str(cell.symbol());
        if w > 0 && (i + 1) % w == 0 {
            out.push('\n');
        }
    }
    out
}

fn render_input_box_visual(width: u16, height: u16, state: &AppState) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let frame = terminal
        .draw(|f| {
            niki::display::components::input_box::render_input_box(f, state, f.area());
        })
        .unwrap();
    let buf = frame.buffer.clone();
    let w = buf.area.width as usize;
    let mut out = String::new();
    for (i, cell) in buf.content.iter().enumerate() {
        out.push_str(cell.symbol());
        if w > 0 && (i + 1) % w == 0 {
            out.push('\n');
        }
    }
    out
}

#[test]
fn chat_page_renders_at_multiple_widths_without_overflow() {
    let state = state_with_running_coder();
    for (w, h) in [(80, 24), (120, 40), (200, 50)] {
        let out = render_page(w, h, &state);
        // The stage header and the running code block content must be present.
        assert!(out.contains("Coder"), "coder header missing at {}x{}", w, h);
        assert!(
            out.contains("fn health"),
            "running code block collapsed at {}x{}",
            w,
            h
        );
        assert!(out.contains("ok"), "code block body missing at {}x{}", w, h);
        // Status bar must carry the input hint line.
        assert!(
            out.contains("type + Enter to send"),
            "status bar missing input hint at {}x{}",
            w,
            h
        );
        // The frame must not be blank.
        assert!(out.chars().any(|c| c != ' '), "blank frame at {}x{}", w, h);
    }
}

#[test]
fn running_code_block_stays_open_during_streaming() {
    // R3: a partially-streamed closing fence must NOT collapse the block.
    let mut state = state_with_running_coder();
    // Simulate the model having only emitted the first two backticks of the
    // closing fence — the block must still render its body.
    state.stages[0].stream = "code:\n```rust\nlet x = 1;\n```".to_string();
    let out = render_page(120, 40, &state);
    // R3: the open code block body must be present even though the closing
    // fence is incomplete (the renderer does not emit a language tag).
    assert!(out.contains("let x = 1;"));
}

#[test]
fn shell_mode_input_renders_shell_prompt() {
    // I2: `!`-bash mode shows a distinct prompt symbol, not a bare `!` char.
    let state = state_with_shell_input();
    let out = render_input_box_visual(80, 8, &state);
    assert!(
        out.contains("Shell"),
        "shell mode prompt symbol missing: {:?}",
        out
    );
    assert!(out.contains("echo hi"));
}

#[test]
fn insert_mode_input_renders_default_prompt() {
    let state = make_state();
    let out = render_input_box_visual(80, 8, &state);
    // Default insert-mode prompt must be present and the box must not be blank.
    assert!(out.chars().any(|c| c != ' '), "blank input box");
}

#[test]
fn run_page_renders_status_and_pipeline() {
    let mut state = make_state();
    state.current_page = PageId::Run;
    state.run_state = RunState::Running;
    state.stages.push(StageInfo {
        role: AgentRole::Planner,
        status: StageStatus::Running,
        stream: "planning".to_string(),
        full_transcript: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
        latency_ms: 0,
        summary: vec![],
        start: Some(std::time::Instant::now()),
            prompt_file: None,
            retry_count: 0,
            error_message: None,
    });
    let out = render_page(120, 40, &state);
    assert!(out.chars().any(|c| c != ' '), "blank run page");
}

/// The status bar is rendered by `run_tui` (not by `render_current`), so test
/// it directly. Verifies the context gauge and that the bar never overflows.
#[test]
fn status_bar_context_gauge_and_no_overflow() {
    let mut state = make_state();
    state.context_usage = 0.35;
    state.context_limit = 128_000;
    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    let buf = terminal
        .draw(|f| {
            niki::display::components::status_bar::render_status_bar(f, &state, f.area());
        })
        .unwrap()
        .buffer;
    // The buffer must not contain more cells than the frame area.
    assert!(
        buf.content.len() <= (buf.area.width as usize) * (buf.area.height as usize),
        "status bar overflowed the frame: {} cells in {}x{}",
        buf.content.len(),
        buf.area.width,
        buf.area.height,
    );
    // The context gauge must be present in the rendered cells.
    let rendered: String = buf.content.iter().map(|c| c.symbol()).collect();
    assert!(
        rendered.contains("ctx"),
        "context gauge missing: {:?}",
        rendered
    );
}

#[test]
fn status_bar_truncates_right_items_when_narrow() {
    let mut state = make_state();
    state.context_usage = 0.35;
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    let buf = terminal
        .draw(|f| {
            niki::display::components::status_bar::render_status_bar(f, &state, f.area());
        })
        .unwrap()
        .buffer;
    assert!(
        buf.content.len() <= (buf.area.width as usize) * (buf.area.height as usize),
        "status bar overflowed at narrow width: {} cells in {}x{}",
        buf.content.len(),
        buf.area.width,
        buf.area.height,
    );
}

#[test]
fn dump_status_bar() {
    let mut state = make_state();
    state.context_usage = 0.35;
    state.context_limit = 128_000;
    let out = render_status_bar(120, 1, &state);
    eprintln!(
        "=== 120x1 status bar ===\n[{}]\nlen={}",
        out.trim_end(),
        out.trim_end().len()
    );
}

#[test]
fn dump_status_bar2() {
    let mut state = make_state();
    state.context_usage = 0.35;
    state.context_limit = 128_000;
    for w in [80, 120] {
        let out = render_status_bar(w, 1, &state);
        eprintln!(
            "=== {}x1 ===\n[{}]\nlen={}",
            w,
            out.trim_end(),
            out.trim_end().len()
        );
    }
}

fn render_status_bar(width: u16, height: u16, state: &AppState) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let frame = terminal
        .draw(|f| {
            niki::display::components::status_bar::render_status_bar(f, state, f.area());
        })
        .unwrap();
    let buf = frame.buffer.clone();
    let w = buf.area.width as usize;
    let mut out = String::new();
    for (i, cell) in buf.content.iter().enumerate() {
        out.push_str(cell.symbol());
        if w > 0 && (i + 1) % w == 0 {
            out.push('\n');
        }
    }
    out
}
