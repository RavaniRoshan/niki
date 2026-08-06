//! Text input box with cursor rendering.

use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::display::state::{InputMode, InputState};
use crate::display::theme;

/// Render the input box with cursor.
pub fn render_input_box(frame: &mut Frame, state: &InputState, area: Rect) {
    let prompt = match state.mode {
        InputMode::Shell => Span::styled("! ", theme::shell()),
        _ => Span::styled("> ", theme::primary()),
    };

    let before_cursor = &state.buffer[..state.cursor_pos.min(state.buffer.len())];
    let cursor_char = state.buffer[state.cursor_pos..].chars().next();
    let after_cursor_start = state.cursor_pos + cursor_char.map_or(0, |c| c.len_utf8());
    let after_cursor = &state.buffer[after_cursor_start.min(state.buffer.len())..];

    let mut spans = vec![prompt];
    spans.push(Span::styled(before_cursor.to_string(), theme::text()));
    spans.push(Span::styled(
        cursor_char.map_or(" ".to_string(), |c| c.to_string()),
        theme::prompt_cursor(),
    ));
    spans.push(Span::styled(after_cursor.to_string(), theme::text()));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render a multiline input area (for longer messages).
pub fn render_input_box_multiline(frame: &mut Frame, state: &InputState, area: Rect) {
    let mut lines = vec![];

    // Top border
    lines.push(Line::from(Span::styled(
        format!("╭{}╮", "─".repeat(area.width.saturating_sub(2) as usize)),
        theme::border(),
    )));

    // Input lines
    let input_text = &state.buffer;
    for input_line in input_text.lines() {
        lines.push(Line::from(vec![
            Span::styled("│ ", theme::border()),
            Span::styled(input_line, theme::text()),
        ]));
    }

    // Cursor line if buffer is empty
    if input_text.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("│ ", theme::border()),
            Span::styled(" ", theme::prompt_cursor()),
        ]));
    }

    // Bottom border
    lines.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(area.width.saturating_sub(2) as usize)),
        theme::border(),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_cursor_at_end() {
        let mut state = InputState::new();
        state.insert_char('a');
        state.insert_char('b');
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn input_state_mode_switch() {
        let mut state = InputState::new();
        state.mode = InputMode::Command;
        assert_eq!(state.mode, InputMode::Command);
    }
}
