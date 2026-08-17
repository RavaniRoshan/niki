//! Slash command menu overlay.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::components::list_cursor::ListCursor;
use crate::display::state::{AppState, Command};
use crate::display::theme;

/// Maximum number of command rows shown at once.
const MAX_VISIBLE: usize = 10;

/// Commands matching the current filter, in row order.
fn filtered_commands(state: &AppState) -> Vec<&Command> {
    let filter = state.command_filter.trim_start_matches('/');
    state
        .commands
        .iter()
        .filter(|c| filter.is_empty() || c.name.contains(filter) || c.description.contains(filter))
        .collect()
}

/// Number of selectable rows currently on screen.
pub fn filtered_count(state: &AppState) -> usize {
    filtered_commands(state).len().min(MAX_VISIBLE)
}

/// The shared [`ListCursor`] over the visible command rows.
pub fn cursor(state: &AppState) -> ListCursor {
    ListCursor::with_selected(filtered_count(state), state.command_selected)
}

/// Render the slash command menu overlay.
pub fn render_command_menu(frame: &mut Frame, area: Rect, state: &AppState) {
    let menu_width = 50u16.min(area.width.saturating_sub(4));
    let _item_height = 1u16;
    let max_visible = MAX_VISIBLE;
    let visible = state.commands.len().min(max_visible);
    let menu_height = (visible as u16) + 3;

    let x = (area.width - menu_width) / 2;
    let y = area.height.saturating_sub(menu_height + 3);

    let modal_area = Rect {
        x,
        y,
        width: menu_width,
        height: menu_height,
    };

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
    let filtered = filtered_commands(state);

    // Show commands
    for (i, cmd) in filtered.iter().enumerate().take(max_visible) {
        let marker = if i == state.command_selected {
            "●"
        } else {
            " "
        };
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

/// Hit-test a mouse position against the command menu, returning the row index.
pub fn click_index(state: &AppState, area: Rect, x: u16, y: u16) -> Option<usize> {
    let menu_width = 50u16.min(area.width.saturating_sub(4));
    let visible = state.commands.len().min(MAX_VISIBLE);
    let menu_height = (visible as u16) + 3;
    let mx = (area.width - menu_width) / 2;
    let my = area.height.saturating_sub(menu_height + 3);
    let inner_left = mx + 1;
    let inner_right = mx + menu_width - 1;
    let inner_top = my + 1;
    if x >= inner_left && x < inner_right && y >= inner_top {
        let idx = (y - inner_top) as usize;
        if idx < filtered_count(state) {
            return Some(idx);
        }
    }
    None
}

/// Get the selected command from state.
pub fn get_selected_command(state: &AppState) -> Option<String> {
    let filtered = filtered_commands(state);
    filtered.get(state.command_selected).map(|c| c.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_selected_command_test() {
        let config = crate::config::NikiConfig::default();
        let mut state =
            crate::display::state::AppState::new("test".to_string(), config, ".".into());
        state.command_filter = "/".to_string();
        state.command_selected = 0;
        let cmd = get_selected_command(&state);
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap(), "/help");
    }

    #[test]
    fn get_selected_command_filtered() {
        let config = crate::config::NikiConfig::default();
        let mut state =
            crate::display::state::AppState::new("test".to_string(), config, ".".into());
        state.command_filter = "/co".to_string();
        state.command_selected = 0;
        let cmd = get_selected_command(&state);
        assert!(cmd.is_some());
        // /compact and /cost both match
    }

    #[test]
    fn cursor_tracks_filtered_rows() {
        let config = crate::config::NikiConfig::default();
        let mut state =
            crate::display::state::AppState::new("test".to_string(), config, ".".into());
        state.command_filter = "/".to_string();
        let count = filtered_count(&state);
        assert!(count > 1);
        assert!(count <= MAX_VISIBLE);

        state.command_selected = 0;
        let mut c = cursor(&state);
        c.prev();
        assert_eq!(c.selected, count - 1, "Up from the top row wraps");
        c.next();
        assert_eq!(c.selected, 0, "Down from the last row wraps");
    }

    #[test]
    fn click_index_rejects_rows_past_the_list() {
        let config = crate::config::NikiConfig::default();
        let mut state =
            crate::display::state::AppState::new("test".to_string(), config, ".".into());
        // A filter that matches nothing → no clickable rows.
        state.command_filter = "/zzzznotacommand".to_string();
        let area = Rect::new(0, 0, 100, 40);
        assert_eq!(filtered_count(&state), 0);
        for row in 0..40u16 {
            assert_eq!(click_index(&state, area, 30, row), None);
        }
    }
}
