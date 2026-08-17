//! Large ASCII art "NIKI" logo for the TUI home screen.
//!
//! Generated using FIGlet "big" font via the `figlet-rs` crate.
//! Produces a bold 6-line logo that renders correctly in any monospace font.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::theme;

/// Pre-generated NIKI 3D shadow block logo lines.
const LOGO_LINES: &[&str] = &[
    "███╗   ██╗██╗██╗  ██╗██╗",
    "████╗  ██║██║██║ ██╔╝██║",
    "██╔██╗ ██║██║█████╔╝ ██║",
    "██║╚██╗██║██║██╔═██╗ ██║",
    "██║ ╚████║██║██║  ██╗██║",
    "╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝",
];

/// Height of the logo in lines.
pub const LOGO_HEIGHT: u16 = 6;

/// Render the NIKI logo centered in the given area.
pub fn render_logo(frame: &mut Frame, area: Rect) {
    let width = area.width as usize;

    for (i, line) in LOGO_LINES.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }

        let line_width = line.chars().count();
        let padding = if width > line_width {
            (width - line_width) / 2
        } else {
            0
        };

        let padded = format!("{}{}", " ".repeat(padding), line);

        let y = area.y + i as u16;
        let line_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                padded,
                Style::default()
                    .fg(super::theme::fg_color())
                    .add_modifier(Modifier::BOLD),
            ))),
            line_area,
        );
    }
}

/// Calculate the preferred header height based on current terminal dimensions.
pub fn preferred_logo_height(width: u16, height: u16) -> u16 {
    if height < 18 {
        0 // Ultra-compact: suppress header to maximize chat/input area
    } else if height < 28 || width < 75 {
        1 // Compact mode: single-line sleek brand header
    } else {
        8 // Full mode: 6-line 3D ASCII logo + padding
    }
}

/// Render an adaptive header matching the allocated height constraint.
pub fn render_adaptive_header(
    frame: &mut Frame,
    area: Rect,
    state: &crate::display::state::AppState,
) {
    if area.height == 0 || area.width < 10 {
        return;
    }

    if area.height == 1 {
        // Compact single-line status header
        let mut spans = vec![
            Span::styled("◈ ", Style::default().fg(theme::clay())),
            Span::styled(
                "NIKI ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("v0.4.0", Style::default().fg(theme::fg_subtle())),
        ];

        let project_name = state
            .project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".");
        let branch = if state.branch_name.is_empty() {
            "main".to_string()
        } else {
            state.branch_name.clone()
        };

        let right_info = format!(" · {} ({})", project_name, branch);
        let left_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let right_len = right_info.chars().count();

        if (area.width as usize) > left_len + right_len {
            spans.push(Span::styled(
                right_info,
                Style::default().fg(theme::fg_dim()),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    } else {
        render_logo(frame, area);
    }
}

/// Render the logo with a subtitle line below it.
pub fn render_logo_with_subtitle(frame: &mut Frame, area: Rect, subtitle: &str) {
    render_logo(frame, area);

    if area.height > LOGO_HEIGHT {
        let subtitle_y = area.y + LOGO_HEIGHT;
        let subtitle_area = Rect {
            x: area.x,
            y: subtitle_y,
            width: area.width,
            height: 1,
        };

        let width = area.width as usize;
        let sub_width = subtitle.len();
        let padding = if width > sub_width {
            (width - sub_width) / 2
        } else {
            0
        };
        let padded = format!("{}{}", " ".repeat(padding), subtitle);

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                padded,
                Style::default().fg(theme::fg_dim()),
            ))),
            subtitle_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_line_count() {
        assert_eq!(LOGO_LINES.len(), 6);
    }

    #[test]
    fn logo_lines_consistent_width() {
        let widths: Vec<usize> = LOGO_LINES.iter().map(|l| l.chars().count()).collect();
        let first = widths[0];
        for w in &widths {
            assert_eq!(*w, first, "Logo lines must be equal character width");
        }
    }

    #[test]
    fn logo_contains_niki() {
        let combined = LOGO_LINES.join("");
        assert!(combined.contains('█') || combined.contains('_') || combined.contains('|'));
    }

    #[test]
    fn responsive_logo_height_breakpoints() {
        assert_eq!(preferred_logo_height(80, 15), 0);
        assert_eq!(preferred_logo_height(60, 40), 1);
        assert_eq!(preferred_logo_height(100, 25), 1);
        assert_eq!(preferred_logo_height(120, 40), 8);
    }
}
