//! Permission request modal overlay.
//!
//! Claude Code–style layout:
//!   tool line → blue separator → description → dotted separator → options → footer
//! Options: Allow · Deny
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
///
/// **Two, not four.** It used to be
/// `["Allow once", "Allow always", "Deny", "Deny always"]`, and
/// [`action_for`] mapped indices `0 | 1` both to `Allow` and `2 | 3` both to
/// `Deny`. `PermissionAction` is `enum { Allow, Deny }` — there is no persistent
/// variant in the protocol, and nothing persisted anything.
///
/// So a user who deliberately picked **"Allow always"**, meaning *trust this
/// command for the rest of the session*, got **"Allow once"**: the identical
/// command asked again on the next step, with no indication that their choice
/// had been reinterpreted. The modal was not four options with two behaviours —
/// it was two options wearing four labels, one of which promised something the
/// product cannot do.
///
/// The labels now match the behaviour. Restoring persistence is a product
/// decision recorded in `ROADMAP.md`, not a line to add here: it needs a
/// protocol field and a store, and shipping a half of it is what caused this.
pub const OPTIONS: [&str; 2] = ["Allow", "Deny"];

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
///
/// One row, one behaviour: index 0 is the only way to allow and index 1 the
/// only way to deny. The old mapping let four rows collapse onto two actions,
/// which is why the labels above are now two.
pub fn action_for(index: usize) -> PermissionAction {
    match index {
        0 => PermissionAction::Allow,
        _ => PermissionAction::Deny,
    }
}

/// Whether the detail panel survives at this size.
///
/// **The options must never be the thing that gets cut.** `modal_rect` clamps the
/// modal to the terminal, and it clamps from the bottom — so when the detail
/// panel makes the box taller than the screen, the *last option* lands past the
/// last row and a user on a small terminal cannot see how to deny.
///
/// It has always been this way: the old test asserted `Allow once`, which is
/// option **0** and sat one row above the cut, so the layout check was green
/// while the second option was off-screen. Asserting both options is what made it
/// visible.
fn effective_detail(area: Rect, request: &PermissionRequest, show_detail: bool) -> bool {
    if !show_detail {
        return false;
    }
    // Everything the modal wants, borders included. If it does not fit, the
    // detail panel is what goes — the command line stays, because a decision
    // about an unseen command is worse than one about a seen one.
    let wanted = option_first_row(request, true) + OPTIONS.len() as u16 + 1 + 2;
    wanted <= area.height
}

/// Geometry of the modal — shared by the renderer and the hit-test.
/// Height follows the content (options always visible); clamped to the area.
pub fn modal_rect(area: Rect, request: &PermissionRequest, show_detail: bool) -> Rect {
    let show_detail = effective_detail(area, request, show_detail);
    let content_rows = option_first_row(request, show_detail) + OPTIONS.len() as u16 + 1;
    let modal_width = 60u16.min(area.width.saturating_sub(4));
    // `.min(area.height)` then `.max(8)` produced a modal *taller than the
    // terminal* on a small one, and rendering it indexed outside the buffer: a
    // panic, in the one moment the user cannot work around, triggered by
    // resizing the window while a prompt was up. The floor comes from the
    // area; the content only sets what it would like.
    let modal_height = (content_rows + 2).max(8).min(area.height);
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
    let show_detail = effective_detail(area, request, show_detail);
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
///   7. Options (Allow · Deny)
///   8. Footer hint
pub fn render_permission_modal(
    frame: &mut Frame,
    request: &PermissionRequest,
    area: Rect,
    state: &AppState,
) {
    // Same flag `modal_rect` used. If the renderer consulted the raw
    // preference it would draw a detail panel the geometry had already decided
    // does not fit — which is how the last option ends up off-screen.
    let show_detail = effective_detail(area, request, state.show_permission_detail);
    let modal_area = modal_rect(area, request, state.show_permission_detail);
    // Nothing is legible in a box this small, and drawing into it indexes
    // outside the buffer. The question modal and onboarding guard the same way.
    if modal_area.width < 8 || modal_area.height < 4 {
        return;
    }

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
    if show_detail {
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

    // The scope selector is **gone**, and it is worth saying why it was there
    // at all.
    //
    // It rendered `Turn / Session / Project` with a selected marker, and `Tab`
    // cycled it — and `state.permission_scope` then reached **nothing**. The
    // response carried `action_for(index)` and nothing else, so whichever scope
    // was highlighted changed no behaviour. It was three labels of pure
    // decoration on a blocking decision surface, one keypress away from
    // implying that the user's answer had been made more specific.
    //
    // Scope returns when scope exists: a field on `PermissionAction`, and
    // somewhere to put the answer. Until then the modal says what it does.

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
    let first_option_row = option_first_row(request, show_detail);
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
        // `Tab` used to cycle the scope selector, which reached nothing — see
        // the note where the selector used to be rendered. It now falls
        // through to the caller, which is what a key that does nothing here
        // should do.
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
        assert!(result.contains("● Allow"));
        assert!(result.contains("○ Deny"));
        assert!(result.contains("○ Deny"));
    }

    #[test]
    fn render_permission_options_selected_1() {
        let result = render_permission_options(1);
        assert!(result.contains("○ Allow"));
        assert!(result.contains("● Deny"));
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
        // One row per option, and a row past the last one is **not** an option.
        // With four options this could not be written; with two it is the
        // boundary that matters, because a click below "Deny" must not resolve
        // to `action_for(2)` — which is `Deny`, and would answer a question the
        // user did not ask by clicking the padding.
        assert_eq!(
            click_index(area, modal.x + 3, first + OPTIONS.len() as u16, &req, false),
            None
        );

        // Rows above the options and the hint row below are not selectable.
        assert_eq!(click_index(area, modal.x + 3, first - 1, &req, false), None);
        assert_eq!(
            click_index(
                area,
                modal.x + 3,
                first + OPTIONS.len() as u16 + 1,
                &req,
                false
            ),
            None,
            "and the hint row below the options is not one either"
        );
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
            click_index(
                area,
                modal.x + 3,
                first + OPTIONS.len() as u16 - 1,
                &req,
                true
            ),
            Some(OPTIONS.len() - 1)
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
        assert_eq!(
            c.selected,
            OPTIONS.len() - 1,
            "the cursor wraps to the last option"
        );
        assert!(matches!(action_for(c.selected), PermissionAction::Deny));
        c.next();
        assert_eq!(c.selected, 0);
        assert!(matches!(action_for(0), PermissionAction::Allow));
        assert!(matches!(action_for(1), PermissionAction::Deny));
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
        assert!(text.contains("Allow"), "{text}");
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
        assert_eq!(state.permission_selected, 0, "Allow is highlighted first");
        handle_key(&key(KeyCode::Enter), &mut state);
        assert_eq!(rx.try_recv().unwrap(), PermissionAction::Allow);

        // Deny is reachable, not just allow.
        let (mut state, rx) = state_with_request();
        handle_key(&key(KeyCode::Down), &mut state);
        assert_eq!(
            state.permission_selected,
            OPTIONS.len() - 1,
            "the arrow moves to the only other option"
        );
        handle_key(&key(KeyCode::Enter), &mut state);
        assert_eq!(rx.try_recv().unwrap(), PermissionAction::Deny);
    }

    /// **Every option must do something different.**
    ///
    /// This is the test that would have caught the defect rather than the fix.
    /// The modal used to offer `Allow once / Allow always / Deny / Deny always`
    /// and `action_for` mapped `0 | 1` to `Allow` and `2 | 3` to `Deny` — so a
    /// user who picked **"Allow always"**, meaning *trust this for the rest of
    /// the session*, silently got **"Allow once"**, and the same command asked
    /// again on the next step.
    ///
    /// `PermissionAction` is `enum { Allow, Deny }`: there is no persistent
    /// variant and nothing persisted anything. The fix was to say what the
    /// product does; this is what keeps the next person from re-adding the
    /// labels without the mechanism.
    #[test]
    fn every_option_resolves_to_a_distinct_action() {
        let mut seen: Vec<PermissionAction> = Vec::new();
        for (i, label) in OPTIONS.iter().enumerate() {
            let action = action_for(i);
            assert!(
                !seen.contains(&action),
                "`{label}` resolves to the same action as an earlier option: a \
                 second label for one behaviour is a promise the product does not \
                 keep"
            );
            seen.push(action);
        }
        assert_eq!(
            seen.len(),
            OPTIONS.len(),
            "and every option is reachable by index"
        );
    }

    /// No scope is offered that the protocol cannot carry.
    ///
    /// The selector rendered `Turn / Session / Project` and `Tab` cycled it, and
    /// `state.permission_scope` then reached **nothing**: the response carried
    /// `action_for(index)` and nothing else. Three labels of decoration on a
    /// blocking decision surface.
    ///
    /// The first version of this test asserted `state.permission_scope == 0`,
    /// which is true whether or not a selector exists — it was green against the
    /// selector being reinstated, which is the fourth vacuous pass in this batch
    /// alone. It now reads the **rendered modal**, because that is where the
    /// promise was made to the user.
    ///
    /// If a scope ever comes back it should come back with a `PermissionAction`
    /// that carries it; then this test is rewritten to check that the choice
    /// reaches `action_for`, rather than deleted.
    #[test]
    fn no_scope_is_offered_that_the_protocol_cannot_carry() {
        let (state, _rx) = state_with_request();
        let req = PermissionRequest {
            tool_name: "sandbox_exec".into(),
            command: "rm -rf /".into(),
            description: "deletes everything".into(),
            params: Some("a\nb".into()),
            response_tx: {
                let (tx, _rx) = std::sync::mpsc::channel();
                tx
            },
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
        for scope in ["Scope:", "Turn", "Session", "Project"] {
            assert!(
                !text.contains(scope),
                "the modal offers `{scope}`, which nothing can carry: the response \
                 is `action_for(index)` and nothing else. A selector over a field \
                 that reaches no behaviour is decoration on a blocking question."
            );
        }
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
/// A terminal too small for the box must not take the run down with it.
///
/// The geometry clamped the height to the area and then floored it at 8, so a
/// short terminal produced a modal *taller than the screen* and the renderer
/// indexed outside the buffer. A user who resized their window while a prompt
/// was up got a panic — in the one moment they could not work around it, and
/// with a destructive command waiting on the answer.
#[test]
fn a_tiny_terminal_does_not_break_the_permission_modal() {
    use ratatui::backend::TestBackend;

    for (w, h) in [(1u16, 1u16), (4, 2), (10, 3), (30, 7), (100, 40)] {
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let config = crate::config::NikiConfig::default();
        let state = crate::display::state::AppState::new("t".into(), config, ".".into());
        let (tx, _rx) = std::sync::mpsc::channel();
        let req = PermissionRequest {
            tool_name: "sandbox_exec".into(),
            command: "rm -rf /".into(),
            description: String::new(),
            params: None,
            response_tx: tx,
        };
        terminal
            .draw(|f| {
                render_permission_modal(f, &req, f.area(), &state);
            })
            .unwrap_or_else(|e| panic!("{w}x{h} must render: {e}"));
    }
}
