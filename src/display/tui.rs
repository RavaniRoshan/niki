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

use tokio::sync::oneshot;

use crate::artifacts::types::AgentRole;
use crate::display::theme;
use crate::permissions::PermissionAction;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::command_palette::CommandPalette;
use super::components::command_menu;
use super::components::list_cursor::FocusState;
use super::components::permission;
use super::modal::{self, ModalAction};
use super::onboarding::{self, OnboardingAction};
use super::pages::chat;
use super::pages::{AppState, Page, PageId, PageRouter};
use super::persistence;
use super::state::InputMode;

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
    /// A chat message submitted/typed into the session (user or assistant turn).
    /// Used by `niki chat` to render the running conversation.
    ChatMessage {
        role: String,
        text: String,
    },
    /// Total token/cost info for the status line.
    StageTotals {
        input_tokens: u32,
        output_tokens: u32,
        cost_usd: f64,
        latency_ms: u64,
    },
    /// A permission prompt from the sandbox — the TUI should render a modal
    /// and send the user's choice back through `response_tx`.
    PermissionRequest {
        command: String,
        response_tx: std::sync::mpsc::Sender<PermissionAction>,
    },
    /// TUI sender for /steer corrections — the pipeline polls this shared state
    /// between agent streaming chunks for user corrections.
    SteerChannel(std::sync::Arc<std::sync::Mutex<Option<String>>>),
}

/// Which panel currently owns list navigation / mouse routing. Overlays win
/// over the chat view, in priority order (permission → palette → slash menu).
fn active_focus(state: &AppState) -> FocusState {
    if state.show_permission_modal {
        FocusState::Permission
    } else if state.show_command_palette {
        FocusState::CommandPalette
    } else if state.show_command_menu {
        FocusState::CommandMenu
    } else {
        FocusState::Chat
    }
}

/// Restore terminal state no matter how we leave `run_tui`.
struct RestoreGuard;

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
}

/// Spawn the TUI thread. Returns the event sender (held by `AgenticDisplay`) and
/// the join handle. The thread exits when the sender is dropped or the user
/// presses `q`/`Esc`.
pub fn spawn_tui(
    description: String,
    project_path: PathBuf,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> (Sender<DisplayEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || run_tui(rx, description, project_path, cancel));
    (tx, handle)
}

fn run_tui(
    rx: Receiver<DisplayEvent>,
    description: String,
    project_path: PathBuf,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let _guard = RestoreGuard;

    if enable_raw_mode().is_err() {
        return;
    }
    if execute!(io::stdout(), EnterAlternateScreen).is_err() {
        return;
    }
    // Enable mouse capture so the chat view can offer drag-to-select + auto-copy.
    let _ = execute!(io::stdout(), EnableMouseCapture);

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
    state.cancel = Some(cancel.clone());
    state.onboarded = onboarding::load_state(&project_path);

    let mut router = PageRouter::new();
    let mut command_palette = CommandPalette::new();

    // Show onboarding modal if needed
    if onboarding::should_show_onboarding(&project_path) {
        state.onboarding = Some(onboarding::OnboardingModal::new());
    }

    // Drive rendering through the high-performance RenderEngine: dirty-flag
    // redraws, 60fps during streaming / 30fps idle, and CSI 2026
    // synchronized output for flicker-free updates.
    let mut engine = crate::display::engine::RenderEngine::new(terminal, sync_capable);
    engine.mark_dirty();
    let mut last_render = std::time::Instant::now();
    // Tracks the previous Ctrl+C press for the two-press-to-exit behaviour.
    let mut last_ctrl_c: Option<std::time::Instant> = None;

    loop {
        // Adapt frame target: 60fps while a stage is streaming, else 30fps idle.
        let target = if state.has_running_stage() {
            crate::display::engine::FrameTarget::High
        } else {
            crate::display::engine::FrameTarget::Low
        };
        engine.set_target(target);
        let interval = Duration::from_millis(engine.frame_interval_ms());

        // Check if we need to redraw
        let now = std::time::Instant::now();
        if engine.needs_render() && now.duration_since(last_render) >= interval {
            state.tick = state.tick.wrapping_add(1);

            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::BeginSynchronizedUpdate
                );
            }

            state.clear_stale_notice();
            let s = &state;
            if engine
                .terminal_mut()
                .draw(|f| render(f, s, &router, &command_palette))
                .is_err()
            {
                break;
            }

            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::EndSynchronizedUpdate
                );
            }

            engine.mark_clean_for_render();
            last_render = now;
        }

        // Handle input events (non-blocking, ~16ms poll)
        if event::poll(Duration::from_millis(16)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
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
                                engine.mark_dirty();
                            }
                        }
                    } else if let Some(ref modal) = state.modal {
                        // Regular modal key handling
                        match modal::handle_modal_key(key, modal) {
                            ModalAction::Dismiss => {
                                state.modal = None;
                                engine.mark_dirty();
                            }
                            ModalAction::Confirm => {
                                state.modal = None;
                                if key.code == KeyCode::Enter {
                                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                                    break;
                                }
                                engine.mark_dirty();
                            }
                            ModalAction::Retry => {
                                state.modal = None;
                                engine.mark_dirty();
                            }
                            ModalAction::Config => {
                                state.modal = None;
                                state.current_page = PageId::Config;
                                engine.mark_dirty();
                            }
                            ModalAction::Skip => {
                                state.modal = None;
                                engine.mark_dirty();
                            }
                            ModalAction::None => {}
                        }
                    } else if state.show_permission_modal {
                        // Permission modal uses the universal list cursor for
                        // Up/Down; Enter confirms the highlighted option.
                        let mut cursor = permission::cursor(&state);
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                cursor.prev();
                                state.permission_selected = cursor.selected;
                                engine.mark_dirty();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                cursor.next();
                                state.permission_selected = cursor.selected;
                                engine.mark_dirty();
                            }
                            _ => {
                                if let Some(req) = state.permission_request.take() {
                                    let action = match key.code {
                                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                                            PermissionAction::Allow
                                        }
                                        KeyCode::Enter => cursor
                                            .submit()
                                            .map(permission::action_for)
                                            .unwrap_or(PermissionAction::Deny),
                                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                            PermissionAction::Deny
                                        }
                                        _ => PermissionAction::Deny,
                                    };
                                    let _ = req.response_tx.send(action);
                                    state.show_permission_modal = false;
                                    engine.mark_dirty();
                                }
                            }
                        }
                    } else if state.show_command_palette {
                        // Command palette takes priority
                        if command_palette.handle_key(key, &mut state) {
                            state.show_command_palette = false;
                            engine.mark_dirty();
                        } else {
                            engine.mark_dirty();
                        }
                    } else if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        // Ctrl+C: first press cancels a running stage / clears input;
                        // a second press within 2s exits the TUI.
                        if state.has_running_stage() {
                            state.request_cancel("Stopping… (Ctrl+C)");
                        } else {
                            state.input_state.buffer.clear();
                            state.input_state.cursor_pos = 0;
                        }
                        let now = std::time::Instant::now();
                        let exit = match last_ctrl_c {
                            Some(t) => now.duration_since(t) < Duration::from_secs(2),
                            None => false,
                        };
                        last_ctrl_c = Some(now);
                        if exit {
                            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                            break;
                        }
                        engine.mark_dirty();
                    } else if state.show_command_menu {
                        // Slash command menu navigation (universal arrow + Enter model).
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                let mut cursor = command_menu::cursor(&state);
                                cursor.prev();
                                state.command_selected = cursor.selected;
                                engine.mark_dirty();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let mut cursor = command_menu::cursor(&state);
                                cursor.next();
                                state.command_selected = cursor.selected;
                                engine.mark_dirty();
                            }
                            KeyCode::Enter => {
                                if let Some(name) =
                                    crate::display::components::command_menu::get_selected_command(
                                        &state,
                                    )
                                {
                                    state.input_state.buffer = format!("/{}", name);
                                    state.input_state.cursor_pos = state.input_state.buffer.len();
                                    state.input_state.mode = InputMode::Insert;
                                    state.show_command_menu = false;
                                    state.command_filter.clear();
                                    state.command_selected = 0;
                                    let enter =
                                        event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                                    let mut page = chat::ChatPage::new();
                                    page.handle_key(enter, &mut state);
                                }
                                engine.mark_dirty();
                            }
                            KeyCode::Esc => {
                                state.show_command_menu = false;
                                state.command_filter.clear();
                                state.command_selected = 0;
                                state.input_state.mode = InputMode::Insert;
                                engine.mark_dirty();
                            }
                            // All other keys fall through to the input handler so the
                            // filter updates live as the user types.
                            _ => {
                                let mut page = chat::ChatPage::new();
                                if page.handle_key(key, &mut state) {
                                    engine.mark_dirty();
                                }
                            }
                        }
                    } else if key.code == KeyCode::Tab
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        // Toggle between the conversational chat view and the page view.
                        state.current_page = if state.current_page == PageId::Chat {
                            PageId::Run
                        } else {
                            PageId::Chat
                        };
                        engine.mark_dirty();
                    } else if state.current_page == PageId::Chat {
                        // Chat view owns all key handling (input + copy-mode).
                        let mut page = chat::ChatPage::new();
                        if page.handle_key(key, &mut state) {
                            engine.mark_dirty();
                        }
                    } else {
                        // Ctrl-P opens command palette (global, from any page)
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('p')
                        {
                            state.show_command_palette = true;
                            command_palette = CommandPalette::new();
                            engine.mark_dirty();
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
                            engine.mark_dirty();
                        } else if state.current_page == PageId::Run {
                            // On Run page: q/Esc shows quit confirm modal
                            if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                                state.modal = Some(super::pages::Modal::Confirm {
                                    title: "Quit NIKI?".to_string(),
                                    message: "The pipeline will continue in the background."
                                        .to_string(),
                                });
                                engine.mark_dirty();
                            } else if router.handle_key(key, &mut state) {
                                engine.mark_dirty();
                            }
                        } else {
                            // On sub-pages: page-specific key handling
                            if router.handle_key(key, &mut state) {
                                engine.mark_dirty();
                            }
                        }
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    use ratatui::crossterm::event::{MouseButton, MouseEventKind};
                    // Hover (move/drag) moves the highlight; a left press activates.
                    let hovering =
                        matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Drag(_));
                    let clicking = matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left));
                    let full = engine
                        .terminal()
                        .size()
                        .ok()
                        .map(|size| ratatui::layout::Rect::new(0, 0, size.width, size.height));

                    // Route to the active overlay first; chat copy-mode only
                    // sees the mouse when no overlay owns it.
                    match active_focus(&state) {
                        FocusState::Permission => {
                            if let Some(full) = full
                                && let Some(idx) =
                                    permission::click_index(full, mouse.column, mouse.row)
                            {
                                let mut cursor = permission::cursor(&state);
                                if hovering {
                                    if cursor.hover(idx) {
                                        state.permission_selected = cursor.selected;
                                        engine.mark_dirty();
                                    }
                                } else if clicking {
                                    if let Some(i) = cursor.click(idx) {
                                        state.permission_selected = i;
                                        if let Some(req) = state.permission_request.take() {
                                            let _ = req.response_tx.send(permission::action_for(i));
                                            state.show_permission_modal = false;
                                        }
                                        engine.mark_dirty();
                                    }
                                }
                            }
                        }
                        FocusState::CommandPalette => {
                            if let Some(full) = full
                                && let Some(idx) = super::command_palette::click_index(
                                    &command_palette,
                                    full,
                                    mouse.column,
                                    mouse.row,
                                )
                            {
                                if hovering {
                                    if command_palette.hover(idx) {
                                        engine.mark_dirty();
                                    }
                                } else if clicking && command_palette.click(idx, &mut state) {
                                    state.show_command_palette = false;
                                    engine.mark_dirty();
                                }
                            }
                        }
                        FocusState::CommandMenu => {
                            if let Some(full) = full
                                && let Some(idx) =
                                    command_menu::click_index(&state, full, mouse.column, mouse.row)
                            {
                                let mut cursor = command_menu::cursor(&state);
                                let changed = if hovering {
                                    cursor.hover(idx)
                                } else if clicking {
                                    cursor.click(idx).is_some()
                                } else {
                                    false
                                };
                                if changed {
                                    state.command_selected = cursor.selected;
                                    engine.mark_dirty();
                                }
                            }
                        }
                        FocusState::Chat => {
                            if state.current_page == PageId::Chat
                                && let Some(full) = full
                            {
                                let chunks = Layout::default()
                                    .direction(Direction::Vertical)
                                    .constraints([
                                        Constraint::Length(8),
                                        Constraint::Min(5),
                                        Constraint::Length(1),
                                    ])
                                    .split(full);
                                chat::ChatPage::handle_mouse(&mut state, mouse, chunks[1]);
                                engine.mark_dirty();
                            }
                        }
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    // Terminal was resized — force a re-render so the layout
                    // reflows to the new dimensions (ratatui re-samples size on draw).
                    engine.mark_dirty();
                }
                _ => {}
            }
        }

        // Drain events from the pipeline — mark dirty on any state change
        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(ev) => {
                state.apply_event(ev);
                engine.mark_dirty();
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
    let _ = engine
        .terminal_mut()
        .draw(|f| render(f, &state, &router, &command_palette));

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
        && (term.contains("kitty") || term.contains("ghostty") || term.contains("xterm"))
    {
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

/// Run the TUI in interactive chat mode (no pipeline events).
/// Used by `niki chat` — the channel is held by the caller so it never disconnects.
pub fn run_chat(
    rx: Receiver<DisplayEvent>,
    description: String,
    project_path: PathBuf,
    on_submit: Option<mpsc::Sender<String>>,
) {
    let _guard = RestoreGuard;

    if enable_raw_mode().is_err() {
        return;
    }
    let mut stdout = io::stdout();
    let _ = execute!(stdout, EnterAlternateScreen, EnableMouseCapture);
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");

    let sync_capable = detect_synchronized_output();

    let config = crate::config::NikiConfig::load(&project_path).unwrap_or_default();
    let mut state = AppState::new(description, config, project_path.clone());
    state.current_page = PageId::Chat;

    if onboarding::should_show_onboarding(&project_path) {
        state.onboarding = Some(onboarding::OnboardingModal::new());
    } else {
        state.onboarded = onboarding::load_state(&project_path);
    }

    let mut command_palette = CommandPalette::new();
    let mut router = PageRouter::new();

    let mut last_frame = std::time::Instant::now();
    let min_frame_interval = std::time::Duration::from_millis(33);
    let mut needs_render = true;

    // Resume persisted chat session (Phase 8 — persistence + resume).
    if let Some(session) = persistence::load_chat_session(&project_path) {
        persistence::apply_session(&mut state, session);
        needs_render = true;
    }

    loop {
        if needs_render {
            state.tick();
            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::BeginSynchronizedUpdate
                );
            }
            terminal
                .draw(|f| render(f, &state, &router, &command_palette))
                .ok();
            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::EndSynchronizedUpdate
                );
            }
            needs_render = false;
            last_frame = std::time::Instant::now();
        }

        let timeout = min_frame_interval.saturating_sub(last_frame.elapsed());
        if event::poll(timeout).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if let Some(ref mut onboard) = state.onboarding {
                    match onboard.handle_key(key) {
                        OnboardingAction::None => {}
                        OnboardingAction::Skip | OnboardingAction::Finish => {
                            if onboard.dont_show_again {
                                onboarding::persist_state(&project_path);
                                state.onboarded = true;
                            }
                            state.onboarding = None;
                            needs_render = true;
                        }
                    }
                    continue;
                }

                if let Some(ref modal) = state.modal.clone() {
                    match modal::handle_modal_key(key, modal) {
                        ModalAction::Dismiss | ModalAction::Skip => {
                            state.modal = None;
                            needs_render = true;
                        }
                        ModalAction::Confirm | ModalAction::Retry => {
                            break;
                        }
                        ModalAction::Config => {
                            state.current_page = PageId::Config;
                            state.modal = None;
                            needs_render = true;
                        }
                        ModalAction::None => {}
                    }
                    continue;
                }

                if key.code == KeyCode::Char('p') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    state.show_command_palette = !state.show_command_palette;
                    if state.show_command_palette {
                        command_palette = CommandPalette::new();
                    }
                    needs_render = true;
                    continue;
                }
                if state.show_command_palette {
                    if command_palette.handle_key(key, &mut state) {
                        state.show_command_palette = false;
                        needs_render = true;
                    }
                    continue;
                }

                if key.code == KeyCode::Tab {
                    state.current_page = match state.current_page {
                        PageId::Chat => PageId::Run,
                        _ => PageId::Chat,
                    };
                    needs_render = true;
                    continue;
                }

                if state.current_page == PageId::Chat {
                    let before = state.chat_log.len();
                    let mut chat_page = chat::ChatPage::new();
                    chat_page.handle_key(key, &mut state);
                    needs_render = true;
                    // Forward any newly submitted user message to the session
                    // processor (Phase 6 — user messages mid-session).
                    if state.chat_log.len() > before {
                        if let Some((role, text)) = state.chat_log.last() {
                            if role == "user" {
                                if let Some(tx) = &on_submit {
                                    let _ = tx.send(text.clone());
                                }
                            }
                        }
                    }
                    persistence::save_chat_session(&project_path, &persistence::snapshot(&state));
                    continue;
                }

                match key.code {
                    KeyCode::Char('t') if key.modifiers.is_empty() => {
                        let new_pref = match state.config.ui.theme {
                            crate::config::types::ThemePreference::Dark => {
                                crate::config::types::ThemePreference::Light
                            }
                            crate::config::types::ThemePreference::Light => {
                                crate::config::types::ThemePreference::Auto
                            }
                            crate::config::types::ThemePreference::Auto => {
                                crate::config::types::ThemePreference::Dark
                            }
                        };
                        let mode = match new_pref {
                            crate::config::types::ThemePreference::Dark => theme::ThemeMode::Dark,
                            crate::config::types::ThemePreference::Light => theme::ThemeMode::Light,
                            crate::config::types::ThemePreference::Auto => theme::ThemeMode::Auto,
                        };
                        theme::set_mode(mode);
                        state.config.ui.theme = new_pref;
                        needs_render = true;
                    }
                    KeyCode::Char('q') => {
                        state.modal = Some(crate::display::pages::Modal::Confirm {
                            title: "Quit".into(),
                            message: "Exit NIKI?".into(),
                        });
                        needs_render = true;
                    }
                    _ => {
                        if router.handle_key(key, &mut state) {
                            needs_render = true;
                        }
                    }
                }
            } else if let Ok(Event::Mouse(mouse)) = event::read() {
                if state.current_page == PageId::Chat {
                    let size = terminal.size().unwrap_or(ratatui::layout::Size {
                        width: 80,
                        height: 24,
                    });
                    chat::ChatPage::handle_mouse(
                        &mut state,
                        mouse,
                        Rect::new(0, 0, size.width, size.height),
                    );
                    needs_render = true;
                }
            } else if let Ok(Event::Resize(_, _)) = event::read() {
                needs_render = true;
            }
        }

        while let Ok(ev) = rx.try_recv() {
            state.apply_event(ev);
            needs_render = true;
        }

        if last_frame.elapsed() >= min_frame_interval {
            needs_render = true;
        }
    }

    // Phase 8 — persist the final chat session on exit (resume next time).
    persistence::save_chat_session(&project_path, &persistence::snapshot(&state));

    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    if sync_capable {
        let _ = execute!(
            io::stdout(),
            ratatui::crossterm::terminal::EndSynchronizedUpdate
        );
    }
}

fn render_status_line(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, state: &AppState) {
    // Delegate to the shared status-bar component (was dead-island component, now
    // wired into the live loop). It reads the canonical AppState fields.
    super::components::status_bar::render_status_bar(frame, state, area);
}

/// Render a spinner + running-stage + progress indicator in the status area while
/// a pipeline stage is active (ties the dead spinner/progress component to the
/// live render loop).
fn render_activity_spinner(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    state: &AppState,
) {
    use ratatui::text::Line;
    let running = state
        .stages
        .iter()
        .filter(|s| s.status == crate::display::state::StageStatus::Running)
        .count();
    let done = state
        .stages
        .iter()
        .filter(|s| s.status == crate::display::state::StageStatus::Done)
        .count();
    let total = state.stages.len();
    let progress = if total > 0 {
        done as f64 / total as f64
    } else {
        0.0
    };
    let bar = crate::display::components::render_progress_bar(
        frame,
        area,
        progress,
        (area.width as usize).saturating_sub(24),
    );
    let spinner = crate::display::components::SpinnerState::with_tick(state.tick);
    let mut spans = vec![spinner.render()];
    spans.push(ratatui::text::Span::styled(
        format!(
            " running ({} stage{})",
            running,
            if running == 1 { "" } else { "s" }
        ),
        ratatui::style::Style::default().fg(crate::display::theme::text_dim()),
    ));
    spans.push(ratatui::text::Span::styled(
        "  ".to_string(),
        ratatui::style::Style::default(),
    ));
    spans.extend(bar.spans);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(spans)),
        ratatui::layout::Rect::new(area.x, area.y, area.width, 1),
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

fn render(
    frame: &mut ratatui::Frame,
    state: &AppState,
    router: &PageRouter,
    command_palette: &CommandPalette,
) {
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
            Constraint::Length(8), // logo (6 lines + 2 padding)
            Constraint::Min(5),    // page content
            Constraint::Length(1), // status line (footer meta)
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

    // Render slash command menu overlay if present (was dead component — now live)
    if state.show_command_menu {
        super::components::render_command_menu(frame, size, state);
    }

    // Render @ file autocomplete overlay if present (was dead component — now live)
    if state.input_state.autocomplete.is_some() {
        super::components::render_autocomplete(frame, size, state);
    }

    // Render permission modal overlay if present (was dead component — now live)
    if state.show_permission_modal {
        if let Some(ref req) = state.permission_request {
            super::components::render_permission_modal(frame, req, size, state);
        }
    }

    // Render a spinner/progress indicator while stages are running
    if state.has_running_stage() {
        render_activity_spinner(frame, size, state);
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

    #[test]
    fn active_focus_priority() {
        let config = crate::config::types::NikiConfig::default();
        let mut state = AppState::new("test".into(), config, ".".into());
        assert_eq!(active_focus(&state), FocusState::Chat);

        state.show_command_menu = true;
        assert_eq!(active_focus(&state), FocusState::CommandMenu);

        state.show_command_palette = true;
        assert_eq!(active_focus(&state), FocusState::CommandPalette);

        state.show_permission_modal = true;
        assert_eq!(active_focus(&state), FocusState::Permission);
        assert!(active_focus(&state).is_overlay());
    }
}
