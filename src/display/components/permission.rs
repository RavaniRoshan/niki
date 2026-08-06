//! Permission request modal overlay.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::display::state::{AppState, PermissionRequest};
use crate::display::theme;

/// Render the permission modal overlay.
pub fn render_permission_modal(frame: &mut Frame, request: &PermissionRequest, area: Rect, state: &AppState) {
    let modal_width = 60u16.min(area.width.saturating_sub(4));
    let modal_height = 11u16;

    let x = (area.width - modal_width) / 2;
    let y = (area.height - modal_height) / 2;

    let modal_area = Rect { x, y, width: modal_width, height: modal_height };

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

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("The agent wants to run:", theme::text_dim())),
        Line::from(""),
        Line::from(Span::styled(
            format!("  $ {}", request.command),
            Style::default().fg(theme::primary()).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            render_permission_options(state.permission_selected),
            theme::text(),
        )),
        Line::from(""),
        Line::from(Span::styled("[Enter] Confirm  [Esc] Deny", theme::text_dim())),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Render permission options with selected marker.
fn render_permission_options(selected: usize) -> String {
    let options = ["Allow once", "Allow always", "Deny"];
    options
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
}
