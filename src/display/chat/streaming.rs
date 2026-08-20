//! Streaming text display with real-time token rendering.
//!
//! Handles incomplete markdown during streaming:
//! - Unclosed code fences (kept open via `trim_partial_closing_fences`)
//! - Unclosed bold/italic spans
//! - Unfinished list items

use ratatui::text::Line;

use super::markdown::render_markdown;
use super::message::MessageRenderConfig;

/// Trim a trailing partial closing code fence so an in-progress code block
/// stays *open* while streaming instead of collapsing on the last fence char.
///
/// While a code block is open (odd number of ``` markers) and the buffer ends
/// with backticks that are part of the still-being-typed closing fence, those
/// trailing backticks are dropped. The block then renders as an open,
/// highlighted code block until the model emits the full closing fence.
pub fn trim_partial_closing_fences(s: &str) -> String {
    let fences = s.matches("```").count();
    if fences % 2 == 1 {
        let trimmed = s.trim_end_matches('`');
        if trimmed.len() < s.len() {
            return trimmed.to_string();
        }
    }
    s.to_string()
}

/// Render markdown for a message that is still streaming.
///
/// Applies [`trim_partial_closing_fences`] so a code block that is open but has
/// not yet received its closing fence does not visually collapse.
pub fn render_streaming_markdown(
    body: &str,
    width: usize,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let trimmed = trim_partial_closing_fences(body);
    render_markdown(&trimmed, width, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn test_config() -> MessageRenderConfig {
        MessageRenderConfig {
            width: 80,
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
    fn trim_partial_closing_fence_drops_trailing_backticks() {
        assert_eq!(
            trim_partial_closing_fences("```rust\nfn main() {\n``"),
            "```rust\nfn main() {\n"
        );
    }

    #[test]
    fn trim_keeps_open_block_without_trailing_backticks() {
        let src = "```rust\nfn main() {";
        assert_eq!(trim_partial_closing_fences(src), src);
    }

    #[test]
    fn trim_keeps_closed_block_untouched() {
        let src = "```rust\nfn main() {}\n```";
        assert_eq!(trim_partial_closing_fences(src), src);
    }

    #[test]
    fn streaming_render_keeps_code_block_open() {
        let config = test_config();
        let body = "Here is code:\n```python\nprint('hi')\n``";
        let lines = render_streaming_markdown(body, 80, &config);
        let text: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(text.contains("print('hi')"));
    }
}
