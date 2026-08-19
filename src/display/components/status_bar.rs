//! Bottom status bar with key shortcuts, model, branch, and transient notices.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::display::state::AppState;
use crate::display::theme;

/// Render the bottom status line.
pub fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let width = area.width as usize;
    if width < 10 || area.height == 0 {
        return;
    }

    let mut left_spans = if width >= 80 {
        vec![
            Span::styled(
                "tab ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("toggle view   ", Style::default().fg(theme::fg_subtle())),
            Span::styled(
                "ctrl-p ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("commands   ", Style::default().fg(theme::fg_subtle())),
            Span::styled(
                "esc ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "quit (run continues)",
                Style::default().fg(theme::fg_subtle()),
            ),
        ]
    } else if width >= 50 {
        vec![
            Span::styled(
                "tab ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("view · ", Style::default().fg(theme::fg_subtle())),
            Span::styled(
                "^p ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("cmd · ", Style::default().fg(theme::fg_subtle())),
            Span::styled(
                "esc ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("exit", Style::default().fg(theme::fg_subtle())),
        ]
    } else {
        vec![
            Span::styled(
                "tab",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("·", Style::default().fg(theme::fg_subtle())),
            Span::styled(
                "^p",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("·", Style::default().fg(theme::fg_subtle())),
            Span::styled(
                "esc",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    };

    // Right-aligned status info
    let mut right_spans = vec![];

    if !state.branch_name.is_empty() && width >= 65 {
        right_spans.push(Span::styled(
            format!("branch {}   ", state.branch_name),
            Style::default().fg(theme::sand()),
        ));
    }

    if state.cost > 0.0 && width >= 45 {
        right_spans.push(Span::styled(
            format!("${:.4}   ", state.cost),
            Style::default().fg(theme::fg_dim()),
        ));
    }

    if state.context_usage > 0.0 && width >= 55 {
        let pct = (state.context_usage * 100.0).round() as u32;
        let filled = (pct / 10).clamp(0, 10) as usize;
        let empty = 10 - filled;
        right_spans.push(Span::styled(
            format!(
                "ctx {}{}{}{} {}%   ",
                theme::thinking_green(),
                "▓".repeat(filled),
                theme::fg_dim(),
                "░".repeat(empty),
                pct
            ),
            Style::default().fg(theme::fg_dim()),
        ));
    }

    if state.input_state.has_queued() && width >= 50 {
        let q = state.input_state.queued.len();
        right_spans.push(Span::styled(
            format!("{} queued   ", q),
            Style::default().fg(theme::clay()),
        ));
    }

    if let Some((msg, _)) = &state.notice {
        right_spans.push(Span::styled(
            format!("· {} ", msg),
            Style::default().fg(theme::clay()),
        ));
    }

    let left_len: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
    let right_len: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();

    if width > left_len + right_len {
        let pad = width - left_len - right_len;
        left_spans.push(Span::styled(" ".repeat(pad), Style::default()));
        left_spans.extend(right_spans);
    }

    frame.render_widget(Paragraph::new(Line::from(left_spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;

    #[test]
    fn status_bar_line_width() {
        let config = NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        assert_eq!(state.model, "claude-sonnet-4-20250514");
    }
}
