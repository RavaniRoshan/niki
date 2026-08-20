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

    let left_spans = if width >= 80 {
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
        right_spans.push(Span::styled("ctx ", Style::default().fg(theme::fg_dim())));
        right_spans.push(Span::styled(
            "▓".repeat(filled),
            Style::default().fg(theme::thinking_green()),
        ));
        right_spans.push(Span::styled(
            "░".repeat(empty),
            Style::default().fg(theme::fg_dim()),
        ));
        right_spans.push(Span::styled(
            format!(" {}%   ", pct),
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

    // Permission mode indicator — styled badge with distinct color per mode
    let (badge_text, badge_color) = match state.permission_mode {
        crate::display::state::PermissionMode::Default => (" MANUAL ", theme::fg_subtle()),
        crate::display::state::PermissionMode::AcceptEdits => (" EDITS ", theme::success()),
        crate::display::state::PermissionMode::Plan => (" PLAN ", theme::thinking_green()),
        crate::display::state::PermissionMode::Auto => (" AUTO ", theme::thinking_green()),
        crate::display::state::PermissionMode::DontAsk => (" YOLO ", theme::error()),
        crate::display::state::PermissionMode::BypassPermissions => (" BYPASS ", theme::error()),
    };
    right_spans.push(Span::styled(
        format!(" {} ", badge_text),
        Style::default().fg(badge_color).add_modifier(
            ratatui::style::Modifier::BOLD,
        ),
    ));

    if let Some((msg, _)) = &state.notice {
        right_spans.push(Span::styled(
            format!("· {} ", msg),
            Style::default().fg(theme::clay()),
        ));
    }

    let left_len: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();

    // Build the final line greedily so it can never overflow the frame:
    // left shortcuts first, then as many right-aligned extras as fit, then a
    // trailing pad. This guarantees `total <= width` in every branch.
    let mut spans = left_spans;
    let mut used = left_len;
    let mut kept_right = Vec::new();
    for s in &right_spans {
        let n = s.content.chars().count();
        if used + n <= width {
            kept_right.push(s.clone());
            used += n;
        } else {
            break;
        }
    }
    if width > used {
        spans.push(Span::styled(" ".repeat(width - used), Style::default()));
    }
    spans.extend(kept_right);

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
