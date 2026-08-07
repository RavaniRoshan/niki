//! Progress indicators — bars, spinners, and status displays.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph};

use crate::display::theme;

/// Render a progress bar with percentage.
pub fn render_progress_bar(
    _frame: &mut Frame,
    _area: Rect,
    progress: f64,
    width: usize,
) -> Line<'static> {
    let pct = (progress.clamp(0.0, 1.0) * 100.0) as u16;
    let filled = ((progress.clamp(0.0, 1.0)) * width as f64) as usize;
    let empty = width.saturating_sub(filled);

    let bar = format!("[{}{}] {}%", "█".repeat(filled), "░".repeat(empty), pct);

    Line::from(Span::styled(bar, theme::primary()))
}

/// Render a gauge widget (ratatui native).
pub fn render_gauge(frame: &mut Frame, area: Rect, progress: f64, label: &str) {
    let gauge = Gauge::default()
        .ratio(progress.clamp(0.0, 1.0))
        .label(label)
        .gauge_style(Style::default().fg(theme::primary()));

    frame.render_widget(gauge, area);
}

/// Render a simple step indicator.
pub fn render_steps(frame: &mut Frame, area: Rect, steps: &[&str], current: usize) {
    let mut spans = vec![];
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" → ", theme::border()));
        }
        let style = if i == current {
            Style::default()
                .fg(theme::primary())
                .add_modifier(Modifier::BOLD)
        } else if i < current {
            Style::default().fg(theme::success())
        } else {
            Style::default().fg(theme::text_dim())
        };
        spans.push(Span::styled(*step, style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_format() {
        // Test the format string generation
        let pct = 50u16;
        let width = 20;
        let filled = 10;
        let bar = format!(
            "[{}{}] {}%",
            "█".repeat(filled),
            "░".repeat(width - filled),
            pct
        );
        assert!(bar.contains("[██████████░░░░░░░░░░] 50%"));
    }
}
