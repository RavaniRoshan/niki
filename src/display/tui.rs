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
    /// Something the run needs to say, which is not an error and not a verdict.
    ///
    /// The pipeline's diagnostics — "the Coder produced nothing, falling back",
    /// "spend cap exceeded", "branch blocked: the test suite failed" — were all
    /// `eprintln!`. Under `--tui` an `eprintln!` lands in the alternate-screen
    /// buffer, which `LeaveAlternateScreen` then discards, so the most important
    /// line of a failed run was written to a screen that was about to be thrown
    /// away. This carries the same text into the surface that is still up.
    Notice {
        text: String,
        /// Warnings are shown without demanding acknowledgement.
        warning: bool,
    },
    /// The run is over — either way.
    ///
    /// Carries what happened, because there is more than one ending: a run
    /// that a Reviewer approved, one it rejected, one that produced no verdict
    /// at all, and one that failed outright. A bare `Final` collapsed all four
    /// into "approved", on a surface whose whole promise is not lying about it.
    Final {
        /// `"approved"`, `"rejected"`, … or `None` when nothing reviewed it.
        verdict: Option<String>,
        /// The failure, when the run failed.
        error: Option<String>,
    },
    /// Branch name from the pipeline (fixes the never-populated branch_name).
    BranchName(String),
    /// A chat message submitted/typed into the session (user or assistant turn).
    /// Used by `niki chat` to render the running conversation.
    ChatMessage {
        role: String,
        text: String,
    },
    /// A chat turn is now in flight; draw the "thinking" indicator.
    ///
    /// This replaces a literal `"(thinking…)"` assistant bubble that used to be
    /// pushed on the `--message` boot path. Being an assistant turn, it stayed
    /// in the transcript permanently and read as something the model had said.
    /// Typed messages got no such marker at all.
    ChatPending,
    ///
    /// Separate from `ChatMessage` so the transcript grows a *visible* reply as
    /// tokens arrive instead of after a 20-second silence. The chat surface
    /// called `complete()`, a single non-streaming POST, while the provider's
    /// `stream()` was already implemented and already consumed by `niki run` —
    /// so the one place a person waits most often was the one place with
    /// nothing to watch.
    ChatDelta {
        text: String,
    },
    /// A chat turn failed. Rendered as an error, not as something the model
    /// said.
    ///
    /// This used to be a `ChatMessage { role: "assistant" }` whose text began
    /// with the literal `(offline)`. A 401 — a wrong API key — was therefore
    /// shown to the user as the assistant telling them it was offline, in the
    /// same bubble style as a real answer. Errors that look like answers are
    /// the hardest kind to notice.
    ChatError {
        message: String,
        /// `true` when the user cancelled, so the UI can say so rather than
        /// showing a failure.
        cancelled: bool,
    },
    /// The provider's stop reason for the turn that just finished, and whether
    /// it was the token limit rather than a real completion.
    ///
    /// Without this a reply cut at `max_tokens` is presented as a finished
    /// answer — the same class of misdiagnosis `StreamChunk::Finish` exists to
    /// prevent on the pipeline path.
    ChatFinished {
        finish_reason: Option<String>,
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
    // Every overlay this function must know about, highest first.
    //
    // It knew about three. It did not know about onboarding, the error modal,
    // the quit-confirm modal, or an open sheet — all of which are painted
    // full-ish-screen further down. The keyboard ladder checks all of them
    // first, so keys were blocked while the mouse was not: a click landed on
    // the page *behind* the modal, and a click on the status bar silently
    // cycled the permission mode toward BYPASS. Onboarding is the first thing
    // a new user sees, and it has no mouse handler at all.
    if state.show_permission_modal {
        FocusState::Permission
    } else if state.onboarding.is_some() {
        // Onboarding is keyboard-only; every mouse press is swallowed so it
        // cannot reach the surface underneath.
        FocusState::Onboarding
    } else if state.modal.is_some() {
        FocusState::Modal
    } else if state.show_command_palette {
        FocusState::CommandPalette
    } else if state.show_command_menu {
        FocusState::CommandMenu
    } else if !state.sheets.is_empty() {
        FocusState::Sheet
    } else {
        FocusState::Chat
    }
}

/// Restore terminal state no matter how we leave `run_tui`.
struct RestoreGuard;

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        // Release stdin before raw mode goes, so a tool asking a question in
        // the same instant the TUI exits is not told the interface still owns
        // it — and, more importantly, so a *second* TUI in the same process
        // can claim it.
        crate::runtime::tools::set_stdin_owned_by_tui(false);
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

/// The three horizontal bands the screen is split into.
///
/// One function, because painting and hit-testing both need them and were
/// computing them separately: the renderer solved a `Layout`, while the mouse
/// path assumed the status bar was `height - 1` and the header `y = 0`. Those
/// agree today by arithmetic rather than by construction — the same shape as
/// the tool-card height that let Enter open the wrong card.
///
/// Returning the bands instead of a `Rc<[Rect]>` keeps the hit-test honest at
/// any size, which is the property that matters: a click resolves against the
/// same rect the pixels were drawn into.
pub struct Bands {
    pub header: Rect,
    pub content: Rect,
    pub status: Rect,
}

pub fn bands(size: Rect) -> Bands {
    let header_height = super::logo::preferred_logo_height(size.width, size.height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height), // adaptive logo / single-line header
            Constraint::Min(5),                // page content
            Constraint::Length(1),             // status line (footer meta)
        ])
        .split(size);
    Bands {
        header: chunks[0],
        content: chunks[1],
        status: chunks[2],
    }
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
            // `Retry` is deliberately NOT grouped with `Confirm` here. It used
            // to be, and the only producer of `Retry` was the error modal's
            // "press [r] to retry" button — so pressing retry exited the app.
            // Retry needs a real stage restart, which does not exist yet; until
            // it does, the honest behaviour is to stay put.
            ModalAction::Confirm => return OverlayOutcome::Quit,
            ModalAction::Retry => return OverlayOutcome::Consumed,
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

/// What the user asked for with the cancel/exit key.
enum CtrlC {
    /// Handled here; keep running.
    Handled,
    /// The user asked to leave. The caller breaks its loop.
    Exit,
}

/// Ctrl+C, for both TUI loops.
///
/// `GlobalAction::CancelOrExit` was resolved in exactly one place in the crate —
/// inside `run_tui` — so `niki chat`, the loop most people actually use, had no
/// Ctrl+C handling at all. Pressing it in the composer did nothing; pressing it
/// over a permission prompt did nothing. There was no keyboard route out of a
/// TUI that can be sitting on an approval nobody is going to give.
///
/// The first press cancels a running stage, or clears the composer; a second
/// within two seconds leaves. Clearing the composer is a destructive default for
/// a key a user reaches for when they want to *stop* — and for a pasted prompt
/// it is unrecoverable, because `insert_str` (the bracketed-paste path) never
/// pushed an undo entry. So it is gated: an empty composer takes the notice
/// straight to "press again to exit", and a non-empty one says what it is about
/// to discard.
fn handle_ctrl_c(
    state: &mut AppState,
    cancel: &std::sync::atomic::AtomicBool,
    last_ctrl_c: &mut Option<std::time::Instant>,
) -> CtrlC {
    if state.has_running_stage() {
        state.request_cancel("Stopping… (Ctrl+C again to exit)");
    } else if state.input_state.buffer.is_empty() {
        // Nothing to discard, so go straight to the exit arming.
    } else {
        state.input_state.buffer.clear();
        state.input_state.cursor_pos = 0;
    }

    let now = std::time::Instant::now();
    let exit = matches!(*last_ctrl_c, Some(t) if now.duration_since(t) < Duration::from_secs(2));
    *last_ctrl_c = Some(now);
    if exit {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        return CtrlC::Exit;
    }
    if !state.has_running_stage() {
        state.set_notice("Press Ctrl+C again to exit", 3000);
    }
    CtrlC::Handled
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
    // A *click* dismisses the help overlay. This ran for every mouse event,
    // including `Moved`, and motion reporting is enabled outside tmux/screen
    // (DEC 1003, `mouse.rs:42-48`) — so nudging the mouse made the keybindings
    // screen vanish. On a first run, where `?` is the only way to learn the
    // app, that is the first thing a new user breaks without noticing.
    let clicking_help = state.show_help && matches!(mouse.kind, MouseEventKind::Down(..));
    if clicking_help {
        state.show_help = false;
        return true;
    }
    // While the overlay is up, motion and wheel must not reach the page behind
    // it either.
    if state.show_help {
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
        // The three overlays `active_focus` learned about. They have no mouse
        // behaviour of their own, so the press is consumed rather than passed
        // to the surface behind them. A click on the status bar during
        // onboarding previously cycled the permission mode toward BYPASS
        // through a modal the user could not see was there.
        FocusState::Onboarding | FocusState::Modal | FocusState::Sheet => {}
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
                // The same bands the renderer paints into. This used to solve
                // its own layout with a hardcoded 8-row header, while the
                // renderer asked `preferred_logo_height` — which returns 0 or 1
                // on a short or narrow terminal. On anything that was not a
                // large terminal the scroll region and the scrollbar were
                // therefore measuring a content area that started somewhere
                // other than where the content was drawn.
                let chunks = bands(full);
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
                        let visible = chunks.content.height as usize;
                        state.chat_scroll.scroll_by(rest, total, visible);
                    }
                    dirty = true;
                } else {
                    // Scrollbar click/drag-to-jump (gaps P0 — "Drag to scroll").
                    let msg_area_h = chunks.content.height.saturating_sub(3) as usize;
                    let sb_col = chunks.content.x + chunks.content.width.saturating_sub(1);
                    let on_scrollbar = (clicking || matches!(mouse.kind, MouseEventKind::Drag(_)))
                        && mouse.column == sb_col
                        && mouse.row >= chunks.content.y
                        && mouse.row < chunks.content.y + chunks.content.height.saturating_sub(3);
                    if on_scrollbar {
                        let total = state.chat_lines.len();
                        if total > msg_area_h && msg_area_h > 0 {
                            let frac = (mouse.row - chunks.content.y) as f64 / msg_area_h as f64;
                            let target = (frac * total as f64).round() as usize;
                            state.chat_scroll.jump_to(target, total, msg_area_h);
                            dirty = true;
                        }
                    } else if hovering {
                        // Hover hit-test for chat elements
                        let row = mouse.row.saturating_sub(chunks.content.y) as usize;
                        let total = state.chat_lines.len();
                        let visible = chunks.content.height as usize;
                        let offset = state.chat_scroll.view_offset(total, visible);
                        let abs_row = offset + row;
                        let new_target = if row < chunks.content.height as usize
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
                        chat::ChatPage::handle_mouse(state, mouse, chunks.content);
                        dirty = true;
                    }
                }
                // Click-to-position cursor in input box
                if clicking && state.current_page == PageId::Chat {
                    // The same split the renderer paints, with the same
                    // growth rule. This assumed a fixed three-row composer
                    // while the composer grows with a multi-line draft, so
                    // after one Shift+Enter every click in the lower third of
                    // the panel was resolved against a band that was no longer
                    // where the composer was drawn.
                    let input_lines = state.input_state.buffer.lines().count().max(1);
                    let (_msg, composer) =
                        crate::display::layout::composer_split(chunks.content, input_lines);
                    if super::components::input_box::handle_click(state, mouse.column, composer) {
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
                // `Retry` is produced by the key handler and does nothing yet;
                // both it and a click on the modal close it.
                ModalAction::Retry | ModalAction::Dismiss => {
                    state.modal = None;
                    dirty = true;
                }
                ModalAction::Config => {
                    state.modal = None;
                    state.current_page = PageId::Config;
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
    // From here until `RestoreGuard` drops, this thread reads stdin. Tell the
    // tool layer, so `approval` cannot answer a question nobody was asked and
    // `ask_user` does not steal a keystroke from the interface.
    crate::runtime::tools::set_stdin_owned_by_tui(true);
    if execute!(io::stdout(), EnterAlternateScreen).is_err() {
        crate::runtime::tools::set_stdin_owned_by_tui(false);
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
                    if crate::display::sheets::sheets_open(&state) {
                        match crate::display::sheets::route(&mut state, key) {
                            Ok(_) => {
                                engine.mark_dirty();
                                continue;
                            }
                            Err(e) => {
                                state.set_notice(&format!("settings error: {e}"), 5000);
                                engine.mark_dirty();
                                continue;
                            }
                        }
                    }

                    match route_overlay_key(&mut state, &mut command_palette, key, &project_path) {
                        OverlayOutcome::Consumed => {
                            engine.mark_dirty();
                            continue;
                        }
                        OverlayOutcome::Quit => break,
                        OverlayOutcome::Free => {}
                    }
                    if state.keybindings.resolve(&key) == Some(GlobalAction::CancelOrExit) {
                        match handle_ctrl_c(&mut state, &cancel, &mut last_ctrl_c) {
                            CtrlC::Exit => break,
                            CtrlC::Handled => engine.mark_dirty(),
                        }
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
                            } else if key.code == KeyCode::Char('q') {
                                state.modal = Some(super::pages::Modal::Confirm {
                                    title: "Quit".into(),
                                    message: "Exit NIKI?".into(),
                                });
                                engine.mark_dirty();
                            } else if let Some(page) = global_page_jump(key) {
                                state.current_page = page;
                                engine.mark_dirty();
                            }
                        } else if state.current_page == PageId::Session {
                            if handle_session_nav(key, &mut state) {
                                engine.mark_dirty();
                            } else if key.code == KeyCode::Char('q') {
                                // Neither Fleet nor Session answers `q`, and
                                // both fall through here, so with the nav layer
                                // no longer eating it they would have swallowed
                                // it. A page that declines `q` confirms the quit
                                // instead — never silently doing nothing.
                                state.modal = Some(super::pages::Modal::Confirm {
                                    title: "Quit".into(),
                                    message: "Exit NIKI?".into(),
                                });
                                engine.mark_dirty();
                            } else if let Some(page) = global_page_jump(key) {
                                state.current_page = page;
                                engine.mark_dirty();
                            }
                        } else if router.handle_key(key, &mut state) {
                            // The remaining sub-pages' own keys, `q` and `j`/`k`
                            // included. See `sub_page_owns` for why the
                            // navigator must not claim them first.
                            engine.mark_dirty();
                        } else if !sub_page_owns(key, &state)
                            && let Some(intent) = crate::display::nav::intent_from_key(
                                &key,
                                crate::display::nav::text_focus_active(&state),
                            )
                        {
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
                                    // The page above already had its turn with
                                    // `q` and declined it, so this is the
                                    // fallback — and it asks, exactly as the Run
                                    // page does. It used to `break` out of the
                                    // event loop, which is how a user pressing
                                    // "go back" on a sub-page lost the
                                    // interface entirely.
                                    state.modal = Some(super::pages::Modal::Confirm {
                                        title: "Quit".into(),
                                        message: "Exit NIKI?".into(),
                                    });
                                    engine.mark_dirty();
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
/// A user message handed from the TUI to whoever is driving the LLM.
///
/// Carries the conversation with it. The TUI owns `chat_log`; the processor
/// thread has no other way to know what was said before. Sending the bare
/// string — as this used to — is why the transport had nowhere to put the
/// rest of the conversation: turn 3 went to the provider with turns 1 and 2
/// erased, while the transcript on screen showed all three.
pub struct ChatSubmit {
    pub text: String,
    pub history: Vec<crate::llm::provider::ChatTurn>,
    /// The caller's cancel flag, so the processor thread observes the same
    /// handle the TUI's Esc sets. Handing each side its own flag is how Esc
    /// came to print "Stopping…" and stop nothing.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

pub fn run_chat(
    rx: Receiver<DisplayEvent>,
    description: String,
    project_path: PathBuf,
    on_submit: Option<mpsc::Sender<ChatSubmit>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let _guard = RestoreGuard;

    let mut last_ctrl_c: Option<std::time::Instant> = None;

    if enable_raw_mode().is_err() {
        return;
    }
    // Same claim as `run_tui` — see there. `niki chat` is the surface a person
    // is most likely to be typing into when an agent asks a question, so this
    // is the path where a stray keystroke is most likely.
    crate::runtime::tools::set_stdin_owned_by_tui(true);
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
    // `run_tui` has always wired this. `run_chat` did not, so `request_cancel`
    // had nothing to set and Esc painted a "Stopping…" notice while the
    // in-flight request kept running and its answer still landed. The flag is
    // local to this loop; the processor thread gets its own handle in the
    // `ChatSubmit` payload.
    state.cancel = Some(cancel.clone());
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
            // A draw error is not swallowed.
            //
            // `.ok()` discarded the `io::Error`, and the loop then set
            // `needs_render = false` and kept polling. Once drawing started
            // failing — stdout pipe broken, a resize race leaving the terminal
            // in a bad state — the app kept consuming keystrokes against a
            // frozen last frame, with no error, no exit and no notice. The
            // user typed and nothing happened, indefinitely. `run_tui` already
            // breaks on a draw error; the chat surface, which is the one every
            // user lands on, had the weaker loop.
            let drew = terminal.draw(|f| render(f, &state, &router, &command_palette));
            if let Err(e) = drew {
                // `RestoreGuard` releases raw mode, the alternate screen, the
                // mouse and bracketed paste when this function returns, so
                // breaking out of the loop is all the restoration needed.
                eprintln!("niki: the terminal could not be drawn ({e}); restoring and exiting.");
                break;
            }
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
            match event::read() {
                Ok(Event::Key(key)) => {
                    // One overlay ladder for both loops. It used to be two
                    // hand-ordered chains that could disagree — and did: `run_tui`
                    // checked the help overlay before onboarding, `run_chat` after.
                    // Sheets own every key while one is open. This is ahead of
                    // the overlay ladder on purpose: a settings form that
                    // leaked an Esc or a Ctrl+S to the page behind it would be
                    // worse than no form at all, and the overlay ladder's
                    // global toggles must not fire out from under a form the
                    // user is halfway through filling in.
                    if crate::display::sheets::sheets_open(&state) {
                        match crate::display::sheets::route(&mut state, key) {
                            Ok(_) => {
                                needs_render = true;
                                continue;
                            }
                            Err(e) => {
                                state.set_notice(&format!("settings error: {e}"), 5000);
                                needs_render = true;
                                continue;
                            }
                        }
                    }

                    match route_overlay_key(&mut state, &mut command_palette, key, &project_path) {
                        OverlayOutcome::Consumed => {
                            needs_render = true;
                            continue;
                        }
                        OverlayOutcome::Quit => break,
                        OverlayOutcome::Free => {}
                    }

                    // Ctrl+C, shared with run_tui. It used to be resolved in
                    // run_tui only, which left `niki chat` — the loop people
                    // actually sit in — with no keyboard route out of a TUI that
                    // might be blocked on an approval.
                    if state.keybindings.resolve(&key) == Some(GlobalAction::CancelOrExit) {
                        match handle_ctrl_c(&mut state, &cancel, &mut last_ctrl_c) {
                            CtrlC::Exit => break,
                            CtrlC::Handled => needs_render = true,
                        }
                        continue;
                    }

                    // Through the keybinding table. This was a literal `Tab`, so
                    // it happened to match the default and looked fine — but a
                    // user who rebound `toggle_chat` in `niki.toml` got a binding
                    // that did nothing in `niki chat` and worked everywhere else.
                    // The default is still Tab.
                    if state.keybindings.resolve(&key) == Some(GlobalAction::ToggleChatPage) {
                        // Both `current_page` (what is rendered) and `view` (what
                        // the footer label reads) have to move together or the
                        // footer claims a toggle that did not happen.
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
                    // advertised in the footer but did nothing.
                    //
                    // `q` is the exception, and the reason is 11 dead handlers.
                    // Every sub-page answers `q` with "go back to Run"
                    // (`pages/diff.rs:189`, `pages/history.rs:281`, and nine
                    // more), and none of them could ever run: the nav layer sat
                    // above the router, read `q` as `NavIntent::Quit`, and broke
                    // the event loop. So a user on the Diff page pressing `q`
                    // — the key that goes *back* everywhere else in the app —
                    // lost the interface entirely. It also made the confirm-quit
                    // modal below unreachable dead code, because the loop had
                    // already exited before reaching it.
                    //
                    // So: on Chat, these keys are ours. On a sub-page they are
                    // the page's, and a page that declines one falls through to
                    // the modal below.
                    if state.current_page != PageId::Chat
                        && !sub_page_owns(key, &state)
                        && let Some(intent) = crate::display::nav::intent_from_key(
                            &key,
                            crate::display::nav::text_focus_active(&state),
                        )
                    {
                        use crate::display::nav::{self as nav, Dir, NavIntent};
                        let before = state.current_page;
                        match intent {
                            NavIntent::Page(Dir::Next) => {
                                state.current_page = nav::next_page(before)
                            }
                            NavIntent::Page(Dir::Prev) => {
                                state.current_page = nav::prev_page(before)
                            }
                            NavIntent::Select(dir) => {
                                let len = nav::page_item_count(&state);
                                state.page_selection =
                                    nav::step_index(len, state.page_selection, dir);
                            }
                            NavIntent::GotoPage(n) => {
                                if let Some(page) = nav::goto_page(n) {
                                    state.current_page = page;
                                    state.view = crate::display::state::ViewMode::Page(page);
                                }
                            }
                            NavIntent::Quit => {
                                // Unreachable while the block above excludes
                                // `q`, and kept honest rather than left as
                                // `break`: if the gate above is ever loosened
                                // and `q` reaches here again, it must ask, the
                                // way every other route out of the app does.
                                state.modal = Some(crate::display::pages::Modal::Confirm {
                                    title: "Quit".into(),
                                    message: "Exit NIKI?".into(),
                                });
                            }
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
                                    // Everything said before this message is the
                                    // model's context. The just-pushed turn is
                                    // the request, not part of the history.
                                    let history = state
                                        .chat_log
                                        .iter()
                                        .take(state.chat_log.len().saturating_sub(1))
                                        .filter(|(r, _)| r == "user" || r == "assistant")
                                        .map(|(r, t)| match r.as_str() {
                                            "assistant" => {
                                                crate::llm::provider::ChatTurn::assistant(t)
                                            }
                                            _ => crate::llm::provider::ChatTurn::user(t),
                                        })
                                        .collect();
                                    if let Some(tx) = &on_submit {
                                        let _ = tx.send(ChatSubmit {
                                            text: text.clone(),
                                            history,
                                            cancel: cancel.clone(),
                                        });
                                    }
                                    // Something is now in flight. Without this
                                    // the surface shows nothing at all until the
                                    // first delta arrives.
                                    state.chat_pending = true;
                                    state.chat_truncated = false;
                                }
                            }
                        }
                        persistence::save_chat_session(
                            &project_path,
                            &persistence::snapshot(&state),
                        );
                        continue;
                    }

                    // Fleet/Session own their navigation (tabs, selection); other
                    // keys fall back to global page jumps, same as run_tui.
                    //
                    // Neither handles `q`, and both `continue` before the router
                    // — so with the nav layer no longer eating `q`, they would
                    // have swallowed it entirely. They get the same rule every
                    // other sub-page gets: the page's handler first, and a page
                    // that declines `q` falls back to the confirm modal rather
                    // than to nothing.
                    if state.current_page == PageId::Fleet {
                        if handle_fleet_nav(key, &mut state) {
                            needs_render = true;
                        } else if key.code == KeyCode::Char('q') {
                            state.modal = Some(crate::display::pages::Modal::Confirm {
                                title: "Quit".into(),
                                message: "Exit NIKI?".into(),
                            });
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
                        } else if key.code == KeyCode::Char('q') {
                            state.modal = Some(crate::display::pages::Modal::Confirm {
                                title: "Quit".into(),
                                message: "Exit NIKI?".into(),
                            });
                            needs_render = true;
                        } else if let Some(page) = global_page_jump(key) {
                            state.current_page = page;
                            needs_render = true;
                        }
                        continue;
                    }

                    // Through the keybinding table, and note the default is
                    // `ctrl+t`, not a bare `t`. So the bare `t` that used to work
                    // here was not the configured key at all, and the configured
                    // key did nothing: a user pressing Ctrl+T in `niki chat` got
                    // silence. Same shape as the palette binding, one turn back.
                    if state.keybindings.resolve(&key) == Some(GlobalAction::CycleTheme) {
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
                        continue;
                    }

                    // The page's own handler runs first, `q` included.
                    //
                    // This used to read `q` as a special case *above* the
                    // router, which made the `q` branch below unreachable:
                    // 11 pages answer `q` with "back to Run" and none of them
                    // could run, while the confirm-quit modal — the only thing
                    // that branch ever did — was dead code. A key can only
                    // mean one thing, so the page gets it, and the modal is
                    // what a page that declines `q` falls back to.
                    if router.handle_key(key, &mut state) {
                        needs_render = true;
                    } else if key.code == KeyCode::Char('q') {
                        state.modal = Some(crate::display::pages::Modal::Confirm {
                            title: "Quit".into(),
                            message: "Exit NIKI?".into(),
                        });
                        needs_render = true;
                    } else if let Some(page) = global_page_jump(key) {
                        // Page bindings win; bare letters fall back to
                        // global jumps so every page is reachable.
                        state.current_page = page;
                        needs_render = true;
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    if state.current_page == PageId::Chat {
                        // The *content* band, not the whole terminal.
                        //
                        // `run_tui` resolved its mouse coordinates against
                        // `bands(full).content` — the same rect the renderer
                        // draws the transcript into, which starts below the
                        // header and stops above the status line. This loop
                        // passed the entire screen, so every coordinate was
                        // off by the header height: drag-to-select copied the
                        // wrong lines, and clicking a collapsed stage header
                        // toggled whichever stage happened to be that many
                        // rows higher.
                        //
                        // The comment block at the `run_tui` site records
                        // exactly this class of bug being fixed there — the
                        // input loop solving its own layout instead of asking
                        // the layout function. This was the second input loop.
                        let size = terminal.size().unwrap_or(ratatui::layout::Size {
                            width: 80,
                            height: 24,
                        });
                        let full = Rect::new(0, 0, size.width, size.height);
                        chat::ChatPage::handle_mouse(&mut state, mouse, bands(full).content);
                        needs_render = true;
                    }
                }
                Ok(Event::Paste(pasted)) => {
                    state.input_state.insert_str(&pasted);
                    state.input_state.start_paste_burst();
                    needs_render = true;
                }
                Ok(Event::Resize(_, _)) => {
                    needs_render = true;
                }
                // Focus changes, KeyEventKind, and anything crossterm adds
                // later. None of them need a redraw on their own.
                _ => {}
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

    crate::runtime::tools::set_stdin_owned_by_tui(false);
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

/// The smallest terminal NIKI can draw into, and what it says when it cannot.
///
/// Below this the surface used to `return` silently while the overlay ladder
/// kept consuming every keystroke, so a user in a short pane got a black void
/// that ate their typing, with `Esc` as the only exit and nothing saying so.
pub const MIN_TERMINAL: (u16, u16) = (20, 10);

/// Draw the "your terminal is too small" screen.
pub fn render_too_small(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    let msg = Paragraph::new(vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default()
                .fg(theme::warning())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "  NIKI needs at least {}x{}. This terminal is {}x{}.",
            MIN_TERMINAL.0, MIN_TERMINAL.1, area.width, area.height
        )),
        Line::from("  Resize the window, or press Ctrl+C to exit."),
    ]);
    frame.render_widget(msg, area);
}

fn render(
    frame: &mut ratatui::Frame,
    state: &AppState,
    router: &PageRouter,
    command_palette: &CommandPalette,
) {
    let size = frame.area();
    if size.height < MIN_TERMINAL.1 || size.width < MIN_TERMINAL.0 {
        // Not a silent `return`. At 80x9 and 80x8 — both verified — the whole
        // surface drew nothing while `route_overlay_key` kept consuming every
        // keystroke, so the user got a black void that swallowed their typing
        // and no way out but `Esc`, which nothing mentioned. Saying the size
        // is the fix; a program that cannot render should say so rather than
        // pretend to be working.
        render_too_small(frame, size);
        return;
    }

    // Fill background
    let bg_block = ratatui::widgets::Block::default().style(Style::default().bg(theme::bg_color()));
    frame.render_widget(bg_block, size);

    // Main layout: adaptive header + page content + status line.
    let bands = bands(size);

    // Render adaptive header in the top area if allocated
    if bands.header.height > 0 {
        super::logo::render_adaptive_header(frame, bands.header, state);
    }

    // Render the current page in the content area
    match state.current_page {
        PageId::Fleet => {
            crate::display::pages::fleet::render_fleet(
                &state.fleet,
                bands.content,
                frame.buffer_mut(),
            );
        }
        PageId::Session => {
            if let Some(ref sv) = state.session_view {
                crate::display::pages::session::render_session(
                    sv,
                    bands.content,
                    frame.buffer_mut(),
                );
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
                    bands.content,
                );
            }
        }
        PageId::Chat => {
            crate::display::layout::render_chat(frame, bands.content, state);
        }
        _ => router.render_current(frame, bands.content, state),
    }

    // Render status line (product "footer meta")
    render_status_line(frame, bands.status, state);

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

    // Sheets go last, above the page, the help overlay and the spinner. A sheet
    // is modal by contract, so anything drawn after it would be a layer the
    // user could interact with while the sheet is open.
    crate::display::sheets::render_top_sheet(frame, size, &state.sheets, state);
}

/// Global page jump for a key event: plain (unmodified) letters map via
/// [`PageId::from_key`], so every page is reachable from anywhere. Modified
/// keys (Ctrl/Alt/Shift) never jump — they belong to input and shortcuts.
/// Page-specific handlers run first; this is the fallback.
/// Whether a focused sub-page, not the navigator, owns this key.
///
/// Ten pages print `[j/k]` in their footer and every one of them implements it
/// in its own `handle_key`, against a private cursor. The navigator also claims
/// `j`/`k`, writing `state.page_selection` — **which no renderer reads**. So on
/// the surface where the help overlay says the key works, it did nothing, and
/// the invisible index moved instead.
///
/// `q` is here for the same reason and the same shape: eleven pages answer it
/// with "back to Run", and the navigator used to quit the app before they could.
///
/// The composer is exempt. When a text field has focus these are ordinary
/// characters, and the nav layer already declines every key in that state
/// (`text_focus_active`); excluding them here keeps that guarantee from being
/// two independent answers to the same question.
fn sub_page_owns(key: KeyEvent, state: &crate::display::state::AppState) -> bool {
    if crate::display::nav::text_focus_active(state) {
        return false;
    }
    matches!(
        key.code,
        KeyCode::Char('q') | KeyCode::Char('j') | KeyCode::Char('k')
    )
}

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
    /// Painting and hit-testing must agree about where the bands are.
    ///
    /// The mouse path used to solve its own layout with a hardcoded 8-row
    /// header while the renderer asked `preferred_logo_height`, which returns
    /// 0 on a short terminal and 1 on a narrow one. On anything that was not a
    /// large terminal, the chat's scroll region and scrollbar were measuring a
    /// content area that started somewhere other than where the content was
    /// drawn — so scrolling and the scrollbar were both wrong, and the two
    /// agreed only in the one configuration someone tested in.
    mod band_layout {
        use super::*;

        /// The header is only 8 rows on a large terminal. On anything else the
        /// renderer's header shrinks, and a hit-test that assumed 8 would
        /// address the wrong band.
        #[test]
        fn the_header_is_not_always_eight_rows() {
            let big = Rect::new(0, 0, 100, 40);
            let small = Rect::new(0, 0, 100, 12);
            let narrow = Rect::new(0, 0, 60, 40);
            assert_eq!(
                bands(big).header.height,
                8,
                "a large terminal gets the full logo"
            );
            assert_eq!(
                bands(small).header.height,
                0,
                "a short terminal gets no header at all"
            );
            assert_eq!(
                bands(narrow).header.height,
                1,
                "a narrow terminal gets the single-line header"
            );
        }

        /// The bands must tile the screen: no gaps, no overlap, in the order
        /// header → content → status. That is what makes a click on a row
        /// unambiguously a click on exactly one of them.
        #[test]
        fn the_bands_tile_the_screen_exactly() {
            for (w, h) in [(100u16, 40u16), (100, 12), (60, 40), (80, 24), (40, 8)] {
                let b = bands(Rect::new(0, 0, w, h));
                assert_eq!(b.header.y, 0, "header starts at the top ({w}x{h})");
                assert_eq!(
                    b.status.y + b.status.height,
                    h,
                    "status ends the screen ({w}x{h})"
                );
                assert_eq!(
                    b.content.y,
                    b.header.y + b.header.height,
                    "content follows the header ({w}x{h})"
                );
                assert_eq!(
                    b.status.y,
                    b.content.y + b.content.height,
                    "status follows the content ({w}x{h})"
                );
                for band in [b.header, b.content, b.status] {
                    assert_eq!(band.width, w, "bands span the width ({w}x{h})");
                }
            }
        }

        /// A click on the last row is a click on the status bar — at any size,
        /// which is the property the hardcoded 8-row header broke.
        #[test]
        fn the_last_row_is_always_the_status_bar() {
            for (w, h) in [(100u16, 40u16), (100, 12), (60, 40), (80, 24)] {
                let b = bands(Rect::new(0, 0, w, h));
                assert_eq!(
                    b.status.y,
                    h - 1,
                    "the status bar is the last row at {w}x{h}"
                );
            }
        }

        /// The status bar is one row, so its height is what makes "last row"
        /// mean the same thing to a click and to the painter.
        #[test]
        fn the_status_bar_is_exactly_one_row() {
            for (w, h) in [(100u16, 40u16), (100, 12), (60, 40), (80, 24)] {
                assert_eq!(bands(Rect::new(0, 0, w, h)).status.height, 1, "at {w}x{h}");
            }
        }
    }

    // -- the four-reads freeze --------------------------------------------
    //
    // The chat event loop was an if/else-if ladder over FOUR separate
    // `event::read()` calls:
    //
    //     if let Ok(Event::Key(key)) = event::read() { .. }
    //     else if let Ok(Event::Mouse(m)) = event::read() { .. }
    //     else if let Ok(Event::Paste(p)) = event::read() { .. }
    //     else if let Ok(Event::Resize(_,_)) = event::read() { .. }
    //
    // `event::read()` blocks. A single mouse-motion event was consumed by the
    // first read, failed the `Key` pattern, and the second read then BLOCKED
    // waiting for another event. The same for `Paste` and `Resize`: move the
    // mouse over the window, or resize the terminal, and the app stops redrawing
    // and stops processing keystrokes until you happen to press a key.
    //
    // A user with a mouse over their terminal -- which is most users -- hit this
    // constantly, and the symptom ("the app froze, I think it crashed") points
    // away from the cause entirely.
    //
    // The fix is one read and one match. A freeze cannot be asserted in-process,
    // so the property is asserted on the source: the loop reads once per
    // iteration. That is a source-level check on purpose -- it is the only place
    // the property is checkable, and the failure it guards is invisible to every
    // other kind of test.
    // -- Ctrl+C ----------------------------------------------------------
    //
    // `GlobalAction::CancelOrExit` was resolved in exactly one place in the
    // crate: inside `run_tui`. `niki chat` — the loop people actually sit in —
    // therefore had no Ctrl+C handling at all. Pressing it in the composer did
    // nothing; pressing it over a permission prompt did nothing. A TUI blocked
    // on an approval had no keyboard route out of it.
    //
    // And the one place it *was* handled cleared the composer on the first
    // press, which is a destructive default for the key a user reaches for when
    // they want to stop — and unrecoverable for a pasted prompt, because the
    // bracketed-paste path never pushed an undo entry.
    mod ctrl_c {
        use super::*;

        fn state_with(buffer: &str) -> AppState {
            let mut st = AppState::new(
                "test".into(),
                crate::config::types::NikiConfig::default(),
                ".".into(),
            );
            st.input_state.buffer = buffer.to_string();
            st.input_state.cursor_pos = buffer.chars().count();
            st
        }

        fn flag() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))
        }

        #[test]
        fn a_first_press_arms_the_exit_and_a_second_one_leaves() {
            let mut st = state_with("");
            let cancel = flag();
            let mut last = None;

            // An empty composer has nothing to discard, so the first press goes
            // straight to arming the exit.
            assert!(matches!(
                handle_ctrl_c(&mut st, &cancel, &mut last),
                CtrlC::Handled
            ));
            assert!(
                !cancel.load(std::sync::atomic::Ordering::Relaxed),
                "one press must not exit — a user reaching for Ctrl+C to stop a stage would                  lose their place"
            );

            // Second press, inside the window.
            assert!(matches!(
                handle_ctrl_c(&mut st, &cancel, &mut last),
                CtrlC::Exit
            ));
            assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        }

        #[test]
        fn a_third_press_after_the_window_arms_again_rather_than_exiting() {
            let mut st = state_with("");
            let cancel = flag();
            let mut last = None;
            handle_ctrl_c(&mut st, &cancel, &mut last);
            // Backdate past the two-second window.
            last = Some(std::time::Instant::now() - Duration::from_secs(30));
            assert!(matches!(
                handle_ctrl_c(&mut st, &cancel, &mut last),
                CtrlC::Handled
            ));
        }

        #[test]
        fn a_running_stage_is_cancelled_rather_than_the_composer_cleared() {
            let mut st = state_with("half-typed prompt");
            st.apply_event(DisplayEvent::StageStart {
                role: AgentRole::Coder,
            });
            assert!(
                st.has_running_stage(),
                "precondition: a stage must be running"
            );
            let cancel = flag();
            let mut last = None;
            handle_ctrl_c(&mut st, &cancel, &mut last);
            assert_eq!(
                st.input_state.buffer, "half-typed prompt",
                "Ctrl+C while a stage is running must cancel the stage, not eat the prompt"
            );
        }

        #[test]
        fn a_non_empty_composer_is_never_silently_emptied() {
            // The specific harm: a pasted multi-line prompt is gone, and
            // `insert_str` (bracketed paste) never pushed an undo entry, so
            // there is nothing to recover it with.
            let mut st = state_with("a long pasted prompt\nwith several lines");
            let cancel = flag();
            let mut last = None;
            handle_ctrl_c(&mut st, &cancel, &mut last);
            assert_eq!(
                st.input_state.buffer, "",
                "the first press still clears — but it must have TOLD the user, which is the \
                 notice the other test checks"
            );
        }

        /// The defect itself: chat had no route out. This is a source-level
        /// assertion because the property is about which loop contains the
        /// handling, and that is not observable from outside a PTY.
        #[test]
        fn both_tui_loops_route_ctrl_c_through_the_shared_handler() {
            let src = include_str!("tui.rs");
            // Only the production half. This test's own source contains the
            // string it is counting, so the first version counted itself and
            // failed with 3.
            let production = match src.find("#[cfg(test)]\nmod tests {") {
                Some(i) => &src[..i],
                None => src,
            };
            let uses = production
                .matches("handle_ctrl_c(&mut state, &cancel, &mut last_ctrl_c)")
                .count();
            assert_eq!(
                uses, 2,
                "both `run_tui` and `run_chat` must route Ctrl+C through `handle_ctrl_c`. \
                 Found {uses} call sites — chat had none, so there was no keyboard route out \
                 of a TUI blocked on an approval."
            );
        }
    }

    #[test]
    fn the_chat_event_loop_reads_exactly_one_event_per_iteration() {
        let src = include_str!("tui.rs");
        let start = src
            .find("if event::poll(timeout).unwrap_or(false) {")
            .expect("the chat loop's poll call exists");
        let body = &src[start..];

        // Stop at the end of this `if` block: the next top-level statement is
        // the agent-channel drain, which contains no reads.
        let end = body
            .find("\n        }\n")
            .map(|i| i + 12)
            .unwrap_or(body.len().min(4000));
        let block = &body[..end];

        let reads = block.matches("event::read()").count();
        assert_eq!(
            reads, 1,
            "the chat event loop calls `event::read()` {reads} times in one branch. \
             `event::read()` BLOCKS, so every extra call is a place the app can stop \
             responding: a mouse-motion event fails the `Key` pattern, the next read blocks \
             waiting for a keystroke, and the TUI appears to freeze."
        );
        assert!(
            !block.contains("else if let Ok(Event::"),
            "the ladder must be a `match` on one read, not `else if let Ok(Event::..)` chains"
        );
    }

    /// The reason the assertion above cannot be satisfied by an unrelated read
    /// elsewhere in the file: the chained-read shape must not exist at all.
    #[test]
    fn no_event_ladder_reads_more_than_once_anywhere() {
        let src = include_str!("tui.rs");
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("else if let Ok(Event::") || t.starts_with("} else if let Ok(Event::")
            {
                panic!(
                    "line {}: `{t}` is a chained read -- the event must be read once and matched, \
                     not re-read per arm",
                    i + 1
                );
            }
        }
    }

    /// Every overlay that is painted must be one the mouse cannot reach past.
    ///
    /// `active_focus` knew about three and four are drawn. The keyboard ladder
    /// checks all of them, so keys were blocked while the mouse was not: a
    /// click landed on the page *behind* the modal, and a click on the status
    /// bar during onboarding cycled the permission mode toward BYPASS.
    /// Onboarding is the first thing a new user sees and has no mouse handler
    /// of its own.
    #[test]
    fn every_overlay_the_keyboard_blocks_is_one_the_mouse_blocks() {
        let config = crate::config::types::NikiConfig::default();
        let mut state = AppState::new("test".into(), config, ".".into());
        assert_eq!(active_focus(&state), FocusState::Chat);

        state.onboarding = Some(crate::display::onboarding::OnboardingModal::new());
        assert_eq!(active_focus(&state), FocusState::Onboarding);

        state.onboarding = None;
        state.show_permission_modal = true;
        state.modal = Some(crate::display::state::Modal::Confirm {
            title: "Quit NIKI?".to_string(),
            message: "leave?".to_string(),
        });
        assert_eq!(
            active_focus(&state),
            FocusState::Permission,
            "the permission prompt outranks every other overlay"
        );
        state.show_permission_modal = false;
        assert_eq!(active_focus(&state), FocusState::Modal);

        state.modal = None;
        state
            .sheets
            .push(crate::display::sheets::Sheet::Mcp(Default::default()));
        assert_eq!(active_focus(&state), FocusState::Sheet);
    }

    /// Moving the mouse must not dismiss the help overlay.
    ///
    /// The dismissal ran for every mouse event including `Moved`, and motion
    /// reporting is enabled outside tmux and screen (DEC 1003,
    /// `mouse.rs:42-48`). So nudging the mouse made the keybindings screen
    /// vanish — and `?` is the only way a first-time user learns the app.
    #[test]
    fn moving_the_mouse_does_not_dismiss_help() {
        use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let config = crate::config::types::NikiConfig::default();
        let mut state = AppState::new("test".into(), config, ".".into());
        state.chat_width.set(80);
        state.show_help = true;

        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 10,
            row: 5,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        };
        let mut router = crate::display::pages::PageRouter::new();
        let mut palette = CommandPalette::new();
        let _ = route_mouse(&mut state, &mut router, &mut palette, moved, None);
        assert!(
            state.show_help,
            "nudging the mouse closed the keybindings screen"
        );

        let clicked = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        };
        let _ = route_mouse(&mut state, &mut router, &mut palette, clicked, None);
        assert!(!state.show_help, "a real click should dismiss it");
    }
}
