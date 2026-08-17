//! Bottom status bar with model, context usage, cost, and background tasks.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::display::state::AppState;
use crate::display::theme;

/// Render the bottom status bar.
pub fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let mut spans = vec![
        Span::styled("● ", theme::claude()),
        Span::styled(
            "NIKI",
            Style::default()
                .fg(theme::claude())
                .add_modifier(Modifier::BOLD),
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
    spans.push(Span::styled(format!("{} ", state.model), theme::text_dim()));
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
    spans.push(Span::styled(format!("ctx: {}% ", pct), color));

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

    // Mode badge
    let mode_str = match state.input_state.mode {
        crate::display::state::InputMode::Command => "COMMAND",
        crate::display::state::InputMode::Insert => "INSERT",
        crate::display::state::InputMode::Shell => "SHELL",
    };
    let mode_color = match state.input_state.mode {
        crate::display::state::InputMode::Command => theme::text_dim(),
        crate::display::state::InputMode::Insert => theme::primary(),
        crate::display::state::InputMode::Shell => theme::warning(),
    };
    spans.push(Span::styled(" ─── ", theme::border()));
    spans.push(Span::styled(
        mode_str,
        Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
    ));

    // Line / Col
    let (line, col) = state.input_state.line_col();
    spans.push(Span::styled(" ─── ", theme::border()));
    spans.push(Span::styled(
        format!("Ln {}, Col {}", line + 1, col + 1),
        theme::text_dim(),
    ));

    // Typing indicator
    if state.input_state.is_typing(2000) {
        spans.push(Span::styled(" ─── ", theme::border()));
        spans.push(Span::styled("Typing… ", theme::text_dim()));
    }

    // Transient notice (e.g. "Esc — stopping…")
    if let Some((msg, _)) = &state.notice {
        spans.push(Span::styled(format!(" ─── {}", msg), mode_color));
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
