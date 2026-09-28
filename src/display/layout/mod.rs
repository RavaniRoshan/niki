//! Layout system — chat layout and overlay rendering.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::display::pages::chat;
use crate::display::state::AppState;
use crate::display::theme;

/// The chat's two vertical regions: the transcript and the composer.
///
/// One function, because both halves of the code needed this split and
/// computed it separately. The renderer grows the composer with the input —
/// a multi-line draft takes up to a third of the panel — while the click
/// handler assumed a fixed three rows. So the moment you pressed Shift+Enter
/// and the composer actually grew, clicks were resolved against a band that
/// was no longer where the composer was drawn: typing into the composer would
/// have put the cursor somewhere else, and clicking the transcript would have
/// gone to the composer.
///
/// Returns `(messages, composer)`.
pub fn composer_split(area: Rect, input_lines: usize) -> (Rect, Rect) {
    // Multi-line composer: grow the input region up to ~1/3 of the screen when
    // the buffer contains newlines (Shift+Enter), otherwise keep it compact.
    let max_input = ((area.height as usize) / 3).max(3);
    let input_h = (input_lines + 2).min(max_input).max(3) as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),          // messages area
            Constraint::Length(input_h), // input box (grows for multi-line)
        ])
        .split(area);
    (chunks[0], chunks[1])
}

/// Render the main chat layout (conversational view).
pub fn render_chat(frame: &mut Frame, area: Rect, state: &AppState) {
    if area.height < 5 {
        return;
    }

    let input_lines = state.input_state.buffer.lines().count().max(1);
    let (msg_chunk, input_chunk) = composer_split(area, input_lines);

    // Reserve a 1-column scrollbar on the right of the message area.
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(msg_chunk);
    let msg_area = body[0];

    // Render messages using the existing build_chat_lines (handles stages,
    // progressive disclosure, chat log). Skip inline input — rendered below.
    let lines = chat::build_chat_lines(state, msg_area.width as usize, false);
    let visible = msg_chunk.height as usize;
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
        super::components::render_input_box_multiline(frame, &state.input_state, input_chunk);
    } else {
        super::components::render_input_box(frame, state, input_chunk);
    }
}

#[cfg(test)]
mod composer_tests {
    use super::*;

    /// The composer grows with the draft; a click handler that assumed a
    /// fixed three rows put the cursor somewhere else after one Shift+Enter.
    /// This is the property the hardcoded split broke.
    #[test]
    fn the_composer_grows_with_a_multiline_draft() {
        let area = Rect::new(0, 0, 80, 24);
        let (_, one) = composer_split(area, 1);
        let (_, three) = composer_split(area, 3);
        let (_, eight) = composer_split(area, 8);
        assert_eq!(one.height, 3, "a single line is the compact composer");
        assert!(
            three.height > one.height,
            "a three-line draft must take more rows than a one-line draft"
        );
        assert!(eight.height >= three.height, "growth is monotonic");
    }

    #[test]
    fn the_composer_never_takes_more_than_a_third_of_the_panel() {
        for h in [12u16, 24, 40, 60] {
            let area = Rect::new(0, 0, 80, h);
            let (_, composer) = composer_split(area, 500);
            let max_input = ((area.height as usize / 3).max(3)) as u16;
            assert!(
                composer.height <= max_input,
                "at {h} rows a huge draft must not swallow the transcript: got {}, \
                 cap {max_input}",
                composer.height
            );
            assert!(
                area.height.saturating_sub(composer.height) >= 3,
                "the transcript must keep its minimum at {h} rows"
            );
        }
    }

    /// The two regions must tile the panel with no gap and no overlap, at any
    /// draft size — that is what makes a click unambiguous.
    #[test]
    fn the_two_regions_tile_the_panel_at_every_draft_size() {
        for h in [10u16, 12, 24, 40] {
            let area = Rect::new(0, 0, 80, h);
            for lines in [1usize, 2, 3, 5, 9, 40] {
                let (msg, composer) = composer_split(area, lines);
                assert_eq!(msg.y, 0, "messages start at the top ({h}x{lines})");
                assert_eq!(
                    msg.y + msg.height,
                    composer.y,
                    "no gap or overlap at {h}x{lines}"
                );
                assert_eq!(
                    composer.y + composer.height,
                    h,
                    "composer ends the panel at {h}x{lines}"
                );
                assert_eq!(msg.width, area.width);
                assert_eq!(composer.width, area.width);
            }
        }
    }

    /// A one-row panel is degenerate; the split must not underflow.
    #[test]
    fn a_tiny_panel_does_not_underflow() {
        for h in [1u16, 2, 4, 5] {
            let area = Rect::new(0, 0, 80, h);
            let (msg, composer) = composer_split(area, 1);
            assert!(
                msg.y + msg.height <= h && composer.y + composer.height <= h,
                "bands must stay inside a {h}-row panel"
            );
        }
    }
}
