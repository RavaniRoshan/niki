//! Permission request modal overlay.
//!
//! Claude Code–style layout:
//!   tool line → blue separator → description → dotted separator → options → footer
//! Options: Allow once · Allow always · Deny · Deny always
//! Keybindings: ↑/↓ navigate · Enter/Y confirm · Esc/N cancel · Ctrl+E explanation · Ctrl+D raw params

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::components::list_cursor::ListCursor;
use crate::display::state::{AppState, PermissionRequest};
use crate::display::theme;
use crate::permissions::PermissionAction;

/// Selectable options, in row order.
pub const OPTIONS: [&str; 4] = ["Allow once", "Allow always", "Deny", "Deny always"];

/// Permission scope labels.
pub const SCOPES: [&str; 3] = ["Turn", "Session", "Project"];

/// First option row inside the modal border, computed from the same layout
/// the renderer emits (TUI-013). Base 12 rows (label, tool, separators,
/// scope block, options header); a description adds 2; the detail panel
/// adds 2 + up to 5 param lines. Render pads to this row and the hit-test
/// reads it, so paint and clicks can never desync.
pub fn option_first_row(request: &PermissionRequest, show_detail: bool) -> u16 {
    let mut row: u16 = 12;
    if !request.description.is_empty() {
        row += 2;
    }
    if show_detail {
        if let Some(ref params) = request.params {
            row += 2 + params.lines().take(5).count().min(5) as u16;
        }
    }
    row
}

/// The cursor over the permission options, seeded from `AppState`.
pub fn cursor(state: &AppState) -> ListCursor {
    ListCursor::with_selected(OPTIONS.len(), state.permission_selected)
}

/// The [`PermissionAction`] a given option row maps to.
/// ("Allow always" / "Deny always" resolve to Allow/Deny here; persistent
/// variants are a future enhancement — the protocol has no persistent flag yet.)
pub fn action_for(index: usize) -> PermissionAction {
    match index {
        0 | 1 => PermissionAction::Allow,
        _ => PermissionAction::Deny,
    }
}

/// Geometry of the modal — shared by the renderer and the hit-test.
/// Height follows the content (options always visible); clamped to the area.
pub fn modal_rect(area: Rect, request: &PermissionRequest, show_detail: bool) -> Rect {
    let content_rows = option_first_row(request, show_detail) + OPTIONS.len() as u16 + 1;
    let modal_width = 60u16.min(area.width.saturating_sub(4));
    let modal_height = (content_rows + 2).min(area.height).max(8);
    Rect {
        x: area.width.saturating_sub(modal_width) / 2,
        y: area.height.saturating_sub(modal_height) / 2,
        width: modal_width,
        height: modal_height,
    }
}

/// Hit-test a mouse position against the option rows, returning the row index.
/// Geometry comes from [`option_first_row`], shared with the renderer.
pub fn click_index(
    area: Rect,
    x: u16,
    y: u16,
    request: &PermissionRequest,
    show_detail: bool,
) -> Option<usize> {
    let modal = modal_rect(area, request, show_detail);
    let inner_left = modal.x + 1;
    let inner_right = modal.x + modal.width.saturating_sub(1);
    if x < inner_left || x >= inner_right {
        return None;
    }
    let first = modal.y + 1 + option_first_row(request, show_detail);
    if y < first {
        return None;
    }
    let idx = (y - first) as usize;
    if idx < OPTIONS.len() { Some(idx) } else { None }
}

/// Render the permission modal overlay.
///
/// Claude Code–style layout (top → bottom inside the modal border):
///   1. "The agent wants to run:" label
///   2. `$ <command>`  (tool call line)
///   3. Blue separator  (rgb 177,185,249)
///   4. Description + hint
///   5. Scope selector (Turn/Session/Project)
///   6. Dotted separator (rgb 80,80,80)
///   7. Options (Allow once · Allow always · Deny · Deny always)
///   8. Footer hint
pub fn render_permission_modal(
    frame: &mut Frame,
    request: &PermissionRequest,
    area: Rect,
    state: &AppState,
) {
    let modal_area = modal_rect(area, request, state.show_permission_detail);

    frame.render_widget(Clear, modal_area);

    // Attention pulse on the border while the modal demands a decision:
    // alternates every ~300ms at 60fps ticks, static when reduced-motion.
    // No per-modal clock needed — pulsing *is* the steady state here.
    let pulse = crate::display::motion::pulse_phase(
        state.tick,
        18,
        crate::display::motion::reduced(state.config.ui.reduced_motion),
    );
    let border = if pulse {
        theme::warning()
    } else {
        theme::border()
    };
    let block = Block::default()
        .title(" Permission Required ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::bg_elevated()));

    frame.render_widget(block, modal_area);

    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    let selected = cursor(state).selected;
    let blue = Style::default().fg(theme::accent());
    let dotted = Style::default().fg(theme::fg_dim());

    let mut lines = vec![
        Line::from(Span::styled("The agent wants to run:", theme::text_dim())),
        Line::from(""),
        Line::from(Span::styled(
            // TUI-023: long commands truncate to the modal instead of
            // overflowing into the border (width-aware, no byte slicing).
            theme::truncate_str_ellipsis(&format!("  $ {}", request.command), inner.width as usize),
            Style::default()
                .fg(theme::primary())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "─".repeat(inner.width.saturating_sub(4) as usize),
            blue,
        )),
        Line::from(""),
    ];

    if !request.description.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", request.description),
            theme::text(),
        )));
        lines.push(Line::from(""));
    }

    // Detail panel (toggled by Ctrl+D)
    if state.show_permission_detail {
        if let Some(ref params) = request.params {
            lines.push(Line::from(Span::styled(
                "  Raw parameters:",
                Style::default()
                    .fg(theme::text_dim())
                    .add_modifier(Modifier::ITALIC),
            )));
            for line in params.lines().take(5) {
                lines.push(Line::from(Span::styled(
                    format!("    {}", line),
                    theme::text_dim(),
                )));
            }
            lines.push(Line::from(""));
        }
    }

    // Scope selector
    let scope_idx = state.permission_scope.min(SCOPES.len() - 1);
    let scope_spans: Vec<Span> = SCOPES
        .iter()
        .enumerate()
        .flat_map(|(i, scope)| {
            let is_selected = i == scope_idx;
            let style = if is_selected {
                Style::default()
                    .fg(theme::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::text_dim())
            };
            vec![
                Span::styled(if is_selected { "●" } else { "○" }, style),
                Span::styled(format!(" {} ", scope), style),
            ]
        })
        .collect();
    lines.push(Line::from(Span::styled("  Scope:", theme::text_dim())));
    lines.push(Line::from(scope_spans));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        format!("  {} options:", OPTIONS.len()),
        theme::text_dim(),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", "·".repeat(inner.width.saturating_sub(4) as usize)),
        dotted,
    )));
    lines.push(Line::from(""));

    // Pad to the first option row (shared with the hit-test).
    let first_option_row = option_first_row(request, state.show_permission_detail);
    while lines.len() < first_option_row as usize {
        lines.push(Line::from(""));
    }

    debug_assert_eq!(lines.len() as u16, first_option_row);
    for (i, opt) in OPTIONS.iter().enumerate() {
        let is_selected = i == selected;
        let style = if is_selected {
            Style::default()
                .fg(theme::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::text())
        };
        lines.push(Line::from(vec![
            Span::styled(if is_selected { "  ● " } else { "  ○ " }, style),
            Span::styled(*opt, style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[↑/↓] Select  [Enter/Y] Confirm  [Esc/N] Deny  [Tab] Scope  [Ctrl+D] Detail",
        theme::text_dim(),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Compact one-line rendering of the options (used by tests and any narrow
/// single-line surface that cannot spare three rows).
pub fn render_permission_options(selected: usize) -> String {
    OPTIONS
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            if i == selected {
                format!("● {}", opt)
            } else {
                format!("○ {}", opt)
            }
        })
        .collect::<Vec<_>>()
        .join("    ")
}

/// Answer a permission prompt from the keyboard.
///
/// Returns `true` when the key was consumed by the prompt. Callers must treat
/// that as "the prompt owns input": every other handler is skipped, because a
/// key that moves a permission cursor must not also type into the composer
/// behind it.
///
/// This exists as one function because it used to exist as one copy inside
/// `run_tui` and no copy at all inside `run_chat`. The prompt *renders* in
/// both — it is drawn by the shared `render` — so in `niki chat` a user was
/// shown a permission request, given no key that answered it, and watched it
/// expire into `Deny` after five seconds. The failure looked like the program
/// rejecting a command on its own.
///
/// The asymmetry with the mouse path is deliberate and shared: `y`/`n` answer
/// the highlighted option's question directly, `Enter` submits whatever is
/// highlighted, and any other key is *not* a silent `Deny` — it is ignored, so
/// a stray keystroke cannot answer a question the user never read.
pub fn handle_key(key: &KeyEvent, state: &mut AppState) -> bool {
    if !state.show_permission_modal {
        return false;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            let mut cursor = cursor(state);
            cursor.prev();
            state.permission_selected = cursor.selected;
            true
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let mut cursor = cursor(state);
            cursor.next();
            state.permission_selected = cursor.selected;
            true
        }
        KeyCode::Tab => {
            state.permission_scope = (state.permission_scope + 1) % SCOPES.len();
            true
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.show_permission_detail = !state.show_permission_detail;
            true
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            respond(state, action_for(state.permission_selected))
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            respond(state, PermissionAction::Deny)
        }
        KeyCode::Enter => {
            let cursor = cursor(state);
            respond(
                state,
                cursor
                    .submit()
                    .map(action_for)
                    .unwrap_or(PermissionAction::Deny),
            )
        }
        // Consumed, but not answered. Anything else is a key the user typed
        // while a prompt was up; treating it as a denial would let a stray
        // character decide a permission question.
        _ => true,
    }
}

/// Send `action` for the pending request and close the prompt.
fn respond(state: &mut AppState, action: PermissionAction) -> bool {
    if let Some(req) = state.permission_request.take() {
        let _ = req.response_tx.send(action);
    }
    state.show_permission_modal = false;
    state.show_permission_detail = false;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_permission_options_test() {
        let result = render_permission_options(0);
        assert!(result.contains("● Allow once"));
        assert!(result.contains("○ Allow always"));
        assert!(result.contains("○ Deny"));
        assert!(result.contains("○ Deny always"));
    }

    #[test]
    fn render_permission_options_selected_1() {
        let result = render_permission_options(1);
        assert!(result.contains("○ Allow once"));
        assert!(result.contains("● Allow always"));
    }

    #[test]
    fn click_index_maps_option_rows() {
        let area = Rect::new(0, 0, 100, 40);
        let (tx, _rx) = std::sync::mpsc::channel();
        let req = PermissionRequest {
            tool_name: "sandbox_exec".to_string(),
            command: "ls".to_string(),
            description: String::new(),
            params: None,
            response_tx: tx,
        };
        let modal = modal_rect(area, &req, false);
        let first = modal.y + 1 + option_first_row(&req, false);
        assert_eq!(first, modal.y + 1 + 12);
        assert_eq!(click_index(area, modal.x + 3, first, &req, false), Some(0));
        assert_eq!(
            click_index(area, modal.x + 3, first + 1, &req, false),
            Some(1)
        );
        assert_eq!(
            click_index(area, modal.x + 3, first + 2, &req, false),
            Some(2)
        );
        assert_eq!(
            click_index(area, modal.x + 3, first + 3, &req, false),
            Some(3)
        );
        // Rows above the options and the hint row below are not selectable.
        assert_eq!(click_index(area, modal.x + 3, first - 1, &req, false), None);
        assert_eq!(click_index(area, modal.x + 3, first + 4, &req, false), None);
        // Border columns are not selectable.
        assert_eq!(click_index(area, modal.x, first, &req, false), None);
    }

    #[test]
    fn click_index_tracks_description_and_detail_rows() {
        let area = Rect::new(0, 0, 100, 40);
        let (tx, _rx) = std::sync::mpsc::channel();
        let req = PermissionRequest {
            tool_name: "sandbox_exec".to_string(),
            command: "rm -rf /".to_string(),
            description: "deletes everything".to_string(),
            params: Some("a\nb\nc".to_string()),
            response_tx: tx,
        };
        // 12 base + 2 description + 2 + 3 param lines = row 19.
        let modal = modal_rect(area, &req, true);
        let first = modal.y + 1 + option_first_row(&req, true);
        assert_eq!(first, modal.y + 1 + 19);
        assert_eq!(click_index(area, modal.x + 3, first, &req, true), Some(0));
        assert_eq!(
            click_index(area, modal.x + 3, first + 3, &req, true),
            Some(3)
        );
        // Without the detail panel the rows move back up.
        let collapsed_modal = modal_rect(area, &req, false);
        let collapsed = collapsed_modal.y + 1 + option_first_row(&req, false);
        assert_eq!(collapsed, collapsed_modal.y + 1 + 14);
        assert_eq!(
            click_index(area, collapsed_modal.x + 3, collapsed, &req, false),
            Some(0)
        );
    }

    #[test]
    fn cursor_wraps_and_maps_to_actions() {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("test".to_string(), config, ".".into());
        state.permission_selected = 0;
        let mut c = cursor(&state);
        c.prev();
        assert_eq!(c.selected, 3);
        assert!(matches!(action_for(c.selected), PermissionAction::Deny));
        c.next();
        assert_eq!(c.selected, 0);
        assert!(matches!(action_for(0), PermissionAction::Allow));
        assert!(matches!(action_for(1), PermissionAction::Allow));
    }

    #[test]
    fn render_with_description_and_detail_paints_options() {
        // Previously the pad/assert assumed a fixed row 10 and would fail
        // with description/detail lines present.
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("test".to_string(), config, ".".into());
        state.show_permission_detail = true;
        let (tx, _rx) = std::sync::mpsc::channel();
        let req = PermissionRequest {
            tool_name: "sandbox_exec".to_string(),
            command: "rm -rf /".to_string(),
            description: "deletes everything".to_string(),
            params: Some("a\nb".to_string()),
            response_tx: tx,
        };
        let backend = ratatui::backend::TestBackend::new(100, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_permission_modal(f, &req, f.area(), &state))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();
        assert!(text.contains("Allow once"), "{text}");
        assert!(text.contains("deletes everything"), "{text}");
    }

    #[test]
    fn render_long_unicode_command_truncates() {
        // TUI-023: long commands stay inside the modal (width-aware ellipsis,
        // no byte slicing).
        let config = crate::config::NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        let (tx, _rx) = std::sync::mpsc::channel();
        let req = PermissionRequest {
            tool_name: "sandbox_exec".to_string(),
            command: "déploie --all --force 日🎉".repeat(6),
            description: String::new(),
            params: None,
            response_tx: tx,
        };
        let backend = ratatui::backend::TestBackend::new(80, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_permission_modal(f, &req, f.area(), &state))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();
        assert!(text.contains("déploie"), "{text}");
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use crate::display::state::PermissionRequest;

    fn state_with_request() -> (AppState, std::sync::mpsc::Receiver<PermissionAction>) {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("t".to_string(), config, ".".into());
        let (tx, rx) = std::sync::mpsc::channel();
        state.permission_request = Some(PermissionRequest {
            tool_name: "sandbox_exec".into(),
            command: "rm -rf /".into(),
            description: String::new(),
            params: None,
            response_tx: tx,
        });
        state.show_permission_modal = true;
        (state, rx)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// The defect: `niki chat` rendered the prompt and had no key that could
    /// answer it, so the sandbox's five-second timeout denied the command. A
    /// user watching that had no way to have said no — and no way to have said
    /// yes either.
    #[test]
    fn a_prompt_can_be_answered_with_y_and_n() {
        let (mut state, rx) = state_with_request();
        assert!(handle_key(&key(KeyCode::Char('y')), &mut state));
        assert_eq!(rx.try_recv().unwrap(), PermissionAction::Allow);
        assert!(!state.show_permission_modal, "the prompt must close");

        let (mut state, rx) = state_with_request();
        assert!(handle_key(&key(KeyCode::Char('n')), &mut state));
        assert_eq!(rx.try_recv().unwrap(), PermissionAction::Deny);
    }

    #[test]
    fn a_stray_key_answers_nothing() {
        // The old inline handler mapped every unmatched key to `Deny`. A user
        // typing into the composer behind an open prompt would silently
        // refuse the command with one keystroke.
        let (mut state, rx) = state_with_request();
        assert!(
            handle_key(&key(KeyCode::Char('x')), &mut state),
            "the prompt still owns the key"
        );
        assert!(
            rx.try_recv().is_err(),
            "an unrecognised key must not decide a permission question"
        );
        assert!(state.show_permission_modal, "and the prompt stays up");
    }

    #[test]
    fn arrows_move_the_cursor_and_enter_submits_the_highlighted_option() {
        let (mut state, rx) = state_with_request();
        assert_eq!(state.permission_selected, 0);
        handle_key(&key(KeyCode::Down), &mut state);
        assert_eq!(state.permission_selected, 1);
        // Option 1 is "Allow always", which resolves to Allow.
        handle_key(&key(KeyCode::Enter), &mut state);
        assert_eq!(rx.try_recv().unwrap(), PermissionAction::Allow);

        // Deny is reachable, not just allow.
        let (mut state, rx) = state_with_request();
        handle_key(&key(KeyCode::Down), &mut state);
        handle_key(&key(KeyCode::Down), &mut state);
        assert_eq!(state.permission_selected, 2, "third option is Deny");
        handle_key(&key(KeyCode::Enter), &mut state);
        assert_eq!(rx.try_recv().unwrap(), PermissionAction::Deny);
    }

    #[test]
    fn a_prompt_with_no_request_still_closes() {
        // The detail flag can be left set; answering must clear it or the next
        // prompt renders with a panel nobody asked for.
        let (mut state, _rx) = state_with_request();
        state.show_permission_detail = true;
        handle_key(&key(KeyCode::Esc), &mut state);
        assert!(!state.show_permission_modal);
        assert!(!state.show_permission_detail);
    }

    #[test]
    fn keys_pass_through_when_no_prompt_is_up() {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("t".to_string(), config, ".".into());
        assert!(
            !handle_key(&key(KeyCode::Char('y')), &mut state),
            "a y with no prompt must reach the composer, not be swallowed"
        );
    }
}
