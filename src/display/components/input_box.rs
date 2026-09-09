//! Text input box with cursor rendering and pill badges (Claude Code / Studio style).
//!
//! During an active streaming pipeline stage the border turns gray and the
//! inner text is dimmed — matching Claude Code's disabled-input behavior.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::display::state::{AppState, InputMode, InputState};
use crate::display::theme;

/// Render the input box with cursor, mode indicator, and status pill badges.
pub fn render_input_box(frame: &mut Frame, state: &AppState, area: Rect) {
    if area.width < 8 || area.height < 2 {
        return;
    }

    let is_streaming = state.has_running_stage();
    let border_style = if is_streaming {
        Style::default().fg(theme::fg_dim())
    } else {
        Style::default().fg(theme::border_dim())
    };
    let bg_style = if is_streaming {
        Style::default().bg(theme::bg_color())
    } else {
        Style::default().bg(theme::bg_highlight())
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border_style)
        .style(bg_style);

    let inner = input_block.inner(area);
    frame.render_widget(input_block, area);

    let inner_width = inner.width as usize;
    if inner_width < 4 {
        return;
    }

    let mode_indicator = match state.input_state.mode {
        InputMode::Shell => vec![
            Span::styled("▎ ", Style::default().fg(theme::shell())),
            Span::styled(
                "Shell ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        InputMode::Command => vec![
            Span::styled("▎ ", Style::default().fg(theme::clay())),
            Span::styled(
                "Cmd ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        InputMode::Insert => vec![
            Span::styled("▎ ", Style::default().fg(theme::clay())),
            Span::styled(
                "Build ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
        ],
    };

    let mut spans = mode_indicator;
    let mode_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();

    if state.input_state.buffer.is_empty() {
        spans.push(Span::styled(" ", theme::prompt_cursor()));
        let placeholder = if inner_width >= 55 {
            "Describe a change or press / for commands..."
        } else if inner_width >= 35 {
            "Ask NIKI or / for commands..."
        } else {
            "Prompt..."
        };
        spans.push(Span::styled(
            placeholder,
            Style::default().fg(theme::fg_subtle()),
        ));
    } else {
        let avail = inner_width.saturating_sub(mode_len + 1); // room for text + cursor
        let buffer_chars: Vec<char> = state.input_state.buffer.chars().collect();
        // cursor_pos is a byte index; rendering counts characters.
        let cursor = state.input_state.cursor_char_idx().min(buffer_chars.len());

        // Horizontal scroll window calculation
        let (start, end) = if buffer_chars.len() <= avail {
            (0, buffer_chars.len())
        } else if cursor < avail {
            (0, avail)
        } else {
            let s = cursor + 1 - avail;
            (s, (s + avail).min(buffer_chars.len()))
        };

        let before_cursor: String = buffer_chars[start..cursor].iter().collect();
        let cursor_char = buffer_chars.get(cursor).copied();
        let after_cursor: String = if cursor < end {
            let next = cursor + 1;
            if next < end {
                buffer_chars[next..end].iter().collect()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        spans.push(Span::styled(
            before_cursor,
            Style::default().fg(theme::fg_bright()),
        ));
        // Caret blink: block cursor visible ~60% of a ~1s cycle at 60fps
        // ticks; static block when reduced-motion is on.
        let caret_on = crate::display::motion::blink_on(
            state.tick,
            36,
            24,
            crate::display::motion::reduced(state.config.ui.reduced_motion),
        );
        let caret_style = if caret_on {
            theme::prompt_cursor()
        } else {
            Style::default().fg(theme::fg_bright())
        };
        spans.push(Span::styled(
            cursor_char.map_or(" ".to_string(), |c| c.to_string()),
            caret_style,
        ));
        if !after_cursor.is_empty() {
            spans.push(Span::styled(
                after_cursor,
                Style::default().fg(theme::fg_bright()),
            ));
        }
    }

    // Measure total left content length to calculate badge right-alignment
    let content_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let badge_spans = vec![
        Span::raw(" "),
        Span::styled(
            " sandbox ",
            Style::default().fg(theme::fg_dim()).bg(theme::bg_deep()),
        ),
        Span::raw(" "),
        Span::styled(
            " podman ",
            Style::default().fg(theme::fg_dim()).bg(theme::bg_deep()),
        ),
    ];
    let badge_len: usize = badge_spans.iter().map(|s| s.content.chars().count()).sum();

    if inner_width >= content_len + badge_len + 2 {
        let pad = inner_width - content_len - badge_len;
        spans.push(Span::styled(" ".repeat(pad), Style::default()));
        spans.extend(badge_spans);
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// Render a multiline input area (for longer messages).
pub fn render_input_box_multiline(frame: &mut Frame, state: &InputState, area: Rect) {
    let mut lines = vec![];

    // Top border
    lines.push(Line::from(Span::styled(
        format!("╭{}╮", "─".repeat(area.width.saturating_sub(2) as usize)),
        Style::default().fg(theme::border_dim()),
    )));

    // Input lines
    let input_text = &state.buffer;
    for input_line in input_text.lines() {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(theme::border_dim())),
            Span::styled(input_line, Style::default().fg(theme::fg_bright())),
        ]));
    }

    // Cursor line if buffer is empty
    if input_text.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(theme::border_dim())),
            Span::styled(" ", theme::prompt_cursor()),
            Span::styled(
                "Describe a change or press / for commands...",
                Style::default().fg(theme::fg_subtle()),
            ),
        ]));
    }

    // Bottom border
    lines.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(area.width.saturating_sub(2) as usize)),
        Style::default().fg(theme::border_dim()),
    )));

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::bg_highlight())),
        area,
    );
}

/// Handle a mouse click inside the input box, positioning the cursor.
/// Returns true if the click was handled.
pub fn handle_click(state: &mut AppState, mouse_col: u16, area: Rect) -> bool {
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .inner(area);

    if mouse_col < inner.x || mouse_col >= inner.x + inner.width {
        return false;
    }

    let col_in_inner = (mouse_col - inner.x) as usize;
    let inner_width = inner.width as usize;

    // Calculate mode indicator length (same as render: "▎ " + label + " ").
    let mode_len = match state.input_state.mode {
        InputMode::Shell => 8,   // "▎ Shell "
        InputMode::Command => 6, // "▎ Cmd "
        InputMode::Insert => 8,  // "▎ Build "
    };

    if col_in_inner < mode_len {
        return false;
    }

    let text_col = col_in_inner - mode_len;
    let buffer_chars: Vec<char> = state.input_state.buffer.chars().collect();
    let avail = inner_width.saturating_sub(mode_len + 1);

    // Calculate the scroll window (same logic as render)
    // cursor_pos is a byte index; hit-testing counts characters.
    let cursor = state.input_state.cursor_char_idx().min(buffer_chars.len());
    let (start, _end) = if buffer_chars.len() <= avail {
        (0, buffer_chars.len())
    } else if cursor < avail {
        (0, avail)
    } else {
        let s = cursor + 1 - avail;
        (s, (s + avail).min(buffer_chars.len()))
    };

    // Map click position to buffer position
    let new_cursor = (start + text_col).min(buffer_chars.len());

    // Check for double-click (select word)
    let now = std::time::Instant::now();
    let is_double_click = if let Some(last_time) = state.last_click_time {
        let elapsed = now.duration_since(last_time).as_millis();
        elapsed < 500 // Double-click within 500ms
    } else {
        false
    };

    if is_double_click {
        // Move cursor to start of word at click position
        let word_start = find_word_start(&buffer_chars, new_cursor);
        state.input_state.set_cursor_char_idx(word_start);
    } else {
        state.input_state.set_cursor_char_idx(new_cursor);
    }

    state.last_click_time = Some(now);
    state.last_click_pos = Some((mouse_col, 0));
    true
}

/// Find the start of the word at the given position.
fn find_word_start(chars: &[char], pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut i = pos;
    // Skip non-word characters
    while i > 0 && !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_' {
        i -= 1;
    }
    // Skip word characters
    while i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;
    use ratatui::layout::Rect;

    fn app_state() -> AppState {
        AppState::new("test".to_string(), NikiConfig::default(), ".".into())
    }

    #[test]
    fn input_state_cursor_at_end() {
        let mut state = InputState::new();
        state.insert_char('a');
        state.insert_char('b');
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn handle_click_maps_char_column_to_byte_cursor() {
        // "aé日x": char idx 0..4 ↔ byte offsets 0,1,3,6,7.
        let mut state = app_state();
        state.input_state.insert_str("aé日x");
        let area = Rect::new(0, 0, 40, 5);
        // inner.x = 1 (border), mode prefix = 8 ("▎ Build "): char idx 3
        // ('x') sits at column 1 + 8 + 3.
        assert!(handle_click(&mut state, 1 + 8 + 3, area));
        assert_eq!(state.input_state.cursor_pos, 6);
        assert_eq!(state.input_state.cursor_char_idx(), 3);
        // Clicking char idx 1 ('é') must land on its byte start, not mid-char.
        // (Reset click time: two rapid clicks would take the double-click path.)
        state.last_click_time = None;
        assert!(handle_click(&mut state, 1 + 8 + 1, area));
        assert_eq!(state.input_state.cursor_pos, 1);
    }

    #[test]
    fn input_state_mode_switch() {
        let mut state = InputState::new();
        state.mode = InputMode::Command;
        assert_eq!(state.mode, InputMode::Command);
    }
}
