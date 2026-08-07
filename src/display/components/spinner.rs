//! Animated spinner with moon loader pattern.

use std::time::Instant;

use ratatui::style::Style;
use ratatui::text::Span;

use crate::display::theme;

/// Spinner frame patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinnerStyle {
    Moon,
    Dots,
    Bars,
    Arrow,
}

impl SpinnerStyle {
    fn frames(&self) -> &'static [&'static str] {
        match self {
            SpinnerStyle::Moon => &["◐", "◓", "◑", "◒"],
            SpinnerStyle::Dots => &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            SpinnerStyle::Bars => &[
                "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃",
            ],
            SpinnerStyle::Arrow => &["←", "↖", "↑", "↗", "→", "↘", "↓", "↙"],
        }
    }
}

/// Animated spinner with configurable style.
pub struct Spinner {
    style: SpinnerStyle,
    index: usize,
    started: Option<Instant>,
    color: Style,
}

impl Spinner {
    /// Create a new moon-style spinner.
    pub fn moon() -> Self {
        Self::new(SpinnerStyle::Moon)
    }

    /// Create a new spinner with the given style.
    pub fn new(style: SpinnerStyle) -> Self {
        Self {
            style,
            index: 0,
            started: Some(Instant::now()),
            color: Style::default().fg(theme::claude()),
        }
    }

    /// Get the current frame without advancing.
    pub fn current_frame(&self) -> &str {
        let frames = self.style.frames();
        frames[self.index % frames.len()]
    }

    /// Advance to the next frame and return it.
    pub fn tick(&mut self) -> &str {
        let frames = self.style.frames();
        self.index = (self.index + 1) % frames.len();
        frames[self.index]
    }

    /// Render the current frame as a Span.
    pub fn render(&self) -> Span<'_> {
        Span::styled(self.current_frame().to_string(), self.color)
    }

    /// Render with a custom label.
    pub fn render_with_label(&self, label: &str) -> Vec<Span<'_>> {
        vec![
            self.render(),
            Span::styled(
                format!(" {}", label),
                Style::default().fg(theme::text_dim()),
            ),
        ]
    }

    /// Set the color.
    pub fn set_color(&mut self, color: Style) {
        self.color = color;
    }

    /// Get elapsed time since start.
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        self.started.map(|t| t.elapsed())
    }

    /// Reset the spinner.
    pub fn reset(&mut self) {
        self.index = 0;
        self.started = Some(Instant::now());
    }
}

/// Global spinner state for the status bar (tick-based animation).
pub struct SpinnerState {
    tick: usize,
}

impl SpinnerState {
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    /// Advance the tick.
    pub fn advance(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Get the current moon frame based on tick.
    pub fn frame(&self) -> &'static str {
        SpinnerStyle::Moon.frames()[self.tick % 4]
    }

    /// Render as a Span.
    pub fn render(&self) -> Span<'_> {
        Span::styled(
            self.frame().to_string(),
            Style::default().fg(theme::claude()),
        )
    }
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_moon() {
        let mut s = Spinner::moon();
        let frame1 = s.tick().to_string();
        let frame2 = s.tick().to_string();
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn spinner_wraps() {
        let mut s = Spinner::moon();
        let initial = s.current_frame().to_string();
        for _ in 0..8 {
            s.tick();
        }
        // After 8 ticks (2 full cycles), should be back to start
        assert_eq!(s.current_frame(), initial.as_str());
    }

    #[test]
    fn spinner_state_advance() {
        let mut s = SpinnerState::new();
        let f1 = s.frame().to_string();
        s.advance();
        let f2 = s.frame().to_string();
        assert_ne!(f1, f2);
    }

    #[test]
    fn spinner_render_with_label() {
        let s = Spinner::moon();
        let spans = s.render_with_label("thinking...");
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn spinner_reset() {
        let mut s = Spinner::moon();
        for _ in 0..5 {
            s.tick();
        }
        s.reset();
        assert_eq!(s.current_frame(), SpinnerStyle::Moon.frames()[0]);
    }

    #[test]
    fn spinner_all_styles() {
        for style in [
            SpinnerStyle::Moon,
            SpinnerStyle::Dots,
            SpinnerStyle::Bars,
            SpinnerStyle::Arrow,
        ] {
            let s = Spinner::new(style);
            assert!(!s.current_frame().is_empty());
        }
    }
}
