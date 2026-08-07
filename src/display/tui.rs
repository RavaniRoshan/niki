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
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::artifacts::types::AgentRole;
use crate::display::theme;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::command_palette::CommandPalette;
use super::modal::{self, ModalAction};
use super::onboarding::{self, OnboardingAction};
use super::pages::{AppState, PageId, PageRouter};

/// Events emitted by the pipeline/display layer for the TUI to render.
#[derive(Debug, Clone)]
pub enum DisplayEvent {
    Banner {
        description: String,
    },
    StageStart {
        role: AgentRole,
    },
    StageToken {
        role: AgentRole,
        token: String,
    },
    StageDone {
        role: AgentRole,
        summary: Vec<String>,
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
        latency_ms: u64,
    },
    StageFailed {
        role: AgentRole,
        error: String,
    },
    Revision {
        round: u32,
        max: u32,
        issues: Vec<String>,
    },
    /// Pipeline produced a diff — feed it to the TUI Diff page.
    DiffContent(String),
    /// Pipeline produced a review report — feed it to the TUI Verdict page.
    ReportContent(String),
    /// Cost breakdown JSON — feed it to the TUI Cost page.
    CostJson(String),
    /// Test log content — feed it to the TUI TestLog page.
    TestLogContent(String),
    ArtifactsDir(String),
    Final,
    /// Branch name from the pipeline (fixes the never-populated branch_name).
    BranchName(String),
    /// Total token/cost info for the status line.
    StageTotals { input_tokens: u32, output_tokens: u32, cost_usd: f64, latency_ms: u64 },
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
pub fn spawn_tui(
    description: String,
    project_path: PathBuf,
) -> (Sender<DisplayEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || run_tui(rx, description, project_path));
    (tx, handle)
}

fn run_tui(rx: Receiver<DisplayEvent>, description: String, project_path: PathBuf) {
    let _guard = RestoreGuard;

    if enable_raw_mode().is_err() {
        return;
    }
    if execute!(io::stdout(), EnterAlternateScreen).is_err() {
        return;
    }

    // Best-effort DEC 2026 synchronized output — eliminates flicker on
    // supporting terminals (kitty, Ghostty, xterm.js ≥6.0, newer tmux).
    let sync_capable = detect_synchronized_output();
    if sync_capable {
        let _ = execute!(
            io::stdout(),
            ratatui::crossterm::terminal::BeginSynchronizedUpdate
        );
    }

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Initial full draw
    let _ = terminal.draw(|f| {
        let area = f.area();
        f.render_widget(
            Paragraph::new(""),
            Rect {
                x: 0,
                y: 0,
                width: area.width,
                height: area.height,
            },
        );
    });

    let config =
        crate::config::types::NikiConfig::load(std::path::Path::new(".")).unwrap_or_default();

    // Initialize theme mode from config
    {
        use crate::config::types::ThemePreference;
        // NO_COLOR overrides everything — force dark mode (all colors become Reset via no_color())
        if theme::no_color() {
            theme::set_mode(theme::ThemeMode::Dark);
        } else {
            let mode = match config.ui.theme {
                ThemePreference::Dark => theme::ThemeMode::Dark,
                ThemePreference::Light => theme::ThemeMode::Light,
                ThemePreference::Auto => theme::ThemeMode::Auto,
            };
            theme::set_mode(mode);
        }
    }

    let mut state = AppState::new(description, config, project_path.clone());
    state.onboarded = onboarding::load_state(&project_path);

    let mut router = PageRouter::new();
    let mut command_palette = CommandPalette::new();

    // Show onboarding modal if needed
    if onboarding::should_show_onboarding(&project_path) {
        state.onboarding = Some(onboarding::OnboardingModal::new());
    }

    // Dirty-flag rendering: only redraw when state changes.
    // Cap at ~30 FPS (33ms) to avoid busy-looping while keeping spinner lively.
    let mut dirty = true;
    let mut last_render = std::time::Instant::now();
    let min_frame_interval = Duration::from_millis(33);

    loop {
        // Check if we need to redraw
        let now = std::time::Instant::now();
        if dirty && now.duration_since(last_render) >= min_frame_interval {
            state.tick = state.tick.wrapping_add(1);

            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::BeginSynchronizedUpdate
                );
            }

            let s = &state;
            if terminal.draw(|f| render(f, s, &router, &command_palette)).is_err() {
                break;
            }

            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::EndSynchronizedUpdate
                );
            }

            dirty = false;
            last_render = now;
        }

        // Handle keypresses (non-blocking, ~16ms poll)
        if event::poll(Duration::from_millis(16)).unwrap_or(false)
            && let Ok(Event::Key(key)) = event::read() {
                // Onboarding modal takes priority
                if let Some(ref mut onboard) = state.onboarding {
                    match onboard.handle_key(key) {
                        OnboardingAction::None => {}
                        OnboardingAction::Skip | OnboardingAction::Finish => {
                            if onboard.dont_show_again {
                                onboarding::persist_state(&project_path);
                                state.onboarded = true;
                            }
                            state.onboarding = None;
                            dirty = true;
                        }
                    }
                } else if let Some(ref modal) = state.modal {
                    // Regular modal key handling
                    match modal::handle_modal_key(key, modal) {
                        ModalAction::Dismiss => {
                            state.modal = None;
                            dirty = true;
                        }
                        ModalAction::Confirm => {
                            state.modal = None;
                            if key.code == KeyCode::Enter {
                                break;
                            }
                            dirty = true;
                        }
                        ModalAction::Retry => {
                            state.modal = None;
                            dirty = true;
                        }
                        ModalAction::Config => {
                            state.modal = None;
                            state.current_page = PageId::Config;
                            dirty = true;
                        }
                        ModalAction::Skip => {
                            state.modal = None;
                            dirty = true;
                        }
                        ModalAction::None => {}
                    }
                } else if state.show_command_palette {
                    // Command palette takes priority
                    if command_palette.handle_key(key, &mut state) {
                        state.show_command_palette = false;
                        dirty = true;
                    } else {
                        dirty = true;
                    }
                } else {
                    // Ctrl-P opens command palette (global, from any page)
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('p')
                    {
                        state.show_command_palette = true;
                        command_palette = CommandPalette::new();
                        dirty = true;
                    } else if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('t')
                    {
                        // Ctrl+T cycles theme: dark → light → auto → dark
                        use crate::config::types::ThemePreference;
                        let new_pref = match state.config.ui.theme {
                            ThemePreference::Dark => ThemePreference::Light,
                            ThemePreference::Light => ThemePreference::Auto,
                            ThemePreference::Auto => ThemePreference::Dark,
                        };
                        // Apply to global theme mode
                        let mode = match new_pref {
                            ThemePreference::Dark => theme::ThemeMode::Dark,
                            ThemePreference::Light => theme::ThemeMode::Light,
                            ThemePreference::Auto => theme::ThemeMode::Auto,
                        };
                        theme::set_mode(mode);
                        state.config.ui.theme = new_pref;
                        // Persist to config file
                        let _ = crate::config::types::NikiConfig::save_theme(new_pref);
                        dirty = true;
                    } else if state.current_page == PageId::Run {
                        // On Run page: q/Esc shows quit confirm modal
                        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                            state.modal = Some(super::pages::Modal::Confirm {
                                title: "Quit NIKI?".to_string(),
                                message: "The pipeline will continue in the background.".to_string(),
                            });
                            dirty = true;
                        } else if router.handle_key(key, &mut state) {
                            dirty = true;
                        }
                    } else {
                        // On sub-pages: page-specific key handling (Esc/q handled by page → Run)
                        if router.handle_key(key, &mut state) {
                            dirty = true;
                        }
                    }
                }
            }

        // Drain events from the pipeline — mark dirty on any state change
        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(ev) => {
                state.apply_event(ev);
                dirty = true;
                // Drain any other queued events this tick
                while let Ok(ev) = rx.try_recv() {
                    state.apply_event(ev);
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    state.finished = true;
    let _ = terminal.draw(|f| render(f, &state, &router, &command_palette));

    // End synchronized update on exit
    if sync_capable {
        let _ = execute!(
            io::stdout(),
            ratatui::crossterm::terminal::EndSynchronizedUpdate
        );
    }
}

/// Best-effort detection of DEC 2026 synchronized output support.
/// Returns true if the terminal likely supports it.
fn detect_synchronized_output() -> bool {
    // Check common env vars that indicate terminal capabilities
    if let Ok(term) = std::env::var("TERM")
        && (term.contains("kitty") || term.contains("ghostty") || term.contains("xterm")) {
            return true;
        }
    if let Ok(term_program) = std::env::var("TERM_PROGRAM")
        && (term_program.contains("kitty")
            || term_program.contains("ghostty")
            || term_program.contains("WezTerm")
            || term_program.contains("iTerm"))
        {
            return true;
        }
    // tmux with sync support (3.4+)
    if std::env::var("TMUX").is_ok() {
        // tmux < 3.4 does not support DEC 2026; we conservatively disable it
        // under tmux since the official docs say releases through 3.6 lack it.
        return false;
    }
    false
}

fn render_status_line(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, state: &AppState) {
    let (in_tokens, out_tokens, cost, _) = state.totals();

    let duration = state
        .start_time
        .map(|s| {
            let elapsed = s.elapsed().as_secs();
            if elapsed > 0 {
                format!("  {}s ", format_duration(elapsed))
            } else {
                String::new()
            }
        })
        .unwrap_or_default();

    let model_info = if !state.stages.is_empty() {
        let active_stage = state.active_stage();
        if let Some(stage) = active_stage {
            let cfg = match stage.role {
                crate::artifacts::types::AgentRole::Planner => &state.config.agents.planner,
                crate::artifacts::types::AgentRole::Coder => &state.config.agents.coder,
                crate::artifacts::types::AgentRole::Tester => &state.config.agents.tester,
                crate::artifacts::types::AgentRole::Reviewer => &state.config.agents.reviewer,
                crate::artifacts::types::AgentRole::Synthesizer => &state.config.agents.synthesizer,
                crate::artifacts::types::AgentRole::SecurityAuditor => &state.config.agents.security_auditor,
                crate::artifacts::types::AgentRole::Red => &state.config.agents.red,
            };
            format!("  {} ", cfg.model)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let branch_display = if state.branch_name.is_empty() {
        "niki/xxxxx".to_string()
    } else {
        state.branch_name.clone()
    };

    let cost_display = format!("  ${:.4} ", cost);
    let token_display = format!("  I/O {}/{} ", in_tokens, out_tokens);

    let status_line = Line::from(vec![
        Span::styled(
            format!("  {}", branch_display),
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(&model_info, Style::default().fg(theme::fg_dim()),),
        Span::styled(&token_display, Style::default().fg(theme::fg_dim()),),
        Span::styled(&cost_display, Style::default().fg(theme::GREEN()),),
        Span::styled(&duration, Style::default().fg(theme::fg_dim()),),
        Span::styled(
            " ctrl-t theme ",
            Style::default().fg(theme::fg_dim()),
        ),
    ]);

    let bg = theme::bg_elevated();
    let status_block = ratatui::widgets::Block::default().style(Style::default().bg(bg));
    frame.render_widget(status_block, area);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(status_line),
        ratatui::layout::Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
    );
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn render(frame: &mut ratatui::Frame, state: &AppState, router: &PageRouter, command_palette: &CommandPalette) {
    let size = frame.area();
    if size.height < 10 {
        return;
    }

    // Fill background
    let bg_block = ratatui::widgets::Block::default().style(Style::default().bg(theme::bg_color()));
    frame.render_widget(bg_block, size);

    // Main layout: logo + page content + status line
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),       // logo (6 lines + 2 padding)
            Constraint::Min(5),          // page content
            Constraint::Length(1),       // status line (footer meta)
        ])
        .split(size);

    // Render logo in the top area
    super::logo::render_logo(frame, chunks[0]);

    // Render the current page in the content area
    router.render_current(frame, chunks[1], state);

    // Render status line (product "footer meta")
    render_status_line(frame, chunks[2], state);

    // Render modal overlay if present
    if let Some(ref modal) = state.modal {
        modal::render_modal(frame, modal, size);
    }

    // Render onboarding modal if present
    if let Some(ref onboard) = state.onboarding {
        onboard.render(frame, size);
    }

    // Render command palette overlay if present
    if state.show_command_palette {
        super::command_palette::render_command_palette(frame, command_palette, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_event_apply() {
        let config = crate::config::types::NikiConfig::default();
        let mut state = AppState::new("test".into(), config, ".".into());
        state.apply_event(DisplayEvent::StageStart {
            role: AgentRole::Planner,
        });
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
}
