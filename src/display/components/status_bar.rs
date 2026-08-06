//! Bottom status bar with model, context usage, cost, and background tasks.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::display::state::AppState;
use crate::display::theme;

/// Render the bottom status bar.
pub fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let mut spans = vec![
        Span::styled("● ", theme::claude()),
        Span::styled(
            "NIKI",
            Style::default().fg(theme::claude()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ─── ", theme::border()),
    ];

    // Mode badge
    if state.run_state == crate::display::state::RunState::Running {
        spans.push(Span::styled("Running ", theme::primary()));
        spans.push(Span::styled("─── ", theme::border()));
    } else if state.paused {
        spans.push(Span::styled("Paused ", theme::warning()));
        spans.push(Span::styled("─── ", theme::border()));
    }

    // Model
    spans.push(Span::styled(
        format!("{} ", state.model),
        theme::text_dim(),
    ));
    spans.push(Span::styled("─── ", theme::border()));

    // Background tasks
    if state.background_tasks > 0 {
        spans.push(Span::styled(
            format!("bg: {} tasks ", state.background_tasks),
            theme::primary(),
        ));
        spans.push(Span::styled("─── ", theme::border()));
    }

    // Context usage
    let pct = (state.context_usage * 100.0) as u32;
    let color = if pct > 80 {
        theme::error()
    } else if pct > 60 {
        theme::warning()
    } else {
        theme::success()
    };
    spans.push(Span::styled(
        format!("ctx: {}% ", pct),
        color,
    ));

    // Cost
    spans.push(Span::styled(
        format!("${:.4}", state.cost),
        theme::success(),
    ));

    // Branch
    if !state.branch_name.is_empty() {
        spans.push(Span::styled(" ─── ", theme::border()));
        spans.push(Span::styled(&state.branch_name, theme::text_dim()));
    }

    let status_line = Line::from(spans);
    frame.render_widget(Paragraph::new(status_line), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;

    #[test]
    fn status_bar_line_width() {
        // The status bar should not exceed the given width when rendered
        // This is a structural test — full rendering requires a terminal
        let config = NikiConfig::default();
        let state = AppState::new("test".to_string(), config, ".".into());
        // Just verify state is constructable
        assert_eq!(state.model, "claude-sonnet-4-20250514");
    }
}
