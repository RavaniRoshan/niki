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
use crate::permissions::PermissionAction;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;

use super::command_palette::CommandPalette;
use super::components::command_menu;
use super::components::list_cursor::FocusState;
use super::components::permission;
use super::keybindings::GlobalAction;
use super::modal::{self, ModalAction};
use super::onboarding::{self, OnboardingAction};
use super::pages::chat;
use super::pages::{AppState, HoverTarget, Page, PageId, PageRouter};
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
    /// A tool call was dispatched to the sandbox — render a pending card.
    /// `summary` is the one-line description (command string, file path, etc.).
    ToolCall {
        role: AgentRole,
        tool_name: String,
        summary: String,
    },
    /// A tool call completed (success or failure). The TUI updates the matching
    /// card by `tool_name` + insertion order (first unmatched pending/running
    /// card of that name). `output` is stdout/stderr on success, or empty.
    ToolResult {
        role: AgentRole,
        tool_name: String,
        success: bool,
        error: Option<String>,
        output: Option<String>,
        duration_ms: u64,
    },
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
        let _ = crate::display::kitty::disable_kitty_keyboard();
        let _ = crate::display::mouse::disable_tracking();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            ratatui::crossterm::event::DisableBracketedPaste
        );
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

/// The overlay ladder: the first open overlay owns the keyboard.
///
/// Returns `true` when the key was consumed and the caller must not route it
/// any further. This is the top of key dispatch, and it now exists once for
/// both loops rather than as two hand-ordered chains that could disagree.
///
/// The order is a decision, not an accident:
/// 1. **Onboarding** and **permission** first. Both are states the program
///    cannot proceed past, and a permission prompt arriving while a help
///    overlay is up must not be hidden behind it — a security question the
///    user cannot see is one they cannot answer.
/// 2. **Help** next: a full-screen overlay that swallows everything below it.
/// 3. **Modal** and the **command palette** last; they sit above a page rather
///    than above each other.
///
/// `run_tui` used to check help *before* onboarding and `run_chat` after it.
/// That difference is now impossible to express.
fn route_overlay_key(
    state: &mut AppState,
    command_palette: &mut CommandPalette,
    key: ratatui::crossterm::event::KeyEvent,
    project_path: &std::path::Path,
) -> OverlayOutcome {
    use ratatui::crossterm::event::KeyCode;

    if let Some(ref mut onboard) = state.onboarding {
        match onboard.handle_key(key) {
            OnboardingAction::None => {}
            OnboardingAction::Skip | OnboardingAction::Finish => {
                if onboard.dont_show_again {
                    onboarding::persist_state(project_path);
                    state.onboarded = true;
                }
                state.onboarding = None;
            }
        }
        return OverlayOutcome::Consumed;
    }

    if permission::handle_key(&key, state) {
        return OverlayOutcome::Consumed;
    }

    // The two globals that must work over a help overlay: they toggle it, and
    // the toggle is the only way out of it.
    if state.keybindings.resolve(&key) == Some(GlobalAction::ToggleHelp) {
        state.show_help = !state.show_help;
        return OverlayOutcome::Consumed;
    }
    if state.keybindings.resolve(&key) == Some(GlobalAction::ToggleMouseCapture) {
        state.mouse_capture = !state.mouse_capture;
        if state.mouse_capture {
            let _ = ratatui::crossterm::execute!(std::io::stdout(), EnableMouseCapture);
            let _ = crate::display::mouse::enable_tracking();
        } else {
            let _ = crate::display::mouse::disable_tracking();
            let _ = ratatui::crossterm::execute!(std::io::stdout(), DisableMouseCapture);
        }
        return OverlayOutcome::Consumed;
    }

    if state.show_help {
        if key.code == KeyCode::Esc {
            state.show_help = false;
        }
        return OverlayOutcome::Consumed;
    }

    if let Some(ref modal) = state.modal.clone() {
        match modal::handle_modal_key(key, modal) {
            ModalAction::Dismiss => {
                state.modal = None;
                return OverlayOutcome::Consumed;
            }
            ModalAction::Confirm | ModalAction::Retry => return OverlayOutcome::Quit,
            ModalAction::Config => {
                state.current_page = PageId::Config;
                state.modal = None;
                return OverlayOutcome::Consumed;
            }
            ModalAction::None => {}
        }
    }

    // Through the table, like the two globals above. `run_chat` used to test a
    // literal Ctrl+P, so a user who rebound `command_palette` in
    // `niki.toml` got a binding that did nothing there and worked everywhere
    // else.
    if state.keybindings.resolve(&key) == Some(GlobalAction::CommandPalette) {
        state.show_command_palette = !state.show_command_palette;
        if state.show_command_palette {
            *command_palette = CommandPalette::new();
        }
        return OverlayOutcome::Consumed;
    }
    if state.show_command_palette {
        if command_palette.handle_key(key, state) {
            state.show_command_palette = false;
        }
        // Both loops mirror the cursor into `state.command_selected`; without
        // it the status bar names the first command while the highlight sits
        // on the fourth.
        state.command_selected = command_palette.cursor.selected;
        return OverlayOutcome::Consumed;
    }

    OverlayOutcome::Free
}

/// What [`route_overlay_key`] decided about a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayOutcome {
    /// An overlay had it; the page below must not see it.
    Consumed,
    /// No overlay wanted it.
    Free,
    /// A modal asked to leave.
    Quit,
}

/// Route one mouse event to whatever owns it.
///
/// Returns `true` when the screen needs redrawing. This used to be a 378-line
/// arm inside `run_tui`'s `match`, which made it untestable: there was no
/// seam to drive a click through, which is why the phantom tab-bar handlers
/// that sat in here could be exercised only by a test of the hit-test
/// function and never by a test of the thing a user actually does.
///
/// The overlays still take the mouse in priority order — help, then whatever
/// `active_focus` names, then the chat page — and that order is the point of
/// having it in one function.
#[allow(clippy::too_many_lines)]
fn route_mouse(
    state: &mut AppState,
    router: &mut super::pages::PageRouter,
    command_palette: &mut CommandPalette,
    mouse: ratatui::crossterm::event::MouseEvent,
    full: Option<ratatui::layout::Rect>,
) -> bool {
    let mut dirty = false;
    use ratatui::crossterm::event::{MouseButton, MouseEventKind};
    // Clicking anywhere dismisses the help overlay.
    if state.show_help {
        state.show_help = false;
        return true;
    }
    // Hover (move/drag) moves the highlight; a left press activates.
    let hovering = matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Drag(_));
    let clicking = matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left));
    let scrolling_up = matches!(mouse.kind, MouseEventKind::ScrollUp);
    let scrolling_down = matches!(mouse.kind, MouseEventKind::ScrollDown);

    // Route to the active overlay first; chat copy-mode only
    // sees the mouse when no overlay owns it.
    match active_focus(state) {
        FocusState::Permission => {
            if scrolling_up || scrolling_down {
                let mut cursor = permission::cursor(state);
                if scrolling_up {
                    cursor.prev();
                } else {
                    cursor.next();
                }
                state.permission_selected = cursor.selected;
                dirty = true;
            } else if let Some(full) = full {
                // Geometry comes from the request + detail flag,
                // shared with the renderer (TUI-013).
                let show_detail = state.show_permission_detail;
                let hit = state.permission_request.as_ref().and_then(|req| {
                    permission::click_index(full, mouse.column, mouse.row, req, show_detail)
                });
                if let Some(idx) = hit {
                    let mut cursor = permission::cursor(state);
                    if hovering {
                        if cursor.hover(idx) {
                            state.permission_selected = cursor.selected;
                            dirty = true;
                        }
                    } else if clicking {
                        if let Some(i) = cursor.click(idx) {
                            state.permission_selected = i;
                            if let Some(req) = state.permission_request.take() {
                                let _ = req.response_tx.send(permission::action_for(i));
                                state.show_permission_modal = false;
                            }
                            dirty = true;
                        }
                    }
                }
            }
        }
        FocusState::CommandPalette => {
            if scrolling_up || scrolling_down {
                if scrolling_up {
                    command_palette.cursor.prev();
                } else {
                    command_palette.cursor.next();
                }
                state.command_selected = command_palette.cursor.selected;
                dirty = true;
            } else if let Some(full) = full
                && let Some(idx) = super::command_palette::click_index(
                    command_palette,
                    full,
                    mouse.column,
                    mouse.row,
                )
            {
                if hovering {
                    if command_palette.hover(idx) {
                        dirty = true;
                    }
                } else if clicking && command_palette.click(idx, state) {
                    state.show_command_palette = false;
                    dirty = true;
                }
            }
        }
        FocusState::CommandMenu => {
            if scrolling_up || scrolling_down {
                let mut cursor = command_menu::cursor(state);
                if scrolling_up {
                    cursor.prev();
                } else {
                    cursor.next();
                }
                state.command_selected = cursor.selected;
                dirty = true;
            } else if let Some(full) = full
                && let Some(idx) = command_menu::click_index(state, full, mouse.column, mouse.row)
            {
                state.command_selected = idx;
                // Execute the command on click (same as Enter)
                if let Some(name) =
                    crate::display::components::command_menu::get_selected_command(state)
                {
                    state.input_state.buffer = format!("/{}", name);
                    state.input_state.cursor_pos = state.input_state.buffer.len();
                    state.input_state.mode = InputMode::Insert;
                    state.show_command_menu = false;
                    state.command_filter.clear();
                    state.command_selected = 0;
                    let enter = event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                    let mut page = chat::ChatPage::new();
                    page.handle_key(enter, state);
                }
                dirty = true;
            }
        }
        FocusState::Chat => {
            // Open tool-detail modal owns left-clicks (TUI-013):
            // inside is consumed, outside dismisses. The wheel
            // path below chains into the modal scroll instead.
            if state.tool_detail_index.is_some() && clicking {
                if let Some(full) = full {
                    super::components::tool_detail::route_click(
                        state,
                        mouse.column,
                        mouse.row,
                        full,
                    );
                    dirty = true;
                }
                return dirty;
            }
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
                // Scroll wheel with innermost-first chaining (TUI-010):
                // an open tool-detail modal consumes the wheel
                // first; the remainder scrolls the chat behind it.
                if scrolling_up || scrolling_down {
                    let delta = if scrolling_up { -3 } else { 3 };
                    let mut rest = delta;
                    if let Some(idx) = state.tool_detail_index {
                        if let Some(card) = state.tool_cards.get(idx) {
                            let content =
                                super::components::tool_detail::detail_content_lines(card);
                            let viewport = super::components::tool_detail::detail_viewport(full);
                            rest = state.tool_detail_scroll.scroll_by(rest, content, viewport);
                        }
                    }
                    if rest != 0 {
                        let total = state.chat_lines.len();
                        let visible = chunks[1].height as usize;
                        state.chat_scroll.scroll_by(rest, total, visible);
                    }
                    dirty = true;
                } else {
                    // Scrollbar click/drag-to-jump (gaps P0 — "Drag to scroll").
                    let msg_area_h = chunks[1].height.saturating_sub(3) as usize;
                    let sb_col = chunks[1].x + chunks[1].width.saturating_sub(1);
                    let on_scrollbar = (clicking || matches!(mouse.kind, MouseEventKind::Drag(_)))
                        && mouse.column == sb_col
                        && mouse.row >= chunks[1].y
                        && mouse.row < chunks[1].y + chunks[1].height.saturating_sub(3);
                    if on_scrollbar {
                        let total = state.chat_lines.len();
                        if total > msg_area_h && msg_area_h > 0 {
                            let frac = (mouse.row - chunks[1].y) as f64 / msg_area_h as f64;
                            let target = (frac * total as f64).round() as usize;
                            state.chat_scroll.jump_to(target, total, msg_area_h);
                            dirty = true;
                        }
                    } else if hovering {
                        // Hover hit-test for chat elements
                        let row = mouse.row.saturating_sub(chunks[1].y) as usize;
                        let total = state.chat_lines.len();
                        let visible = chunks[1].height as usize;
                        let offset = state.chat_scroll.view_offset(total, visible);
                        let abs_row = offset + row;
                        let new_target = if row < chunks[1].height as usize
                            && let Some(line) = state.chat_lines.get(abs_row)
                        {
                            if line.header_stage.is_some() {
                                HoverTarget::StageHeader(line.header_stage.unwrap_or(0))
                            } else if line.is_input {
                                HoverTarget::InputBox
                            } else if line.msg_index != usize::MAX {
                                HoverTarget::ChatMessage(line.msg_index)
                            } else {
                                HoverTarget::None
                            }
                        } else {
                            HoverTarget::None
                        };
                        if state.hover_target != new_target {
                            state.hover_target = new_target;
                            state.hover_time = Some(std::time::Instant::now());
                            dirty = true;
                        }
                    } else {
                        chat::ChatPage::handle_mouse(state, mouse, chunks[1]);
                        dirty = true;
                    }
                }
                // Click-to-position cursor in input box
                if clicking && state.current_page == PageId::Chat {
                    let input_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Min(3), Constraint::Length(3)])
                        .split(chunks[1]);
                    if super::components::input_box::handle_click(
                        state,
                        mouse.column,
                        input_chunks[1],
                    ) {
                        dirty = true;
                    }
                }
            }
        }
    }
    // Modal click handling (always active when modal is present)
    if clicking
        && let Some(ref modal) = state.modal
        && let Some(full) = full
    {
        if let Some(action) = modal::modal_hit_test(mouse.column, mouse.row, full, modal) {
            match action {
                ModalAction::Confirm => {
                    state.modal = None;
                    if let Some(req) = state.permission_request.take() {
                        let _ = req
                            .response_tx
                            .send(crate::permissions::PermissionAction::Allow);
                    }
                    dirty = true;
                }
                ModalAction::Retry => {
                    state.modal = None;
                    // Retry is handled by the key handler
                    dirty = true;
                }
                ModalAction::Config => {
                    state.modal = None;
                    state.current_page = PageId::Config;
                    dirty = true;
                }
                ModalAction::Dismiss => {
                    state.modal = None;
                    dirty = true;
                }
                ModalAction::None => {}
            }
        }
    }
    // Status bar: hover *and* click, always active regardless of overlay focus.
    //
    // The whole block used to be `if hovering && ...`, with the click handler
    // nested inside it. But `hovering` is Moved/Drag and `clicking` is
    // Down(Left) — mutually exclusive — so the click branch could never run.
    // The status bar highlighted on hover and told the user to click the mode
    // badge, and nothing happened when they did. Only extracting this into a
    // testable function made it findable: the test drove a click and the mode
    // did not change.
    if (hovering || clicking)
        && let Some(full) = full
    {
        let status_area = Rect {
            x: 0,
            y: full.height.saturating_sub(1),
            width: full.width,
            height: 1,
        };
        if mouse.row == status_area.y {
            let new_target =
                super::components::status_bar::hover_test(mouse.column, status_area, state);
            if hovering {
                if state.hover_target != new_target {
                    state.hover_target = new_target;
                    state.hover_time = Some(std::time::Instant::now());
                    dirty = true;
                }
            }
            if clicking {
                // Handle status bar clicks
                match new_target {
                    HoverTarget::StatusBarMode => {
                        // Cycle permission modes
                        state.permission_mode = match state.permission_mode {
                            crate::display::state::PermissionMode::Default => {
                                crate::display::state::PermissionMode::AcceptEdits
                            }
                            crate::display::state::PermissionMode::AcceptEdits => {
                                crate::display::state::PermissionMode::Plan
                            }
                            crate::display::state::PermissionMode::Plan => {
                                crate::display::state::PermissionMode::Auto
                            }
                            crate::display::state::PermissionMode::Auto => {
                                crate::display::state::PermissionMode::DontAsk
                            }
                            crate::display::state::PermissionMode::DontAsk => {
                                crate::display::state::PermissionMode::BypassPermissions
                            }
                            crate::display::state::PermissionMode::BypassPermissions => {
                                crate::display::state::PermissionMode::Default
                            }
                        };
                        state.set_notice(
                            &format!("Permission mode: {:?}", state.permission_mode),
                            1500,
                        );
                        dirty = true;
                    }
                    _ => {}
                }
            }
        } else if matches!(
            state.hover_target,
            HoverTarget::StatusBarMode
                | HoverTarget::StatusBarCost
                | HoverTarget::StatusBarBranch
                | HoverTarget::StatusBarCtx
        ) {
            // Mouse left the status bar
            state.hover_target = HoverTarget::None;
            dirty = true;
        }
    }
    // There is no tab bar. `layout::render_page` was the only
    // thing that drew one and it has no callers, so the click
    // and hover handlers here were responding to a region with
    // no pixels in it: clicking the top row of the screen
    // teleported the user to an arbitrary page, and a test
    // exercised the hit-test against a bar that is never drawn.
    if clicking
        && state.current_page == PageId::Fleet
        && let Some(full) = full
    {
        if state.fleet.handle_click(mouse.column, mouse.row, full) {
            dirty = true;
        }
    }
    // Scroll wheel for non-chat pages (sends synthetic Up/Down keys)
    if (scrolling_up || scrolling_down) && state.current_page != PageId::Chat {
        let key_code = if scrolling_up {
            KeyCode::Up
        } else {
            KeyCode::Down
        };
        let key = KeyEvent::new(key_code, KeyModifiers::NONE);
        router.handle_key(key, state);
        dirty = true;
    }
    // Click feedback flash (brief visual indicator on any click)
    if clicking {
        state.trigger_click_flash((mouse.column, mouse.row));
        dirty = true;
    }
    dirty
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
    // Enable mouse capture and bracketed paste mode
    let _ = execute!(
        io::stdout(),
        EnableMouseCapture,
        ratatui::crossterm::event::EnableBracketedPaste
    );
    // TUI-030: button-motion + SGR coordinates so drags (scrollbar,
    // selection) and hover highlights actually arrive. Best-effort.
    let _ = crate::display::mouse::enable_tracking();
    // Progressive adoption of the Kitty keyboard protocol (I4): disambiguates
    // Shift+Enter from Enter on supporting terminals. Disabled on exit.
    if crate::display::kitty::kitty_capable() {
        let _ = crate::display::kitty::enable_kitty_keyboard();
    }

    // Best-effort DEC 2026 synchronized output — eliminates flicker on
    // supporting terminals (kitty, Ghostty, xterm.js ≥6.0, newer tmux).
    // The bracket is opened/closed around each frame draw inside the render
    // loop, so we do NOT open it here: leaving it open would desync the
    // bracket and make the trailing End at exit unmatched.
    let sync_capable = detect_synchronized_output();

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(_) => return,
    };
    let mut engine = super::engine::RenderEngine::new(terminal, sync_capable);

    // Initial full draw
    engine.begin_frame();
    let _ = engine.terminal_mut().draw(|f| {
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
    engine.end_frame();
    engine.mark_clean_for_render();

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
    engine.mark_dirty();
    let mut last_render = std::time::Instant::now();
    // Tracks the previous Ctrl+C press for the two-press-to-exit behaviour.
    let mut last_ctrl_c: Option<std::time::Instant> = None;
    // Frame counter for the TUI-00D debug log.
    let mut frame_no: u64 = 0;

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
            state.clear_stale_click_flash();
            // Throttled: the mission-store round-trip is pure overhead at
            // 30-60fps; the grid only needs ~2Hz freshness in the loop.
            state.refresh_fleet_if_stale(Duration::from_millis(500));
            let s = &state;
            engine.begin_frame();
            if engine
                .terminal_mut()
                .draw(|f| render(f, s, &router, &command_palette))
                .is_err()
            {
                break;
            }
            engine.end_frame();

            if sync_capable {
                let _ = execute!(
                    io::stdout(),
                    ratatui::crossterm::terminal::EndSynchronizedUpdate
                );
            }

            // TUI-00D: publish rolling frame stats for the Cost page and the
            // NIKI_TUI_DEBUG per-frame log. Read the dirty reason before the
            // clean call below clears it.
            let stats = engine.stats();
            let frame_ms = stats.mean().as_secs_f64() * 1000.0;
            state.frame_mean_ms = frame_ms;
            state.frame_p95_ms = stats.p95().as_secs_f64() * 1000.0;
            frame_no += 1;
            if crate::display::debug::enabled() {
                let target = match target {
                    crate::display::engine::FrameTarget::High => "High",
                    crate::display::engine::FrameTarget::Low => "Low",
                };
                crate::display::debug::log_frame(
                    frame_no,
                    target,
                    now.elapsed().as_secs_f64() * 1000.0,
                    engine.dirty_reason().unwrap_or("unknown"),
                    state.stages.len(),
                    state.token_count,
                    state.frame_mean_ms,
                    state.frame_p95_ms,
                );
            }

            engine.mark_clean_for_render();
            last_render = now;
        }

        // Handle input events (non-blocking, ~16ms poll)
        if event::poll(Duration::from_millis(16)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
                    // Tag the frame reason up front; inner handlers use plain
                    // mark_dirty(), which preserves this explicit reason.
                    engine.mark_dirty_reason("key");
                    // Global keys that work even inside chat input (TUI-003:
                    // resolved through the central keybinding table).
                    // One overlay ladder for both loops. The inline chain
                    // this replaces checked the help overlay before onboarding
                    // and permission after; `run_chat` had the opposite order.
                    // Ordering is now a single decision, stated in one place.
                    match route_overlay_key(&mut state, &mut command_palette, key, &project_path) {
                        OverlayOutcome::Consumed => {
                            engine.mark_dirty();
                            continue;
                        }
                        OverlayOutcome::Quit => break,
                        OverlayOutcome::Free => {}
                    }
                    if state.keybindings.resolve(&key) == Some(GlobalAction::CancelOrExit) {
                        // Ctrl+C: first press cancels a running stage / clears input;
                        // a second press within 2s exits the TUI.
                        if state.has_running_stage() {
                            state.request_cancel("Stopping… (Ctrl+C again to exit)");
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
                        } else if !state.has_running_stage() {
                            state.set_notice("Press Ctrl+C again to exit", 3000);
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
                    } else if state.keybindings.resolve(&key) == Some(GlobalAction::ToggleChatPage)
                    {
                        // Toggle between the conversational chat view and the page view.
                        //
                        // This has to move `view`, not just `current_page`. `view` is
                        // what decides what is rendered; `current_page` alone left the
                        // app permanently in the chat view, so the footer kept saying
                        // "tab pages", Tab appeared to do nothing, and every page
                        // navigation key silently changed an invisible field.
                        state.view = match state.view {
                            crate::display::state::ViewMode::Chat => {
                                let page = if state.current_page == PageId::Chat {
                                    PageId::Run
                                } else {
                                    state.current_page
                                };
                                state.current_page = page;
                                crate::display::state::ViewMode::Page(page)
                            }
                            crate::display::state::ViewMode::Page(_) => {
                                crate::display::state::ViewMode::Chat
                            }
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
                        if state.keybindings.resolve(&key) == Some(GlobalAction::CommandPalette) {
                            state.show_command_palette = true;
                            command_palette = CommandPalette::new();
                            engine.mark_dirty();
                        } else if state.keybindings.resolve(&key) == Some(GlobalAction::CycleTheme)
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
                            } else if let Some(page) = global_page_jump(key) {
                                state.current_page = page;
                                engine.mark_dirty();
                            }
                        } else if state.current_page == PageId::Fleet {
                            if handle_fleet_nav(key, &mut state) {
                                engine.mark_dirty();
                            } else if let Some(page) = global_page_jump(key) {
                                state.current_page = page;
                                engine.mark_dirty();
                            }
                        } else if state.current_page == PageId::Session {
                            if handle_session_nav(key, &mut state) {
                                engine.mark_dirty();
                            } else if let Some(page) = global_page_jump(key) {
                                state.current_page = page;
                                engine.mark_dirty();
                            }
                        } else if let Some(intent) = crate::display::nav::intent_from_key(
                            &key,
                            crate::display::nav::text_focus_active(&state),
                        ) {
                            // Arrow / hjkl / digit navigation. Placed after the
                            // page-specific router above so a page's own keys
                            // still win, and gated on text focus so the composer
                            // keeps its arrows.
                            use crate::display::nav::{Dir, NavIntent};
                            match intent {
                                NavIntent::Page(Dir::Next) => {
                                    state.current_page =
                                        crate::display::nav::next_page(state.current_page);
                                    engine.mark_dirty();
                                }
                                NavIntent::Page(Dir::Prev) => {
                                    state.current_page =
                                        crate::display::nav::prev_page(state.current_page);
                                    engine.mark_dirty();
                                }
                                NavIntent::Select(dir) => {
                                    let len = crate::display::nav::page_item_count(&state);
                                    let next = crate::display::nav::step_index(
                                        len,
                                        state.page_selection,
                                        dir,
                                    );
                                    if next != state.page_selection {
                                        state.page_selection = next;
                                        engine.mark_dirty();
                                    }
                                }
                                NavIntent::GotoPage(n) => {
                                    if let Some(page) = crate::display::nav::goto_page(n) {
                                        state.current_page = page;
                                        engine.mark_dirty();
                                    }
                                }
                                NavIntent::Quit => {
                                    // Mirror the Ctrl+C exit path: signal the
                                    // run to stop first, so quitting does not
                                    // leave a stage running.
                                    if let Some(c) = state.cancel.clone() {
                                        c.store(true, std::sync::atomic::Ordering::Relaxed);
                                    }
                                    engine.mark_dirty();
                                    break;
                                }
                            }
                        } else if state.keybindings.resolve(&key) == Some(GlobalAction::GotoFleet) {
                            // 'g' jumps to the Fleet grid from any page.
                            state.current_page = PageId::Fleet;
                            engine.mark_dirty();
                        } else if state.keybindings.resolve(&key) == Some(GlobalAction::GotoSession)
                        {
                            // 's' opens the Session view (falls back to the Fleet
                            // selection when nothing is open yet).
                            if state.session_view.is_none() {
                                state.open_selected_mission();
                            } else {
                                state.current_page = PageId::Session;
                            }
                            engine.mark_dirty();
                        } else {
                            // On sub-pages: page-specific key handling first —
                            // page bindings always win over global jumps.
                            if router.handle_key(key, &mut state) {
                                engine.mark_dirty();
                            } else if let Some(page) = global_page_jump(key) {
                                state.current_page = page;
                                engine.mark_dirty();
                            }
                        }
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    let full = engine
                        .terminal()
                        .size()
                        .ok()
                        .map(|size| ratatui::layout::Rect::new(0, 0, size.width, size.height));
                    if route_mouse(&mut state, &mut router, &mut command_palette, mouse, full) {
                        engine.mark_dirty();
                        continue;
                    }
                }
                Ok(Event::Paste(pasted)) => {
                    state.input_state.insert_str(&pasted);
                    state.input_state.start_paste_burst();
                    engine.mark_dirty_reason("paste");
                }
                Ok(Event::Resize(_, _)) => {
                    // Terminal was resized — force a re-render so the layout
                    // reflows to the new dimensions (ratatui re-samples size on draw).
                    engine.mark_dirty_reason("resize");
                }
                _ => {}
            }
        }

        // Drain events from the pipeline — mark dirty on any state change
        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(ev) => {
                state.apply_event(ev);
                engine.mark_dirty_reason("pipeline-event");
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
    state.refresh_fleet();
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
    let _ = execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        ratatui::crossterm::event::EnableBracketedPaste
    );
    // Progressive adoption of the Kitty keyboard protocol (I4) — see run_tui.
    if crate::display::kitty::kitty_capable() {
        let _ = crate::display::kitty::enable_kitty_keyboard();
    }
    // TUI-030: motion tracking (see run_tui).
    let _ = crate::display::mouse::enable_tracking();
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
            state.refresh_fleet_if_stale(std::time::Duration::from_millis(500));
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
                // One overlay ladder for both loops. It used to be two
                // hand-ordered chains that could disagree — and did: `run_tui`
                // checked the help overlay before onboarding, `run_chat` after.
                match route_overlay_key(&mut state, &mut command_palette, key, &project_path) {
                    OverlayOutcome::Consumed => {
                        needs_render = true;
                        continue;
                    }
                    OverlayOutcome::Quit => break,
                    OverlayOutcome::Free => {}
                }

                if key.code == KeyCode::Tab {
                    // `niki chat` runs the second of two event loops in this
                    // file, and this handler was a duplicate of the one in the
                    // first loop — carrying the same defect, so the fix applied
                    // there did not reach this path. Both `current_page`
                    // (what is rendered) and `view` (what the footer label
                    // reads) have to move together or the footer claims a
                    // toggle that did not happen.
                    let next = match state.current_page {
                        PageId::Chat => PageId::Run,
                        _ => PageId::Chat,
                    };
                    state.current_page = next;
                    state.view = match next {
                        PageId::Chat => crate::display::state::ViewMode::Chat,
                        other => crate::display::state::ViewMode::Page(other),
                    };
                    needs_render = true;
                    continue;
                }

                // Page navigation, before the chat composer. `niki chat` runs
                // this second event loop, so the handler added to the first
                // loop did not apply here — arrows and digits were bound and
                // advertised in the footer but did nothing. Page-scoped keys
                // still win: this sits after the router and the Fleet/Session
                // handlers below, and is skipped entirely in the composer.
                if state.current_page != PageId::Chat
                    && let Some(intent) = crate::display::nav::intent_from_key(
                        &key,
                        crate::display::nav::text_focus_active(&state),
                    )
                {
                    use crate::display::nav::{self as nav, Dir, NavIntent};
                    let before = state.current_page;
                    match intent {
                        NavIntent::Page(Dir::Next) => state.current_page = nav::next_page(before),
                        NavIntent::Page(Dir::Prev) => state.current_page = nav::prev_page(before),
                        NavIntent::Select(dir) => {
                            let len = nav::page_item_count(&state);
                            state.page_selection = nav::step_index(len, state.page_selection, dir);
                        }
                        NavIntent::GotoPage(n) => {
                            if let Some(page) = nav::goto_page(n) {
                                state.current_page = page;
                                state.view = crate::display::state::ViewMode::Page(page);
                            }
                        }
                        NavIntent::Quit => break,
                    }
                    state.view = match state.current_page {
                        PageId::Chat => crate::display::state::ViewMode::Chat,
                        other => crate::display::state::ViewMode::Page(other),
                    };
                    if state.current_page != before || matches!(intent, NavIntent::Select(_)) {
                        needs_render = true;
                    }
                    continue;
                }

                if state.current_page == PageId::Chat {
                    let before_len = state.chat_log.len();
                    let mut chat_page = chat::ChatPage::new();
                    chat_page.handle_key(key, &mut state);
                    needs_render = true;
                    // Forward any newly submitted user message to the session
                    // processor (Phase 6 — user messages mid-session).
                    if state.chat_log.len() > before_len {
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

                // Fleet/Session own their navigation (tabs, selection); other
                // keys fall back to global page jumps, same as run_tui.
                if state.current_page == PageId::Fleet {
                    if handle_fleet_nav(key, &mut state) {
                        needs_render = true;
                    } else if let Some(page) = global_page_jump(key) {
                        state.current_page = page;
                        needs_render = true;
                    }
                    continue;
                }
                if state.current_page == PageId::Session {
                    if handle_session_nav(key, &mut state) {
                        needs_render = true;
                    } else if let Some(page) = global_page_jump(key) {
                        state.current_page = page;
                        needs_render = true;
                    }
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
                        } else if let Some(page) = global_page_jump(key) {
                            // Page bindings win; bare letters fall back to
                            // global jumps so every page is reachable.
                            state.current_page = page;
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
            } else if let Ok(Event::Paste(pasted)) = event::read() {
                state.input_state.insert_str(&pasted);
                state.input_state.start_paste_burst();
                needs_render = true;
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
    let _ = crate::display::mouse::disable_tracking();
    let _ = execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        ratatui::crossterm::event::DisableBracketedPaste
    );
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
    let reduced_motion =
        state.config.ui.reduced_motion || std::env::var_os("NIKI_REDUCED_MOTION").is_some();
    let bar = crate::display::components::render_progress_bar_shimmer(
        progress,
        (area.width as usize).saturating_sub(24),
        state.tick,
        reduced_motion,
    );
    let spinner = crate::display::components::SpinnerState::with_tick(if reduced_motion {
        0
    } else {
        state.tick
    });
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

    // Main layout: adaptive header + page content + status line
    let header_height = super::logo::preferred_logo_height(size.width, size.height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height), // adaptive logo / single-line header
            Constraint::Min(5),                // page content
            Constraint::Length(1),             // status line (footer meta)
        ])
        .split(size);

    // Render adaptive header in the top area if allocated
    if header_height > 0 {
        super::logo::render_adaptive_header(frame, chunks[0], state);
    }

    // Render the current page in the content area
    match state.current_page {
        PageId::Fleet => {
            crate::display::pages::fleet::render_fleet(&state.fleet, chunks[1], frame.buffer_mut());
        }
        PageId::Session => {
            if let Some(ref sv) = state.session_view {
                crate::display::pages::session::render_session(sv, chunks[1], frame.buffer_mut());
            } else {
                // No session open: explicit empty state, never a blank screen.
                use ratatui::widgets::Paragraph;
                frame.render_widget(
                    Paragraph::new(vec![
                        ratatui::text::Line::from(" session"),
                        ratatui::text::Line::from(""),
                        ratatui::text::Line::from(ratatui::text::Span::styled(
                            "  No session open — run a task or pick a mission from Fleet (g).",
                            ratatui::style::Style::default().fg(crate::display::theme::fg_dim()),
                        )),
                    ]),
                    chunks[1],
                );
            }
        }
        PageId::Chat => {
            crate::display::layout::render_chat(frame, chunks[1], state);
        }
        _ => router.render_current(frame, chunks[1], state),
    }

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

    // Render which-key style help overlay if present
    if state.show_help {
        super::help_overlay::render_help_overlay(
            frame,
            size,
            &state.keybindings,
            &state.keybinding_overrides,
            state.keybinding_conflicts.len(),
        );
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

    // Render tool detail modal if a card is open (TUI-010: previously tracked
    // state but never painted). Renders above chat, below help overlays.
    if let Some(idx) = state.tool_detail_index {
        if let Some(card) = state.tool_cards.get(idx) {
            let content = super::components::tool_detail::detail_content_lines(card);
            let viewport = super::components::tool_detail::detail_viewport(size);
            let offset = state.tool_detail_scroll.view_offset(content, viewport);
            super::components::tool_detail::render_tool_detail(frame, card, size, offset);
        }
    }

    // Render a spinner/progress indicator while stages are running
    if state.has_running_stage() {
        render_activity_spinner(frame, size, state);
    }
}

/// Global page jump for a key event: plain (unmodified) letters map via
/// [`PageId::from_key`], so every page is reachable from anywhere. Modified
/// keys (Ctrl/Alt/Shift) never jump — they belong to input and shortcuts.
/// Page-specific handlers run first; this is the fallback.
fn global_page_jump(key: KeyEvent) -> Option<PageId> {
    if key.modifiers.is_empty()
        && let KeyCode::Char(c) = key.code
    {
        return PageId::from_key(c);
    }
    None
}

/// Key navigation for the Fleet grid (`g` page).
fn handle_fleet_nav(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => state.fleet.select_prev(),
        KeyCode::Down | KeyCode::Char('j') => state.fleet.select_next(),
        KeyCode::Left => state.fleet.select_left(2),
        KeyCode::Right => state.fleet.select_right(2),
        KeyCode::Char('s') => {
            // Open the selected mission's Session view directly from Fleet.
            state.open_selected_mission();
        }
        KeyCode::Enter => state.open_selected_mission(),
        KeyCode::Esc => state.current_page = PageId::Chat,
        _ => return false,
    }
    true
}

/// Key navigation for the Session view (`s` page).
fn handle_session_nav(key: KeyEvent, state: &mut AppState) -> bool {
    match key.code {
        KeyCode::Tab => {
            if let Some(ref mut sv) = state.session_view {
                sv.next_tab();
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut sv) = state.session_view {
                sv.prev_tab();
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut sv) = state.session_view {
                sv.next_tab();
            }
        }
        KeyCode::Esc => state.close_session_to_fleet(),
        _ => return false,
    }
    true
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
    fn global_page_jump_only_plain_letters() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let plain = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty());
        assert_eq!(global_page_jump(plain('p')), Some(PageId::Pipeline));
        assert_eq!(global_page_jump(plain('l')), Some(PageId::TestLog));
        assert_eq!(global_page_jump(plain('x')), None);
        // Modified keys never navigate (input and shortcuts own them).
        assert_eq!(
            global_page_jump(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            None
        );
        assert_eq!(
            global_page_jump(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())),
            None
        );
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
    /// Mouse routing was untestable: it lived as a 378-line `match` arm inside
    /// `run_tui`, with no seam to drive an event through. That is not a
    /// theoretical gap — it is why a click handler for a tab bar that is never
    /// drawn could sit there passing review.
    ///
    /// These are the cases that could not be written before.
    mod mouse {
        use super::*;
        use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        fn state() -> AppState {
            let config = crate::config::NikiConfig::default();
            AppState::new("test".to_string(), config, ".".into())
        }

        fn ev(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
            MouseEvent {
                kind,
                column: col,
                row,
                modifiers: KeyModifiers::NONE,
            }
        }

        fn full() -> ratatui::layout::Rect {
            ratatui::layout::Rect::new(0, 0, 100, 30)
        }

        fn route(
            st: &mut AppState,
            palette: &mut CommandPalette,
            router: &mut crate::display::pages::PageRouter,
            e: MouseEvent,
        ) -> bool {
            route_mouse(st, router, palette, e, Some(full()))
        }

        /// The phantom tab bar: clicking the top row must not change the page,
        /// because nothing is drawn there.
        #[test]
        fn clicking_the_top_row_does_not_navigate() {
            let mut st = state();
            let mut palette = CommandPalette::new();
            let mut router = crate::display::pages::PageRouter::new();
            st.current_page = PageId::Chat;
            for col in [2u16, 20, 40, 60, 90] {
                let kind = MouseEventKind::Down(MouseButton::Left);
                route(&mut st, &mut palette, &mut router, ev(kind, col, 0));
            }
            assert_eq!(
                st.current_page,
                PageId::Chat,
                "nothing is drawn on the top row, so a click there must do nothing"
            );
        }

        /// The help overlay owns the mouse while it is up, whatever the click.
        #[test]
        fn a_click_anywhere_dismisses_help() {
            let mut st = state();
            let mut palette = CommandPalette::new();
            let mut router = crate::display::pages::PageRouter::new();
            st.show_help = true;
            let kind = MouseEventKind::Down(MouseButton::Left);
            assert!(route(&mut st, &mut palette, &mut router, ev(kind, 50, 20)));
            assert!(!st.show_help, "help must close on any click");
        }

        /// A help click must not also fall through to whatever is underneath.
        #[test]
        fn a_help_click_does_not_also_reach_the_page_below() {
            let mut st = state();
            let mut palette = CommandPalette::new();
            let mut router = crate::display::pages::PageRouter::new();
            st.show_help = true;
            st.current_page = PageId::Chat;
            let before = st.current_page;
            let kind = MouseEventKind::Down(MouseButton::Left);
            route(&mut st, &mut palette, &mut router, ev(kind, 10, 3));
            assert_eq!(st.current_page, before);
        }

        /// An overlay that owns the mouse consumes it: a click inside an open
        /// tool-detail modal must not also reach the chat page behind it.
        #[test]
        fn an_open_overlay_consumes_the_click() {
            let mut st = state();
            let mut palette = CommandPalette::new();
            let mut router = crate::display::pages::PageRouter::new();
            st.current_page = PageId::Chat;
            st.tool_detail_index = Some(0);
            st.tool_cards
                .push(crate::display::components::tool_card::ToolCard::new(
                    "bash",
                    "cargo test",
                ));
            let before = st.current_page;
            let kind = MouseEventKind::Down(MouseButton::Left);
            // Inside the modal: the click is consumed, nothing below is touched.
            route(&mut st, &mut palette, &mut router, ev(kind, 50, 2));
            assert_eq!(st.current_page, before);
        }

        /// The last row is the status bar, and clicking the mode badge cycles
        /// the permission mode — the one mouse affordance that is drawn.
        #[test]
        fn clicking_the_permission_badge_cycles_the_mode() {
            let mut st = state();
            let mut palette = CommandPalette::new();
            let mut router = crate::display::pages::PageRouter::new();
            st.current_page = PageId::Chat;
            let before = st.permission_mode;
            let kind = MouseEventKind::Down(MouseButton::Left);
            route(&mut st, &mut palette, &mut router, ev(kind, 97, 29));
            assert_ne!(
                st.permission_mode, before,
                "the badge is drawn on the last row, so a click there must act"
            );
        }
    }
    /// The overlay ladder is the top of key dispatch, and until this turn it
    /// existed as two hand-ordered chains — one in each loop — that had
    /// already drifted: `run_tui` checked the help overlay before onboarding,
    /// `run_chat` after it.
    mod overlay_ladder {
        use super::*;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        fn state() -> AppState {
            let config = crate::config::NikiConfig::default();
            AppState::new("test".to_string(), config, ".".into())
        }

        fn ladder(st: &mut AppState, k: KeyCode) -> OverlayOutcome {
            let mut palette = CommandPalette::new();
            route_overlay_key(
                st,
                &mut palette,
                KeyEvent::new(k, KeyModifiers::NONE),
                std::path::Path::new("."),
            )
        }

        /// Nothing open: the page gets the key. Every other case below is a
        /// deviation from this, so it is the baseline that makes the rest
        /// meaningful.
        #[test]
        fn with_nothing_open_the_key_reaches_the_page() {
            let mut st = state();
            assert_eq!(ladder(&mut st, KeyCode::Char('a')), OverlayOutcome::Free);
        }

        #[test]
        fn a_permission_prompt_outranks_the_help_overlay() {
            // A security question hidden behind a help overlay is a question
            // the user cannot answer. Whichever overlay is on top, the prompt
            // has to be reachable — and it is the one that can be forgotten.
            let mut st = state();
            st.show_help = true;
            let (tx, _rx) = std::sync::mpsc::channel();
            st.permission_request = Some(crate::display::state::PermissionRequest {
                tool_name: "sandbox_exec".into(),
                command: "rm -rf /".into(),
                description: String::new(),
                params: None,
                response_tx: tx,
            });
            st.show_permission_modal = true;
            assert_eq!(
                ladder(&mut st, KeyCode::Char('n')),
                OverlayOutcome::Consumed,
                "the prompt must take the key, not the overlay behind it"
            );
            assert!(!st.show_help || !st.show_permission_modal);
        }

        #[test]
        fn onboarding_outranks_everything() {
            let mut st = state();
            st.onboarding = Some(crate::display::onboarding::OnboardingModal::new());
            st.show_help = true;
            assert_eq!(
                ladder(&mut st, KeyCode::Char('a')),
                OverlayOutcome::Consumed
            );
            assert!(
                st.onboarding.is_some(),
                "onboarding owns the key; nothing below it may act"
            );
        }

        #[test]
        fn help_swallows_everything_but_its_own_toggle() {
            let mut st = state();
            st.show_help = true;
            // A letter is swallowed and does not reach the page.
            assert_eq!(
                ladder(&mut st, KeyCode::Char('a')),
                OverlayOutcome::Consumed
            );
            assert!(st.show_help, "help stays up");
            // Esc is the way out.
            assert_eq!(ladder(&mut st, KeyCode::Esc), OverlayOutcome::Consumed);
            assert!(!st.show_help, "esc closes it");
        }

        /// A rebound key must work, not just the built-in one.
        ///
        /// The ladder used to test a literal Ctrl+P, copied from the chat loop,
        /// while the other two globals resolved through the table. A user who
        /// rebound `command_palette` in `niki.toml` got a binding that worked
        /// in `niki` and did nothing in `niki chat` — the same one-loop-two-
        /// behaviours defect, in a third form.
        #[test]
        fn a_rebound_palette_key_opens_the_palette() {
            use std::collections::HashMap;
            let mut overrides: HashMap<String, Vec<String>> = HashMap::new();
            overrides.insert("command_palette".to_string(), vec!["ctrl+g".to_string()]);
            let (kb, _conflicts) =
                crate::display::keybindings::KeyBindings::with_overrides(&overrides);

            let mut st = state();
            st.keybindings = kb;
            let mut palette = CommandPalette::new();
            let g = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);

            // The rebound key opens it.
            assert_eq!(
                route_overlay_key(&mut st, &mut palette, g, std::path::Path::new(".")),
                OverlayOutcome::Consumed
            );
            assert!(
                st.show_command_palette,
                "the configured key must open the palette"
            );

            // And the built-in key no longer does — it was rebound away.
            st.show_command_palette = false;
            let p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
            assert_eq!(
                route_overlay_key(&mut st, &mut palette, p, std::path::Path::new(".")),
                OverlayOutcome::Free,
                "a rebound-away key must not still open it"
            );
        }

        #[test]
        fn the_palette_opens_on_ctrl_p_and_swallows_the_next_key() {
            let mut st = state();
            let mut palette = CommandPalette::new();
            route_overlay_key(
                &mut st,
                &mut palette,
                KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                std::path::Path::new("."),
            );
            assert!(st.show_command_palette);
            // While it is open, a bare key is the palette's, not the page's.
            let out = route_overlay_key(
                &mut st,
                &mut palette,
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                std::path::Path::new("."),
            );
            assert_eq!(out, OverlayOutcome::Consumed);
            assert_eq!(st.command_selected, palette.cursor.selected);
        }

        #[test]
        fn a_confirm_modal_asks_to_leave() {
            let mut st = state();
            st.modal = Some(crate::display::state::Modal::Confirm {
                title: "Quit".into(),
                message: "Exit NIKI?".into(),
            });
            assert_eq!(ladder(&mut st, KeyCode::Enter), OverlayOutcome::Quit);
        }

        #[test]
        fn a_dismissed_modal_keeps_the_run_alive() {
            let mut st = state();
            st.modal = Some(crate::display::state::Modal::Confirm {
                title: "Quit".into(),
                message: "Exit NIKI?".into(),
            });
            assert_eq!(ladder(&mut st, KeyCode::Esc), OverlayOutcome::Consumed);
            assert!(st.modal.is_none(), "esc dismisses without leaving");
        }
    }
}
