//! Syntax-highlighted code block rendering.
//!
//! Uses syntect for syntax highlighting with fallback to plain text.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{ThemeSet};
use syntect::parsing::SyntaxSet;

use super::message::MessageRenderConfig;

/// Lazy-loaded syntax set (initialized on first use).
fn syntax_set() -> &'static SyntaxSet {
    use std::sync::OnceLock;
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Lazy-loaded theme set.
fn theme_set() -> &'static ThemeSet {
    use std::sync::OnceLock;
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

/// Render a code block with syntax highlighting.
pub fn render_code_block(
    code: &str,
    lang: &str,
    width: usize,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let ss = syntax_set();
    let ts = theme_set();

    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_token(lang).unwrap_or_else(|| ss.find_syntax_plain_text())
    };

    let theme_name = if crate::display::theme::is_light() {
        "base16-ocean.light"
    } else {
        "base16-ocean.dark"
    };
    let theme = &ts.themes[theme_name];
    let mut h = HighlightLines::new(syntax, theme);

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Top border
    lines.push(Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(config.border_color),
    )));

    // Highlighted code lines
    for code_line in code.lines() {
        let highlighted = match h.highlight_line(code_line, ss) {
            Ok(h) => h,
            Err(_) => vec![(syntect::highlighting::Style::default(), code_line)],
        };

        let mut rendered = Line::default();
        for (style, text) in highlighted {
            let fg = syntect_color_to_ratatui(style.foreground);
            rendered.push_span(Span::styled(text.to_string(), Style::default().fg(fg)));
        }
        lines.push(rendered);
    }

    // Bottom border
    lines.push(Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(config.border_color),
    )));

    lines
}

/// Convert syntect color to ratatui Color.
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MessageRenderConfig {
        MessageRenderConfig {
            width: 40,
            show_timestamps: false,
            role_user_color: Color::Yellow,
            role_assistant_color: Color::Blue,
            role_system_color: Color::Gray,
            text_color: Color::White,
            text_dim_color: Color::Gray,
            border_color: Color::DarkGray,
            success_color: Color::Green,
            warning_color: Color::Yellow,
            error_color: Color::Red,
            claude_color: Color::Magenta,
            primary_color: Color::Cyan,
        }
    }

    #[test]
    fn render_code_block_test() {
        let config = test_config();
        let lines = render_code_block("fn main() {}", "rust", 40, &config);
        assert!(lines.len() >= 3); // top, content, bottom
    }

    #[test]
    fn render_code_block_plain() {
        let config = test_config();
        let lines = render_code_block("hello", "", 40, &config);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn syntect_color_conversion() {
        let c = syntect::highlighting::Color { r: 255, g: 128, b: 0, a: 255 };
        let ratatui_color = syntect_color_to_ratatui(c);
        assert_eq!(ratatui_color, Color::Rgb(255, 128, 0));
    }
}
