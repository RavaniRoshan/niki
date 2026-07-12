//! Rich terminal TUI (opt-in via `niki run --tui`), styled after the Claude Code
//! CLI: a flowing vertical transcript (not bordered panels) with `⏺` action
//! bullets, `⎿` nested result connectors, an animated sparkle spinner carrying
//! elapsed time + token counts, and a bottom `⏵⏵` mode/status line.
//!
//! The pipeline runs on the async runtime and pushes [`DisplayEvent`]s over a
//! channel; a dedicated OS thread owns the `ratatui` terminal and renders the
//! transcript. The TUI is strictly a viewer: it never blocks the pipeline, and
//! on exit (channel closed, `q`/`Esc`, or panic) it restores the terminal.
//!
//! It must not swallow Ctrl+C — the SIGINT handler lives in `cli/run.rs` and uses
//! async signal handling, which is independent of the terminal's raw mode.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::artifacts::types::AgentRole;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Terminal;

// ── Claude-Code dark palette ────────────────────────────────────────────────
const FG: Color = Color::Rgb(230, 237, 243); // primary text
const OK: Color = Color::Rgb(78, 186, 101); // success green
const ERR: Color = Color::Rgb(255, 107, 128); // error red
const WARN: Color = Color::Rgb(255, 193, 7); // amber
const BLUE: Color = Color::Rgb(177, 185, 249); // permission/accent
const SUBTLE: Color = Color::Rgb(120, 128, 140); // secondary text
const INACTIVE: Color = Color::Rgb(110, 118, 129); // pending/dim
const ADD_BG: Color = Color::Rgb(34, 92, 43);
const DEL_BG: Color = Color::Rgb(122, 41, 54);

// Signature glyphs.
const BULLET: &str = "⏺"; // action / stage marker (U+23FA)
const CONNECT: &str = "⎿"; // nested result connector (U+23BF)

/// Sparkle spinner frames, animated ~one step per render tick (~100ms). Mirrors
/// the Claude Code "working" pulse.
const SPINNER: &[&str] = &["✶", "✷", "✸", "✹", "✺", "✹", "✸", "✷"];

/// Gerund words the spinner cycles through while a stage runs, matching
/// Claude Code's playful status line.
const GERUNDS: &[&str] = &[
    "Thinking", "Pondering", "Herding", "Sketching", "Sauntering", "Undulating",
    "Scampering", "Brewing", "Combobulating", "Shenaniganing", "Crafting", "Reasoning",
];

/// Events emitted by the pipeline/display layer for the TUI to render.
#[derive(Debug, Clone)]
pub enum DisplayEvent {
    Banner { description: String },
    StageStart { role: AgentRole },
    StageToken { role: AgentRole, token: String },
    StageDone {
        role: AgentRole,
        summary: Vec<String>,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
        latency_ms: u64,
    },
    StageFailed { role: AgentRole, error: String },
    Revision { round: u32, max: u32, issues: Vec<String> },
    Final,
}

fn role_color(role: AgentRole) -> Color {
    match role {
        AgentRole::Planner => Color::Rgb(126, 178, 255),      // soft blue
        AgentRole::Coder => Color::Rgb(198, 160, 246),        // lavender
        AgentRole::Tester => OK,                              // green
        AgentRole::Reviewer => WARN,                          // amber
        AgentRole::Synthesizer => Color::Rgb(129, 200, 190),  // teal
        AgentRole::SecurityAuditor => ERR,                    // red
    }
}

fn role_name(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "Planner",
        AgentRole::Coder => "Coder",
        AgentRole::Tester => "Tester",
        AgentRole::Reviewer => "Reviewer",
        AgentRole::Synthesizer => "Synthesizer",
        AgentRole::SecurityAuditor => "Security Auditor",
    }
}

#[derive(Clone, PartialEq)]
enum Status {
    Running,
    Done,
    Failed,
}

struct StageView {
    role: AgentRole,
    status: Status,
    /// Last few lines of live streaming output (bounded).
    stream: String,
    input_tokens: u32,
    output_tokens: u32,
    cost_usd: f64,
    latency_ms: u64,
    summary: Vec<String>,
    /// When this stage started running (for a live elapsed timer).
    start: Option<Instant>,
}

struct TuiView {
    description: String,
    stages: Vec<StageView>,
    /// Free-form transcript notes (revisions, failures) appended in order.
    notes: Vec<(String, Color)>,
    finished: bool,
    /// Advances once per render tick to animate the spinner.
    tick: usize,
}

impl TuiView {
    fn new(description: String) -> Self {
        Self {
            description,
            // Stages are created lazily as events arrive, so arbitrary role
            // topologies (parallel coders + Synthesizer, SecurityAuditor, …)
            // render without a hardcoded stage list.
            stages: Vec::new(),
            notes: Vec::new(),
            finished: false,
            tick: 0,
        }
    }

    fn stage_mut(&mut self, role: AgentRole) -> &mut StageView {
        if !self.stages.iter().any(|s| s.role == role && s.status == Status::Running) {
            // Reuse a prior view only if it's the active one; otherwise start a
            // fresh entry so a re-run (revision round) reads as a new action.
            if let Some(idx) = self
                .stages
                .iter()
                .position(|s| s.role == role && s.status == Status::Running)
            {
                return &mut self.stages[idx];
            }
        }
        if let Some(idx) = self
            .stages
            .iter()
            .rposition(|s| s.role == role && s.status == Status::Running)
        {
            return &mut self.stages[idx];
        }
        self.stages.push(StageView {
            role,
            status: Status::Running,
            stream: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            latency_ms: 0,
            summary: Vec::new(),
            start: None,
        });
        self.stages.last_mut().unwrap()
    }

    fn apply(&mut self, ev: DisplayEvent) {
        match ev {
            DisplayEvent::Banner { description } => self.description = description,
            DisplayEvent::StageStart { role } => {
                // Always begin a fresh action entry so revision rounds stack.
                self.stages.push(StageView {
                    role,
                    status: Status::Running,
                    stream: String::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: 0.0,
                    latency_ms: 0,
                    summary: Vec::new(),
                    start: Some(Instant::now()),
                });
            }
            DisplayEvent::StageToken { role, token } => {
                let s = self.stage_mut(role);
                s.stream.push_str(&token);
                // Keep only the tail so memory/rendering stay cheap.
                if s.stream.len() > 2000 {
                    let drop = s.stream.len() - 2000;
                    s.stream.drain(..drop);
                }
            }
            DisplayEvent::StageDone {
                role,
                summary,
                input_tokens,
                output_tokens,
                cost_usd,
                latency_ms,
            } => {
                let s = self.stage_mut(role);
                s.status = Status::Done;
                s.summary = summary;
                s.input_tokens = input_tokens;
                s.output_tokens = output_tokens;
                s.cost_usd = cost_usd;
                s.latency_ms = latency_ms;
                s.stream.clear();
            }
            DisplayEvent::StageFailed { role, error } => {
                let s = self.stage_mut(role);
                s.status = Status::Failed;
                s.summary = vec![error];
                s.stream.clear();
            }
            DisplayEvent::Revision { round, max, issues } => {
                self.notes
                    .push((format!("Revision {} of {} requested", round, max), WARN));
                for i in &issues {
                    self.notes.push((format!("  {}", i), SUBTLE));
                }
            }
            DisplayEvent::Final => self.finished = true,
        }
    }

    fn totals(&self) -> (u32, u32, f64, u64) {
        let mut in_t = 0;
        let mut out_t = 0;
        let mut cost = 0.0;
        let mut ms = 0;
        for s in &self.stages {
            in_t += s.input_tokens;
            out_t += s.output_tokens;
            cost += s.cost_usd;
            ms += s.latency_ms;
        }
        (in_t, out_t, cost, ms)
    }

    fn active(&self) -> Option<&StageView> {
        self.stages.iter().find(|s| s.status == Status::Running)
    }
}

/// Restore terminal state no matter how we leave `run_tui`.
struct RestoreGuard;

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

/// Spawn the TUI thread. Returns the event sender (held by `AgenticDisplay`) and
/// the join handle. The thread exits when the sender is dropped or the user
/// presses `q`/`Esc`.
pub fn spawn_tui(description: String) -> (Sender<DisplayEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || run_tui(rx, description));
    (tx, handle)
}

fn run_tui(rx: Receiver<DisplayEvent>, description: String) {
    let _guard = RestoreGuard;

    if enable_raw_mode().is_err() {
        return;
    }
    if execute!(io::stdout(), EnterAlternateScreen).is_err() {
        return;
    }

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(_) => return,
    };

    let mut view = TuiView::new(description);

    loop {
        view.tick = view.tick.wrapping_add(1);
        let v = &view;
        if terminal.draw(|f| render(f, v)).is_err() {
            break;
        }

        // Handle a keypress (non-blocking) — q/Esc leaves the view early; the
        // pipeline keeps running underneath.
        if event::poll(Duration::from_millis(40)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        // ~100ms cadence keeps the spinner lively without busy-looping.
        match rx.recv_timeout(Duration::from_millis(60)) {
            Ok(ev) => {
                view.apply(ev);
                // Drain any other queued events this tick.
                while let Ok(ev) = rx.try_recv() {
                    view.apply(ev);
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    view.finished = true;
    let _ = terminal.draw(|f| render(f, &view));
}

/// Last non-empty line of a streaming buffer, trimmed for the transcript.
fn last_line(s: &str) -> &str {
    s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim_end()
}

/// During live streaming, color any trailing diff context so an agent mid-edit
/// shows the same green/red inline diffs as the dashboard.
fn stream_line_style(line: &str) -> Style {
    if line.starts_with('+') {
        Style::default().fg(OK).bg(ADD_BG)
    } else if line.starts_with('-') {
        Style::default().fg(ERR).bg(DEL_BG)
    } else {
        Style::default().fg(SUBTLE)
    }
}

fn fmt_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

/// Build the flowing transcript: one `⏺` action per stage, `⎿` nested results,
/// a live spinner line for the running stage, then any notes.
fn build_transcript(view: &TuiView) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    for stage in &view.stages {
        let color = role_color(stage.role);
        let name = role_name(stage.role);

        // Action header: ⏺ RoleName
        let marker_color = match stage.status {
            Status::Running => color,
            Status::Done => OK,
            Status::Failed => ERR,
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", BULLET), Style::default().fg(marker_color)),
            Span::styled(
                name.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]));

        match stage.status {
            Status::Running => {
                let tail = last_line(&stage.stream);
                if !tail.is_empty() {
                    let shown: String = tail.chars().take(200).collect();
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", CONNECT), Style::default().fg(SUBTLE)),
                        Span::styled(shown, stream_line_style(tail)),
                    ]));
                }
            }
            Status::Done => {
                for (i, summ) in stage.summary.iter().enumerate() {
                    let connector = if i == 0 { CONNECT } else { " " };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", connector), Style::default().fg(SUBTLE)),
                        Span::styled(summ.clone(), Style::default().fg(FG)),
                    ]));
                }
                let cost = if stage.cost_usd > 0.0 {
                    format!(" · ${:.4}", stage.cost_usd)
                } else {
                    String::new()
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "     ↑ {} ↓ {} · {:.1}s{}",
                        fmt_tokens(stage.input_tokens),
                        fmt_tokens(stage.output_tokens),
                        stage.latency_ms as f64 / 1000.0,
                        cost
                    ),
                    Style::default().fg(INACTIVE),
                )));
            }
            Status::Failed => {
                for summ in &stage.summary {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", CONNECT), Style::default().fg(ERR)),
                        Span::styled(summ.clone(), Style::default().fg(ERR)),
                    ]));
                }
            }
        }
        lines.push(Line::from(""));
    }

    for (note, color) in &view.notes {
        lines.push(Line::from(Span::styled(note.clone(), Style::default().fg(*color))));
    }

    lines
}

fn render(frame: &mut ratatui::Frame, view: &TuiView) {
    let size = frame.area();
    if size.height < 6 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),    // transcript
            Constraint::Length(1), // spinner / status line
            Constraint::Length(1), // mode line
        ])
        .split(size);

    // ── Header (slim, no box) ──────────────────────────────────────────────
    let desc: String = view.description.chars().take(60).collect();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("✻ ", Style::default().fg(BLUE)),
            Span::styled("NIKI", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled("  ", Style::default()),
            Span::styled(desc, Style::default().fg(SUBTLE)),
        ])),
        chunks[0],
    );

    // ── Transcript (scrolls to bottom) ─────────────────────────────────────
    let lines = build_transcript(view);
    let total = lines.len() as u16;
    let view_h = chunks[1].height;
    let scroll = total.saturating_sub(view_h);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        chunks[1],
    );

    // ── Spinner / live status line ─────────────────────────────────────────
    let (tot_in, tot_out, _cost, _ms) = view.totals();
    let spinner_line = if let Some(active) = view.active() {
        let frame_glyph = SPINNER[view.tick % SPINNER.len()];
        let gerund = GERUNDS[(view.tick / 8) % GERUNDS.len()];
        let elapsed = active.start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
        Line::from(vec![
            Span::styled(format!("{} ", frame_glyph), Style::default().fg(BLUE)),
            Span::styled(
                format!("{}… ", gerund),
                Style::default().fg(FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "({}s · ↓ {} · ↑ {} · esc to interrupt)",
                    elapsed,
                    fmt_tokens(active.input_tokens),
                    fmt_tokens(active.output_tokens),
                ),
                Style::default().fg(SUBTLE),
            ),
        ])
    } else if view.finished {
        Line::from(vec![
            Span::styled("✓ ", Style::default().fg(OK)),
            Span::styled(
                "Done ",
                Style::default().fg(OK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· {} tokens total", fmt_tokens(tot_in + tot_out)),
                Style::default().fg(SUBTLE),
            ),
        ])
    } else {
        Line::from(Span::styled(
            "  idle",
            Style::default().fg(INACTIVE),
        ))
    };
    frame.render_widget(Paragraph::new(spinner_line), chunks[2]);

    // ── Mode line (Claude Code style) ──────────────────────────────────────
    let (tot_in2, tot_out2, tot_cost, _ms2) = view.totals();
    let cost_str = if tot_cost > 0.0 {
        format!(" · ${:.4}", tot_cost)
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⏵⏵ ", Style::default().fg(OK)),
            Span::styled(
                "niki pipeline",
                Style::default().fg(SUBTLE),
            ),
            Span::styled(
                format!(
                    "  ·  {} agents · {}/{} tok{}  ·  q to detach",
                    view.stages.len(),
                    fmt_tokens(tot_in2),
                    fmt_tokens(tot_out2),
                    cost_str
                ),
                Style::default().fg(INACTIVE),
            ),
        ])),
        chunks[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_includes_markers() {
        let mut view = TuiView::new("demo".into());
        view.apply(DisplayEvent::StageStart { role: AgentRole::Planner });
        view.apply(DisplayEvent::StageDone {
            role: AgentRole::Planner,
            summary: vec!["Spec: 2 files".into()],
            input_tokens: 1200,
            output_tokens: 800,
            cost_usd: 0.01,
            latency_ms: 3400,
        });
        let lines = build_transcript(&view);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(text.contains(BULLET), "expected action bullet");
        assert!(text.contains(CONNECT), "expected nested connector");
        assert!(text.contains("Planner"));
        assert!(text.contains("Spec: 2 files"));
    }

    #[test]
    fn token_formatting() {
        assert_eq!(fmt_tokens(999), "999");
        assert_eq!(fmt_tokens(1500), "1.5k");
    }

    #[test]
    fn tracks_active_stage() {
        let mut view = TuiView::new("d".into());
        view.apply(DisplayEvent::StageStart { role: AgentRole::Coder });
        assert!(view.active().is_some());
        view.apply(DisplayEvent::StageDone {
            role: AgentRole::Coder,
            summary: vec![],
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            latency_ms: 0,
        });
        assert!(view.active().is_none());
    }
}
