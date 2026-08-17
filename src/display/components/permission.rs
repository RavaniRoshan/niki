//! Permission request modal overlay.
//!
//! The three options (`Allow once` / `Allow always` / `Deny`) are rendered one
//! per row and driven by the shared [`ListCursor`], so keyboard nav and mouse
//! hover/click behave exactly like the command palette and slash menu.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::components::list_cursor::ListCursor;
use crate::display::state::{AppState, PermissionRequest};
use crate::display::theme;
use crate::permissions::PermissionAction;

/// Selectable options, in row order.
pub const OPTIONS: [&str; 3] = ["Allow once", "Allow always", "Deny"];

/// Height of the modal: borders + prompt + command + one row per option + hint.
const MODAL_HEIGHT: u16 = 13;
/// Row offset (inside the modal border) of the first option row.
const FIRST_OPTION_ROW: u16 = 4;

/// The cursor over the permission options, seeded from `AppState`.
pub fn cursor(state: &AppState) -> ListCursor {
    ListCursor::with_selected(OPTIONS.len(), state.permission_selected)
}

/// The [`PermissionAction`] a given option row maps to.
/// (`Allow always` currently resolves to `Allow` — the protocol has no
/// persistent variant, so we do not change the display event contract.)
pub fn action_for(index: usize) -> PermissionAction {
    match index {
        0 | 1 => PermissionAction::Allow,
        _ => PermissionAction::Deny,
    }
}

/// Geometry of the modal — shared by the renderer and the hit-test.
pub fn modal_rect(area: Rect) -> Rect {
    let modal_width = 60u16.min(area.width.saturating_sub(4));
    let modal_height = MODAL_HEIGHT.min(area.height);
    Rect {
        x: area.width.saturating_sub(modal_width) / 2,
        y: area.height.saturating_sub(modal_height) / 2,
        width: modal_width,
        height: modal_height,
    }
}

/// Hit-test a mouse position against the option rows, returning the row index.
pub fn click_index(area: Rect, x: u16, y: u16) -> Option<usize> {
    let modal = modal_rect(area);
    let inner_left = modal.x + 1;
    let inner_right = modal.x + modal.width.saturating_sub(1);
    if x < inner_left || x >= inner_right {
        return None;
    }
    let first = modal.y + 1 + FIRST_OPTION_ROW;
    if y < first {
        return None;
    }
    let idx = (y - first) as usize;
    if idx < OPTIONS.len() { Some(idx) } else { None }
}

/// Render the permission modal overlay.
pub fn render_permission_modal(
    frame: &mut Frame,
    request: &PermissionRequest,
    area: Rect,
    state: &AppState,
) {
    let modal_area = modal_rect(area);

    // Clear the area (dim background)
    frame.render_widget(Clear, modal_area);

    // Border block
    let block = Block::default()
        .title(" Permission Required ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border()))
        .style(Style::default().bg(theme::bg_elevated()));

    frame.render_widget(block, modal_area);

    // Inner layout
    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    let selected = cursor(state).selected;

    let mut lines = vec![
        Line::from(Span::styled("The agent wants to run:", theme::text_dim())),
        Line::from(""),
        Line::from(Span::styled(
            format!("  $ {}", request.command),
            Style::default()
                .fg(theme::primary())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // One row per option so hover-highlight / click-to-select line up 1:1.
    debug_assert_eq!(lines.len() as u16, FIRST_OPTION_ROW);
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
        "[↑/↓] Select  [Enter] Confirm  [Esc] Deny",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_permission_options_test() {
        let result = render_permission_options(0);
        assert!(result.contains("● Allow once"));
        assert!(result.contains("○ Allow always"));
        assert!(result.contains("○ Deny"));
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
        let modal = modal_rect(area);
        let first = modal.y + 1 + FIRST_OPTION_ROW;
        assert_eq!(click_index(area, modal.x + 3, first), Some(0));
        assert_eq!(click_index(area, modal.x + 3, first + 1), Some(1));
        assert_eq!(click_index(area, modal.x + 3, first + 2), Some(2));
        // Rows above the options and the hint row below are not selectable.
        assert_eq!(click_index(area, modal.x + 3, first - 1), None);
        assert_eq!(click_index(area, modal.x + 3, first + 3), None);
        // Border columns are not selectable.
        assert_eq!(click_index(area, modal.x, first), None);
    }

    #[test]
    fn cursor_wraps_and_maps_to_actions() {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("test".to_string(), config, ".".into());
        state.permission_selected = 0;
        let mut c = cursor(&state);
        c.prev();
        assert_eq!(c.selected, 2);
        assert!(matches!(action_for(c.selected), PermissionAction::Deny));
        c.next();
        assert_eq!(c.selected, 0);
        assert!(matches!(action_for(0), PermissionAction::Allow));
        assert!(matches!(action_for(1), PermissionAction::Allow));
    }
}
