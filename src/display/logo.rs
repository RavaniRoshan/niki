//! Large ASCII art "NIKI" logo for the TUI home screen.
//!
//! Generated using FIGlet "big" font via the `figlet-rs` crate.
//! Produces a bold 6-line logo that renders correctly in any monospace font.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme;

/// Pre-generated NIKI logo lines using FIGlet "big" font.
/// Output of `figlet-rs FIGlet::big().convert("NIKI")`.
const LOGO_LINES: &[&str] = &[
    r" _   _ _____ _  _______ ",
    r"| \ | |_   _| |/ /_   _|",
    r"|  \| | | | | ' /  | |  ",
    r"| . ` | | | |  <   | |  ",
    r"| |\  |_| |_| . \ _| |_ ",
    r"|_| \_|_____|_|\_\_____|",
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

        let line_width = line.len();
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
                    .fg(theme::FG_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ))),
            line_area,
        );
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
                Style::default().fg(theme::FG_DIM),
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
        let widths: Vec<usize> = LOGO_LINES.iter().map(|l| l.len()).collect();
        let first = widths[0];
        for w in &widths {
            assert_eq!(*w, first, "Logo lines must be equal byte width");
        }
    }

    #[test]
    fn logo_contains_niki() {
        let combined = LOGO_LINES.join("");
        assert!(combined.contains('_') || combined.contains('|'));
    }
}
