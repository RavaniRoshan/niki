//! Headless render-performance harness (TUI-000).
//!
//! Measures transcript build + full `TestBackend` render across the scenarios
//! in `docs/tui/pi-extraction-plan.md` §9, prints the numbers, and enforces
//! generous smoke budgets (2× expected — signal, not flakes).
//!
//! Run with: `cargo test --test tui_perf -- --nocapture` (timings print).
//! Absolute budgets live here; per-PR deltas are tracked by comparing output.

use std::time::{Duration, Instant};

use niki::artifacts::types::AgentRole;
use niki::config::types::NikiConfig;
use niki::display::layout::render_chat;
use niki::display::pages::chat::build_chat_lines;
use niki::display::pages::{AppState, StageInfo, StageStatus};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn make_state() -> AppState {
    AppState::new(
        "benchmark fixture task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

/// Transcript-heavy fixture: markdown-rich assistant turns (code fences hit
/// the streaming renderer) plus completed stages with summaries.
fn transcript_state(turns: usize, stages: usize) -> AppState {
    let mut state = make_state();
    let body = "Here is the change:\n```rust\nfn health() -> &'static str {\n    \"ok\"\n}\n```\nTests pass.";
    for i in 0..turns {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        state
            .chat_log
            .push((role.to_string(), format!("turn {i}\n{body}")));
    }
    let roles = [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Reviewer,
    ];
    for i in 0..stages {
        state.stages.push(StageInfo {
            role: roles[i % 4],
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: format!("transcript {i}\n{body}"),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.001,
            latency_ms: 500,
            summary: vec![format!("summary {i} line one"), "line two".to_string()],
            start: Some(Instant::now()),
            completed_at: Some(Instant::now()),
            prompt_file: None,
            retry_count: 0,
            error_message: None,
        });
    }
    state
}

fn render_once(state: &AppState, width: u16, height: u16) -> Duration {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let start = Instant::now();
    terminal.draw(|f| render_chat(f, f.area(), state)).unwrap();
    start.elapsed()
}

fn report(name: &str, elapsed: Duration, budget: Duration) {
    println!(
        "[tui_perf] {name}: {}ms (budget {}ms)",
        elapsed.as_millis(),
        budget.as_millis()
    );
    assert!(
        elapsed <= budget,
        "{name} exceeded smoke budget: {:?} > {:?}",
        elapsed,
        budget
    );
}

#[test]
fn perf_transcript_build_2k_lines() {
    let state = transcript_state(300, 20);
    let start = Instant::now();
    let lines = build_chat_lines(&state, 100, false);
    let elapsed = start.elapsed();
    println!("[tui_perf] built {} chat lines", lines.len());
    assert!(lines.len() > 2000, "fixture too small: {}", lines.len());
    report("transcript_build_2k", elapsed, Duration::from_millis(100));
}

#[test]
fn perf_full_render_chat() {
    let state = transcript_state(300, 20);
    // Warmup (theme/globals first touch).
    render_once(&state, 100, 40);
    let start = Instant::now();
    let iters = 20;
    for _ in 0..iters {
        render_once(&state, 100, 40);
    }
    let mean = start.elapsed() / iters;
    report(
        "full_render_chat_mean_x20",
        mean,
        Duration::from_millis(100),
    );
}

#[test]
fn perf_streaming_token_appends() {
    let mut state = transcript_state(50, 4);
    let start = Instant::now();
    for i in 0..50 {
        state
            .chat_log
            .push(("assistant".to_string(), format!("stream chunk {i}\n```")));
        let _ = build_chat_lines(&state, 100, false);
    }
    report(
        "streaming_50_appends_with_rebuild",
        start.elapsed(),
        Duration::from_millis(300),
    );
}

#[test]
fn perf_scroll_steps() {
    let state = transcript_state(300, 20);
    for offset in [0usize, 500, 1000] {
        let mut scrolled = transcript_state(0, 0);
        scrolled.chat_log = state.chat_log.clone();
        scrolled.stages = state.stages.clone();
        scrolled.description = state.description.clone();
        scrolled.chat_scroll.follow = false;
        scrolled.chat_scroll.offset = offset;
        let elapsed = render_once(&scrolled, 100, 40);
        report(
            &format!("scroll_render_offset_{offset}"),
            elapsed,
            Duration::from_millis(100),
        );
    }
}

#[test]
fn perf_expanded_stages_rebuild() {
    // TUI-004: 20 expanded stages re-rendered per frame (spinner tick defeats
    // the coarse hash). First build populates the memo; the next 19 must be
    // hash + row-clone only.
    use niki::display::pages::StageInfo;
    use niki::display::pages::StageStatus;
    let mut state = make_state();
    for i in 0..20 {
        state.stages.push(StageInfo {
            role: AgentRole::Coder,
            status: StageStatus::Done,
            stream: String::new(),
            full_transcript: format!(
                "transcript {i}\n```rust\nfn f{i}() {{}}\n```\nmore text here"
            ),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.001,
            latency_ms: 500,
            summary: vec![format!("summary {i}")],
            start: Some(Instant::now()),
            completed_at: Some(Instant::now()),
            prompt_file: None,
            retry_count: 0,
            error_message: None,
        });
        state.expanded_stages.insert(i);
    }
    let start = Instant::now();
    let first = build_chat_lines(&state, 100, false);
    let first_ms = start.elapsed();
    let start = Instant::now();
    for _ in 0..19 {
        let _ = build_chat_lines(&state, 100, false);
    }
    let rest_mean = start.elapsed() / 19;
    println!(
        "[tui_perf] expanded first build {} lines in {}ms, memoized mean {}ms",
        first.len(),
        first_ms.as_millis(),
        rest_mean.as_millis()
    );
    assert!(first.len() > 100);
    report(
        "expanded_rebuild_memoized_mean_x19",
        rest_mean,
        Duration::from_millis(100),
    );
}

#[test]
fn perf_resize_widths() {
    let state = transcript_state(300, 20);
    let start = Instant::now();
    for width in [80usize, 120, 200] {
        let lines = build_chat_lines(&state, width, false);
        assert!(!lines.is_empty());
        render_once(&state, width as u16, 40);
    }
    report(
        "resize_3_widths",
        start.elapsed(),
        Duration::from_millis(500),
    );
}

#[test]
fn perf_large_diff_render() {
    // TUI-022: 2k-line diff through the real DiffPage. First render
    // processes + highlights; repeats must hit the processed-diff cache.
    use niki::display::pages::diff::DiffPage;
    let mut body = String::from("diff --git a/big.rs b/big.rs\n");
    for i in 0..1000 {
        body.push_str(&format!("-old line number {i} with some content here\n"));
        body.push_str(&format!("+new line number {i} with some content here\n"));
    }
    let mut state = make_state();
    state.diff_content = Some(body);
    state.current_page = niki::display::pages::PageId::Diff;
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let page = DiffPage::new();
    let start = Instant::now();
    terminal
        .draw(|f| {
            use niki::display::pages::Page;
            page.render(f, f.area(), &state);
        })
        .unwrap();
    let first = start.elapsed();
    let start = Instant::now();
    for _ in 0..9 {
        terminal
            .draw(|f| {
                use niki::display::pages::Page;
                page.render(f, f.area(), &state);
            })
            .unwrap();
    }
    let rest_mean = start.elapsed() / 9;
    println!(
        "[tui_perf] diff first render {}ms, memoized mean {}ms",
        first.as_millis(),
        rest_mean.as_millis()
    );
    report("diff_first_render", first, Duration::from_millis(2000));
    report(
        "diff_memoized_mean_x9",
        rest_mean,
        Duration::from_millis(500),
    );
}
