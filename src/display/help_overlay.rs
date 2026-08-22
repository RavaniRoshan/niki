//! Which-key style keybinding overlay.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::theme;

/// (Key, description) pairs shown in the overlay.
const BINDINGS: &[(&str, &str)] = &[
    ("Enter", "Submit input / run command"),
    ("Esc", "Close menu, modal, or this overlay"),
    ("Ctrl+C", "Cancel running stage / exit"),
    ("Ctrl+L", "Clear the screen"),
    ("Ctrl+P", "Open the command palette"),
    ("Ctrl+T", "Cycle theme (dark → light → auto)"),
    ("Ctrl+E", "Toggle mouse capture (text selection)"),
    ("Tab", "Autocomplete / switch chat ↔ page"),
    ("↑ / ↓", "History navigation / menu navigation"),
    ("Ctrl+A / E", "Jump to line start / end"),
    ("Ctrl+W", "Delete word backward"),
    ("Ctrl+U / K", "Delete to line start / end"),
    ("@ / / / !", "File / slash-command / shell autocomplete"),
    ("? ", "Toggle this keybinding help"),
];

/// Render the centered keybinding overlay.
pub fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let width = (area.width.saturating_sub(4)).min(56);
    let height = (BINDINGS.len() as u16 + 4).min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border_color()))
        .title(Span::styled(
            " Keybindings ",
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD),
        ));

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(popup);

    let mut lines: Vec<Line> = Vec::new();
    for (key, desc) in BINDINGS {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<12}", key),
                Style::default()
                    .fg(theme::primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(theme::fg_color())),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), inner[0]);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "press ? or Esc to close",
            Style::default().fg(theme::fg_dim()),
        )),
        inner[1],
    );
}
