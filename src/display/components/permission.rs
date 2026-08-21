//! Permission request modal overlay.
//!
//! Claude Code–style layout:
//!   tool line → blue separator → description → dotted separator → options → footer
//! Options: Allow once · Allow always · Deny · Deny always
//! Keybindings: ↑/↓ navigate · Enter/Y confirm · Esc/N cancel · Ctrl+E explanation · Ctrl+D raw params

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
pub const OPTIONS: [&str; 4] = ["Allow once", "Allow always", "Deny", "Deny always"];

/// Permission scope labels.
pub const SCOPES: [&str; 3] = ["Turn", "Session", "Project"];

/// Height of the modal: borders + tool line + blue sep + description + scope + dotted sep + 4 options + hint.
const MODAL_HEIGHT: u16 = 20;
/// Row offset (inside the modal border) of the first option row.
const FIRST_OPTION_ROW: u16 = 10;

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
    let modal_area = modal_rect(area);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Permission Required ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border()))
        .style(Style::default().bg(theme::bg_elevated()));

    frame.render_widget(block, modal_area);

    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    let selected = cursor(state).selected;
    let blue = Style::default().fg(ratatui::style::Color::Rgb(177, 185, 249));
    let dotted = Style::default().fg(ratatui::style::Color::Rgb(80, 80, 80));

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

    // Pad to FIRST_OPTION_ROW
    while lines.len() < FIRST_OPTION_ROW as usize {
        lines.push(Line::from(""));
    }

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
        let modal = modal_rect(area);
        let first = modal.y + 1 + FIRST_OPTION_ROW;
        assert_eq!(click_index(area, modal.x + 3, first), Some(0));
        assert_eq!(click_index(area, modal.x + 3, first + 1), Some(1));
        assert_eq!(click_index(area, modal.x + 3, first + 2), Some(2));
        assert_eq!(click_index(area, modal.x + 3, first + 3), Some(3));
        // Rows above the options and the hint row below are not selectable.
        assert_eq!(click_index(area, modal.x + 3, first - 1), None);
        assert_eq!(click_index(area, modal.x + 3, first + 4), None);
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
        assert_eq!(c.selected, 3);
        assert!(matches!(action_for(c.selected), PermissionAction::Deny));
        c.next();
        assert_eq!(c.selected, 0);
        assert!(matches!(action_for(0), PermissionAction::Allow));
        assert!(matches!(action_for(1), PermissionAction::Allow));
    }
}
