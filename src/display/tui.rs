//! Rich terminal TUI (opt-in via `niki run --tui`), with multi-page navigation.
//!
//! The pipeline runs on the async runtime and pushes [`DisplayEvent`]s over a
//! channel; a dedicated OS thread owns the `ratatui` terminal and renders the
//! active page. Pages are: Run (live stream), Pipeline, Agents, Diff, Verdict,
//! Cost, Artifacts, History, Config, Help. Modals overlay on top.
//!
//! The TUI is strictly a viewer: it never blocks the pipeline, and on exit
//! (channel closed, `q`/`Esc`, or panic) it restores the terminal.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::artifacts::types::AgentRole;
use crate::display::theme;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use super::modal::{self, ModalAction};
use super::pages::{AppState, PageId, PageRouter};

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
    /// Pipeline produced a diff — feed it to the TUI Diff page.
    DiffContent(String),
    /// Pipeline produced a review report — feed it to the TUI Verdict page.
    ReportContent(String),
    /// Cost breakdown JSON — feed it to the TUI Cost page.
    CostJson(String),
    /// Artifacts directory path — feed it to the TUI Artifacts page.
    ArtifactsDir(String),
    Final,
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

    // Set the terminal background color
    let _ = terminal.draw(|f| {
        let area = f.area();
        f.render_widget(
            Paragraph::new(""),
            Rect { x: 0, y: 0, width: area.width, height: area.height },
        );
    });

    let config = crate::config::types::NikiConfig::load(std::path::Path::new(".")).unwrap_or_default();
    let mut state = AppState::new(description, config, std::path::PathBuf::from("."));
    let mut router = PageRouter::new();

    loop {
        state.tick = state.tick.wrapping_add(1);
        let s = &state;
        if terminal.draw(|f| render(f, s, &router)).is_err() {
            break;
        }

        // Handle keypresses (non-blocking)
        if event::poll(Duration::from_millis(40)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                // Global quit: q or Esc on any page (except modal)
                if state.modal.is_none() {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        break;
                    }
                }

                // Modal key handling
                if let Some(ref modal) = state.modal {
                    match modal::handle_modal_key(key, modal) {
                        ModalAction::Dismiss => {
                            state.modal = None;
                        }
                        ModalAction::Confirm => {
                            state.modal = None;
                            // If confirming quit, break
                            if key.code == KeyCode::Enter {
                                break;
                            }
                        }
                        ModalAction::Retry => {
                            state.modal = None;
                        }
                        ModalAction::Config => {
                            state.modal = None;
                            state.current_page = PageId::Config;
                        }
                        ModalAction::None => {}
                    }
                } else {
                    // Page-specific key handling
                    router.handle_key(key, &mut state);
                }
            }
        }

        // ~100ms cadence keeps the spinner lively without busy-looping.
        match rx.recv_timeout(Duration::from_millis(60)) {
            Ok(ev) => {
                state.apply_event(ev);
                // Drain any other queued events this tick.
                while let Ok(ev) = rx.try_recv() {
                    state.apply_event(ev);
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    state.finished = true;
    let _ = terminal.draw(|f| render(f, &state, &router));
}

fn render(frame: &mut ratatui::Frame, state: &AppState, router: &PageRouter) {
    let size = frame.area();
    if size.height < 10 {
        return;
    }

    // Fill background
    let bg_block = ratatui::widgets::Block::default()
        .style(Style::default().bg(theme::BG));
    frame.render_widget(bg_block, size);

    // Main layout: logo area + page content + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),   // logo (6 lines + 2 padding)
            Constraint::Min(5),      // page content
            Constraint::Length(1),   // status bar
        ])
        .split(size);

    // Render logo in the top area
    super::logo::render_logo(frame, chunks[0]);

    // Render the current page in the content area
    router.render_current(frame, chunks[1], state);

    // Minimal status bar at the bottom
    let (tot_in, tot_out, tot_cost, _ms) = state.totals();
    let cost_str = if tot_cost > 0.0 {
        format!(" ${:.4}", tot_cost)
    } else {
        String::new()
    };
    let status = Line::from(vec![
        Span::styled(" niki ", Style::default().fg(theme::BLUE).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(
                "· {}/{} tok{}",
                fmt_tokens(tot_in),
                fmt_tokens(tot_out),
                cost_str,
            ),
            Style::default().fg(theme::FG_DIM),
        ),
        Span::styled(
            format!(" · {} ", state.current_page.title()),
            Style::default().fg(theme::BORDER_ACTIVE),
        ),
    ]);
    frame.render_widget(Paragraph::new(status), chunks[2]);

    // Render modal overlay if present
    if let Some(ref modal) = state.modal {
        modal::render_modal(frame, modal, size);
    }
}

fn fmt_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_event_apply() {
        let config = crate::config::types::NikiConfig::default();
        let mut state = AppState::new("test".into(), config, ".".into());
        state.apply_event(DisplayEvent::StageStart { role: AgentRole::Planner });
        assert_eq!(state.stages.len(), 1);
        assert_eq!(state.stages[0].role, AgentRole::Planner);

        state.apply_event(DisplayEvent::StageDone {
            role: AgentRole::Planner,
            summary: vec!["Spec: 1 file".into()],
            input_tokens: 1200,
            output_tokens: 800,
            cost_usd: 0.01,
            latency_ms: 3400,
        });
        assert_eq!(state.stages[0].input_tokens, 1200);
    }

    #[test]
    fn page_id_from_key() {
        assert_eq!(PageId::from_key('p'), Some(PageId::Pipeline));
        assert_eq!(PageId::from_key('a'), Some(PageId::Agents));
        assert_eq!(PageId::from_key('d'), Some(PageId::Diff));
        assert_eq!(PageId::from_key('v'), Some(PageId::Verdict));
        assert_eq!(PageId::from_key('c'), Some(PageId::Cost));
        assert_eq!(PageId::from_key('f'), Some(PageId::Artifacts));
        assert_eq!(PageId::from_key('h'), Some(PageId::History));
        assert_eq!(PageId::from_key(','), Some(PageId::Config));
        assert_eq!(PageId::from_key('?'), Some(PageId::Help));
        assert_eq!(PageId::from_key('x'), None);
    }

    #[test]
    fn token_formatting() {
        assert_eq!(fmt_tokens(999), "999");
        assert_eq!(fmt_tokens(1500), "1.5k");
    }
}
