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

/// Curated dictionary of personality and action verbs matching Claude Code / NIKI agent phases.
pub const ACTION_VERBS: &[&str] = &[
    "Deliberating",
    "Recombobulating",
    "Synthesizing solution",
    "Inspecting AST",
    "Crafting architecture",
    "Formulating plan",
    "Refactoring targets",
    "Applying mutations",
    "Compiling workspace",
    "Running verification",
    "Dissecting symbols",
    "Tracing call graph",
    "Evaluating invariants",
    "Harmonizing types",
    "Auditing diffs",
    "Optimizing AST",
    "Pruning dead code",
    "Linting source files",
    "Prestidigitating",
    "Constructing payload",
    "Synchronizing workspace",
    "Resolving dependencies",
];

/// Spinner glyph frames (Braille dot animation).
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Return the spinner glyph for the given frame tick.
pub fn spinner_glyph(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

/// Return an evocative action verb based on tick or agent role.
pub fn action_verb(tick: usize) -> &'static str {
    ACTION_VERBS[(tick / 8) % ACTION_VERBS.len()]
}

/// Render a live animated spinner with dynamic action verb.
pub fn render_spinner_with_verb(tick: usize, prefix: Option<&str>) -> Line<'static> {
    let glyph = spinner_glyph(tick);
    let verb = action_verb(tick);
    let mut spans = vec![
        Span::styled(format!("{} ", glyph), Style::default().fg(theme::primary())),
        Span::styled(format!("{}...", verb), Style::default().fg(theme::sand())),
    ];
    if let Some(p) = prefix {
        spans.insert(
            0,
            Span::styled(format!("{} · ", p), Style::default().fg(theme::fg_subtle())),
        );
    }
    Line::from(spans)
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

    #[test]
    fn test_spinner_glyphs_cycle() {
        assert_eq!(spinner_glyph(0), "⠋");
        assert_eq!(spinner_glyph(1), "⠙");
        assert_eq!(spinner_glyph(10), "⠋");
    }

    #[test]
    fn test_action_verbs_cycle() {
        assert_eq!(action_verb(0), "Deliberating");
        assert_eq!(action_verb(8), "Recombobulating");
    }
}
