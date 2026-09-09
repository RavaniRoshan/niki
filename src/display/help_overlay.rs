//! Which-key style keybinding overlay (TUI-003: rows generated from the
//! central [`KeyBindings`](super::keybindings::KeyBindings) table so help
//! always matches behavior, including user overrides).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::keybindings::KeyBindings;
use crate::display::theme;

/// Render the centered keybinding overlay.
/// `overridden` marks user-rebound rows with `*`; `conflicts` appends a
/// warning footer instead of failing.
pub fn render_help_overlay(
    frame: &mut Frame,
    area: Rect,
    bindings: &KeyBindings,
    overridden: &[String],
    conflicts: usize,
) {
    let rows = bindings.help_rows(overridden);
    let height =
        (rows.len() as u16 + 4 + u16::from(conflicts > 0)).min(area.height.saturating_sub(2));
    let width = (area.width.saturating_sub(4)).min(56);
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
    for (key, desc, is_overridden) in &rows {
        let label = if *is_overridden {
            format!("{key} *")
        } else {
            key.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{label:<12}"),
                Style::default()
                    .fg(theme::primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(theme::fg_color())),
        ]));
    }
    if conflicts > 0 {
        lines.push(Line::from(vec![Span::styled(
            format!("⚠ {conflicts} keybinding conflict(s): table order wins"),
            Style::default().fg(theme::warning()),
        )]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), inner[0]);

    let close_key = rows.first().map(|(l, _, _)| l.clone()).unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("press {close_key} or Esc to close"),
            Style::default().fg(theme::fg_dim()),
        )),
        inner[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;
    use crate::display::state::AppState;

    fn buffer_text(state: &AppState) -> String {
        let backend = ratatui::backend::TestBackend::new(80, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render_help_overlay(
                    f,
                    f.area(),
                    &state.keybindings,
                    &state.keybinding_overrides,
                    state.keybinding_conflicts.len(),
                )
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    #[test]
    fn help_shows_defaults() {
        let state = AppState::new("t".to_string(), NikiConfig::default(), ".".into());
        let text = buffer_text(&state);
        assert!(text.contains("Ctrl+P"), "{text}");
        assert!(text.contains("command palette"), "{text}");
        assert!(!text.contains('⚠'), "{text}");
    }

    #[test]
    fn help_marks_overrides_and_conflicts() {
        let mut config = NikiConfig::default();
        config
            .ui
            .keybindings
            .insert("command_palette".to_string(), vec!["ctrl+k".to_string()]);
        config
            .ui
            .keybindings
            .insert("cycle_theme".to_string(), vec!["ctrl+k".to_string()]);
        let state = AppState::new("t".to_string(), config, ".".into());
        assert_eq!(state.keybinding_conflicts.len(), 1);
        let text = buffer_text(&state);
        assert!(text.contains("Ctrl+K *"), "{text}");
        assert!(text.contains("conflict(s)"), "{text}");
    }
}
