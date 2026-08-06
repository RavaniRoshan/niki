//! Slash command menu overlay.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::display::state::AppState;
use crate::display::theme;

/// Render the slash command menu overlay.
pub fn render_command_menu(frame: &mut Frame, area: Rect, state: &AppState) {
    let menu_width = 50u16.min(area.width.saturating_sub(4));
    let item_height = 1u16;
    let max_visible = 10usize;
    let visible = state.commands.len().min(max_visible);
    let menu_height = (visible as u16) + 3;

    let x = (area.width - menu_width) / 2;
    let y = area.height.saturating_sub(menu_height + 3);

    let modal_area = Rect { x, y, width: menu_width, height: menu_height };

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Commands ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border()))
        .style(Style::default().bg(theme::bg_elevated()));

    frame.render_widget(block, modal_area);

    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: menu_width.saturating_sub(2),
        height: menu_height.saturating_sub(2),
    };

    let mut lines = vec![];

    // Filter commands by current input
    let filter = state.command_filter.trim_start_matches('/');
    let filtered: Vec<_> = state.commands.iter()
        .filter(|c| filter.is_empty() || c.name.contains(filter) || c.description.contains(filter))
        .collect();

    // Show commands
    for (i, cmd) in filtered.iter().enumerate().take(max_visible) {
        let marker = if i == state.command_selected { "●" } else { " " };
        let color = if i == state.command_selected {
            theme::primary()
        } else {
            theme::text()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", marker), color),
            Span::styled(
                format!("{:<16}", cmd.name),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&cmd.description, theme::text_dim()),
        ]));
    }

    // Footer hint
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Type to filter  [Tab] Complete  [Enter] Run",
        theme::text_dim(),
    )));

    frame.render_widget(Paragraph::new(lines), inner)
}

/// Get the selected command from state.
pub fn get_selected_command(state: &AppState) -> Option<String> {
    let filter = state.command_filter.trim_start_matches('/');
    let filtered: Vec<_> = state.commands.iter()
        .filter(|c| filter.is_empty() || c.name.contains(filter) || c.description.contains(filter))
        .collect();

    filtered.get(state.command_selected).map(|c| c.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_selected_command_test() {
        let config = crate::config::NikiConfig::default();
        let mut state = crate::display::state::AppState::new("test".to_string(), config, ".".into());
        state.command_filter = "/".to_string();
        state.command_selected = 0;
        let cmd = get_selected_command(&state);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap(), "/help");
    }

    #[test]
    fn get_selected_command_filtered() {
        let config = crate::config::NikiConfig::default();
        let mut state = crate::display::state::AppState::new("test".to_string(), config, ".".into());
        state.command_filter = "/co".to_string();
        state.command_selected = 0;
        let cmd = get_selected_command(&state);
        assert!(cmd.is_some());
        // /compact and /cost both match
    }
}
