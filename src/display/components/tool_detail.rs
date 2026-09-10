//! Full-screen tool output modal.
//!
//! Triggered when a user presses Enter on a tool card whose output is
//! truncated. Shows the complete output with syntax highlighting cues and
//! a copy-all action.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::display::components::tool_card::ToolCard;
use crate::display::scroll::ScrollState;
use crate::display::state::AppState;
use crate::display::theme;

/// Geometry: centered modal, 80% width, 70% height.
pub fn modal_rect(area: Rect) -> Rect {
    let w = (area.width as f32 * 0.8) as u16;
    let h = (area.height as f32 * 0.7) as u16;
    Rect {
        x: (area.width - w) / 2,
        y: (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Scrollable body lines of the open card (TUI-010 chaining input).
pub fn detail_content_lines(card: &ToolCard) -> usize {
    card.output
        .as_deref()
        .map(|o| o.lines().count().max(1))
        .unwrap_or(1)
}

/// Visible body rows of the modal on `area` (mirrors the render geometry:
/// outer borders + one footer row are chrome).
pub fn detail_viewport(area: Rect) -> usize {
    modal_rect(area).height.saturating_sub(2 + 1) as usize
}

/// Route a left-click while the detail modal is open (TUI-013). Clicks
/// inside the modal are consumed (no-op — scrolling uses the wheel);
/// outside clicks dismiss. Returns true when the click was consumed.
pub fn route_click(state: &mut AppState, x: u16, y: u16, area: Rect) -> bool {
    let modal = modal_rect(area);
    let inside =
        x >= modal.x && x < modal.x + modal.width && y >= modal.y && y < modal.y + modal.height;
    if !inside {
        state.tool_detail_index = None;
        state.tool_detail_scroll = ScrollState::new();
    }
    true
}

/// Render the tool output modal.
///
/// Layout (top → bottom):
/// - Header: tool name + summary line + copy hint
/// - Divider
/// - Scrollable output body (wrapped)
/// - Footer: Esc to close · y to copy
pub fn render_tool_detail(
    frame: &mut ratatui::Frame,
    card: &ToolCard,
    area: Rect,
    scroll_offset: usize,
) {
    let modal = modal_rect(area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border_active()))
        .style(Style::default().bg(theme::bg_elevated()))
        .title(format!(" {} · {} ", card.tool_name, card.summary));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),    // body
            Constraint::Length(1), // footer
        ])
        .split(inner);

    // ── Output body ───────────────────────────────────────────────────
    let body_text = card.output.as_deref().unwrap_or("(no output)");
    let body_lines: Vec<Line> = body_text
        .lines()
        .skip(scroll_offset)
        .take(chunks[0].height as usize)
        .map(|line| {
            let style = Style::default().fg(theme::fg_color());
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let body = Paragraph::new(body_lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(theme::bg_elevated()));
    frame.render_widget(body, chunks[0]);

    // ── Footer ────────────────────────────────────────────────────────
    let footer_text = format!(
        " ↑/↓ scroll · y copy · Esc close · {} ",
        card.timing().unwrap_or_default()
    );
    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(theme::fg_subtle()),
    )));
    frame.render_widget(footer, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::components::tool_card::{ToolCard, ToolStatus};

    #[test]
    fn modal_rect_centers() {
        let area = Rect::new(0, 0, 100, 50);
        let modal = modal_rect(area);
        assert!(modal.x > 0);
        assert!(modal.y > 0);
        assert!(modal.x + modal.width <= area.width);
        assert!(modal.y + modal.height <= area.height);
    }

    #[test]
    fn modal_rect_clamps_on_small_terminals() {
        let area = Rect::new(0, 0, 20, 10);
        let modal = modal_rect(area);
        assert!(modal.width <= area.width);
        assert!(modal.height <= area.height);
    }

    #[test]
    fn tool_card_timing_empty_for_pending() {
        let card = ToolCard::new("Bash", "test");
        assert!(card.timing().is_none());
    }

    #[test]
    fn tool_card_timing_shows_for_success() {
        let mut card = ToolCard::new("Bash", "test");
        card.set_success(None, 42);
        assert_eq!(card.timing(), Some("42ms".to_string()));
    }

    #[test]
    fn tool_status_equality() {
        assert_eq!(ToolStatus::Pending, ToolStatus::Pending);
        assert_ne!(ToolStatus::Pending, ToolStatus::Running { elapsed_ms: 0 });
    }

    #[test]
    fn tool_card_clone() {
        let card = ToolCard::new("Read", "src/main.rs");
        let cloned = card.clone();
        assert_eq!(card.tool_name, cloned.tool_name);
        assert_eq!(card.summary, cloned.summary);
    }

    #[test]
    fn detail_viewport_matches_render_geometry() {
        // 100x20 area → modal 80x14 → inner 12 rows → 11 body rows.
        let viewport = detail_viewport(Rect::new(0, 0, 100, 20));
        assert_eq!(viewport, 11);
    }

    #[test]
    fn detail_modal_paints_scrolled_slice() {
        use crate::display::scroll::ScrollState;
        let mut card = ToolCard::new("Bash", "ls");
        let output = (0..20)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        card.set_success(Some(output), 10);
        assert_eq!(detail_content_lines(&card), 20);

        let area = Rect::new(0, 0, 100, 20);
        let paint = |offset: usize| -> String {
            let backend = ratatui::backend::TestBackend::new(100, 20);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render_tool_detail(f, &card, area, offset))
                .unwrap();
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol().to_string())
                .collect()
        };
        let top = paint(0);
        assert!(top.contains("line0"), "{top}");
        assert!(top.contains("Bash"), "{top}");
        let scrolled = paint(5);
        assert!(!scrolled.contains("line0"), "{scrolled}");
        assert!(scrolled.contains("line5"), "{scrolled}");

        // ScrollState drives the offset the render call receives.
        let mut scroll = ScrollState::new();
        scroll.jump_to(5, detail_content_lines(&card), detail_viewport(area));
        assert_eq!(
            scroll.view_offset(detail_content_lines(&card), detail_viewport(area)),
            5
        );
    }

    #[test]
    fn route_click_consumes_inside_dismisses_outside() {
        use crate::config::NikiConfig;
        use crate::display::state::AppState;
        let mut state = AppState::new("t".to_string(), NikiConfig::default(), ".".into());
        state.tool_detail_index = Some(0);
        let area = Rect::new(0, 0, 100, 30);
        let modal = modal_rect(area);
        // Inside: consumed, modal stays open.
        assert!(route_click(&mut state, modal.x + 2, modal.y + 2, area));
        assert_eq!(state.tool_detail_index, Some(0));
        // Outside: consumed + dismissed.
        assert!(route_click(&mut state, 0, 0, area));
        assert_eq!(state.tool_detail_index, None);
    }
}
