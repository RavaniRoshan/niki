//! Rich terminal TUI (opt-in via `niki run --tui`).
//!
//! The pipeline runs on the async runtime and pushes [`DisplayEvent`]s over a
//! channel; a dedicated OS thread owns the `ratatui` terminal and renders panels
//! for each stage. The TUI is strictly a viewer: it never blocks the pipeline,
//! and on exit (channel closed, `q`/`Esc`, or panic) it restores the terminal.
//!
//! It must not swallow Ctrl+C — the SIGINT handler lives in `cli/run.rs` and uses
//! async signal handling, which is independent of the terminal's raw mode.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

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
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;

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
        AgentRole::Planner => Color::Green,
        AgentRole::Coder => Color::Blue,
        AgentRole::Tester => Color::Magenta,
        AgentRole::Reviewer => Color::Yellow,
    }
}

fn role_name(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "Planner",
        AgentRole::Coder => "Coder",
        AgentRole::Tester => "Tester",
        AgentRole::Reviewer => "Reviewer",
    }
}

#[derive(Debug, Clone)]
struct StageView {
    role: AgentRole,
    status: String,
    stream: String,
    input_tokens: u32,
    output_tokens: u32,
    cost_usd: f64,
    latency_ms: u64,
    summary: Vec<String>,
}

struct TuiView {
    description: String,
    stages: Vec<StageView>,
    log: Vec<String>,
    finished: bool,
}

impl TuiView {
    fn new(description: String) -> Self {
        let stages = vec![
            StageView {
                role: AgentRole::Planner,
                status: "pending".into(),
                stream: String::new(),
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.0,
                latency_ms: 0,
                summary: Vec::new(),
            },
            StageView {
                role: AgentRole::Coder,
                status: "pending".into(),
                stream: String::new(),
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.0,
                latency_ms: 0,
                summary: Vec::new(),
            },
            StageView {
                role: AgentRole::Tester,
                status: "pending".into(),
                stream: String::new(),
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.0,
                latency_ms: 0,
                summary: Vec::new(),
            },
            StageView {
                role: AgentRole::Reviewer,
                status: "pending".into(),
                stream: String::new(),
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 0.0,
                latency_ms: 0,
                summary: Vec::new(),
            },
        ];
        Self {
            description,
            stages,
            log: Vec::new(),
            finished: false,
        }
    }

    fn stage_mut(&mut self, role: AgentRole) -> &mut StageView {
        self.stages.iter_mut().find(|s| s.role == role).unwrap()
    }

    fn apply(&mut self, ev: DisplayEvent) {
        match ev {
            DisplayEvent::Banner { description } => self.description = description,
            DisplayEvent::StageStart { role } => {
                let s = self.stage_mut(role);
                s.status = "running".into();
                s.stream.clear();
                s.summary.clear();
            }
            DisplayEvent::StageToken { role, token } => {
                let s = self.stage_mut(role);
                s.stream.push_str(&token);
                // Bound the live buffer so memory/rendering stay cheap.
                if s.stream.len() > 4000 {
                    let drop = s.stream.len() - 4000;
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
                s.status = "done".into();
                s.summary = summary;
                s.input_tokens = input_tokens;
                s.output_tokens = output_tokens;
                s.cost_usd = cost_usd;
                s.latency_ms = latency_ms;
            }
            DisplayEvent::StageFailed { role, error } => {
                let s = self.stage_mut(role);
                s.status = "failed".into();
                s.summary = vec![error];
            }
            DisplayEvent::Revision { round, max, issues } => {
                let mut line = format!("⟳ Revision {} of {} requested:", round, max);
                for i in &issues {
                    line.push_str(&format!("  • {}", i));
                }
                self.log.push(line);
                if self.log.len() > 30 {
                    self.log.remove(0);
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
        let v = &view;
        if terminal.draw(|f| render(f, v)).is_err() {
            break;
        }

        // Handle a keypress (non-blocking) — q/Esc leaves the view early; the
        // pipeline keeps running underneath.
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        match rx.recv_timeout(Duration::from_millis(80)) {
            Ok(ev) => view.apply(ev),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = terminal.draw(|f| render(f, &view));
}

fn render(frame: &mut ratatui::Frame, view: &TuiView) {
    let size = frame.area();
    if size.height < 8 {
        return;
    }

    let n_stages = view.stages.len();
    let mut constraints = vec![Constraint::Length(3)];
    for _ in 0..n_stages {
        constraints.push(Constraint::Min(3));
    }
    constraints.push(Constraint::Length(5));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    // Header
    let (tot_in, tot_out, tot_cost, tot_ms) = view.totals();
    let cost_str = if tot_cost > 0.0 {
        format!(" · ${:.4}", tot_cost)
    } else {
        String::new()
    };
    let header_text = format!(
        "NIKI · {} · tokens {}/{} · {:.1}s{}",
        view.description.chars().take(40).collect::<String>(),
        tot_in,
        tot_out,
        tot_ms as f64 / 1000.0,
        cost_str
    );
    frame.render_widget(
        Paragraph::new(header_text).style(Style::default().add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    // Stages
    for (i, stage) in view.stages.iter().enumerate() {
        let color = role_color(stage.role);
        let title = format!(" {} · {} ", role_name(stage.role), stage.status);
        let block = Block::default()
            .title(Span::styled(title, Style::default().fg(color).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));

        let body = if stage.status == "running" {
            stage.stream.clone()
        } else if stage.summary.is_empty() {
            String::new()
        } else {
            stage.summary.join("\n")
        };

        let mut lines = vec![Line::from(body)];
        if stage.status == "done" {
            let cost = if stage.cost_usd > 0.0 {
                format!(" · ${:.4}", stage.cost_usd)
            } else {
                String::new()
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "in {} · out {} · {:.1}s{}",
                    stage.input_tokens,
                    stage.output_tokens,
                    stage.latency_ms as f64 / 1000.0,
                    cost
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }

        frame.render_widget(Paragraph::new(lines).block(block), chunks[i + 1]);
    }

    // Footer log
    let items: Vec<ListItem> = view
        .log
        .iter()
        .map(|l| ListItem::new(l.clone()))
        .collect();
    let log_block = Block::default()
        .title(" Activity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        List::new(items).block(log_block),
        chunks[n_stages + 1],
    );
}
