//! Layout system — chat layout and overlay rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::display::pages::chat;
use crate::display::state::AppState;
use crate::display::theme;

/// Render the main chat layout (conversational view).
pub fn render_chat(frame: &mut Frame, area: Rect, state: &AppState) {
    if area.height < 5 {
        return;
    }

    // Multi-line composer: grow the input region up to ~1/3 of the screen when
    // the buffer contains newlines (Shift+Enter), otherwise keep it compact.
    let input_lines = state.input_state.buffer.lines().count().max(1);
    let max_input = ((area.height as usize) / 3).max(3);
    let input_h = (input_lines + 2).min(max_input).max(3) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),          // messages area
            Constraint::Length(input_h), // input box (grows for multi-line)
        ])
        .split(area);

    // Reserve a 1-column scrollbar on the right of the message area.
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[0]);
    let msg_area = body[0];

    // Render messages using the existing build_chat_lines (handles stages,
    // progressive disclosure, chat log). Skip inline input — rendered below.
    let lines = chat::build_chat_lines(state, msg_area.width as usize, false);
    let visible = chunks[0].height as usize;
    state.chat_viewport_h.set(visible);
    let total = lines.len();
    let scroll = state.chat_scroll.view_offset(total, visible);

    // Scroll indicator: show "↑ more" when scrolled up
    let mut display_lines: Vec<Line> = Vec::with_capacity(visible);
    if scroll > 0 {
        let indicator_text = format!("  ↑ {} lines above  ", scroll);
        let indicator_style = Style::default()
            .fg(theme::fg_subtle())
            .add_modifier(ratatui::style::Modifier::ITALIC);
        display_lines.push(Line::from(Span::styled(indicator_text, indicator_style)));
    }

    let visible_lines: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(visible.saturating_sub(display_lines.len()))
        .map(|cl| {
            cl.rich
                .clone()
                .unwrap_or_else(|| Line::from(cl.text.clone()))
        })
        .collect();
    display_lines.extend(visible_lines);
    while display_lines.len() < visible {
        display_lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(display_lines), msg_area);

    // Visible scrollbar (product-gaps P0: "users can't navigate without it").
    if total > visible && visible > 0 {
        let mut sb_state = ScrollbarState::new(total)
            .position(scroll)
            .viewport_content_length(visible);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme::scrollbar_thumb()))
                .track_style(Style::default().fg(theme::text_dim())),
            body[1],
            &mut sb_state,
        );
    }

    // Render input box (Claude Code elevated capsule). Use the multi-line
    // renderer when the composer holds newlines so long prompts stay readable.
    if state.input_state.buffer.contains('\n') {
        super::components::render_input_box_multiline(frame, &state.input_state, chunks[1]);
    } else {
        super::components::render_input_box(frame, state, chunks[1]);
    }
}
