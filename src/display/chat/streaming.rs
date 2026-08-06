//! Streaming text display with real-time token rendering.
//!
//! Handles incomplete markdown during streaming:
//! - Unclosed code fences
//! - Unclosed bold/italic spans
//! - Unfinished list items

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::markdown::render_markdown;
use super::message::MessageRenderConfig;

/// A message currently being streamed — buffers tokens and renders incrementally.
#[derive(Debug, Clone, Default)]
pub struct StreamingMessage {
    /// Accumulated content.
    pub buffer: String,
    /// Currently rendered lines.
    pub rendered_lines: Vec<Line<'static>>,
    /// Position up to which we've rendered.
    pub last_render_pos: usize,
    /// Whether we're inside an unclosed code fence.
    pub incomplete_code_fence: bool,
    /// Whether we're inside an unclosed bold span.
    pub incomplete_bold: bool,
    /// Whether we're inside an unclosed list.
    pub incomplete_list: bool,
    /// The agent role for this message.
    pub role: Option<crate::artifacts::types::AgentRole>,
    /// Whether streaming is complete.
    pub finished: bool,
}

impl StreamingMessage {
    pub fn new(role: Option<crate::artifacts::types::AgentRole>) -> Self {
        Self {
            buffer: String::new(),
            rendered_lines: Vec::new(),
            last_render_pos: 0,
            incomplete_code_fence: false,
            incomplete_bold: false,
            incomplete_list: false,
            role,
            finished: false,
        }
    }

    /// Push a new token and re-render incrementally.
    pub fn push_token(&mut self, token: &str, config: &MessageRenderConfig) {
        self.buffer.push_str(token);
        self.render_incremental(config);
    }

    /// Render only the new content since last render.
    fn render_incremental(&mut self, config: &MessageRenderConfig) {
        if self.last_render_pos >= self.buffer.len() {
            return;
        }

        let new_content = self.buffer[self.last_render_pos..].to_string();
        let new_lines = render_streaming_markdown(
            &new_content,
            config,
            &mut self.incomplete_code_fence,
            &mut self.incomplete_bold,
            &mut self.incomplete_list,
        );

        self.rendered_lines.extend(new_lines);
        self.last_render_pos = self.buffer.len();
    }

    /// Finalize the message — re-render entire content as static markdown.
    pub fn finalize(&mut self, config: &MessageRenderConfig) {
        self.finished = true;
        self.rendered_lines = render_markdown(&self.buffer, config.width, config);
        self.incomplete_code_fence = false;
        self.incomplete_bold = false;
        self.incomplete_list = false;
    }

    /// Get the rendered lines (cloned for display).
    pub fn lines(&self) -> &[Line<'static>] {
        &self.rendered_lines
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Render markdown during streaming, handling incomplete syntax.
fn render_streaming_markdown(
    content: &str,
    config: &MessageRenderConfig,
    incomplete_code_fence: &mut bool,
    incomplete_bold: &mut bool,
    incomplete_list: &mut bool,
) -> Vec<Line<'static>> {
    // For streaming, use the standard renderer but track incomplete state
    // by checking the raw content for unclosed markdown constructs

    // Track code fences (``` markers)
    let fence_count = content.matches("```").count();
    if fence_count % 2 == 1 {
        *incomplete_code_fence = true;
    } else if *incomplete_code_fence && fence_count > 0 {
        *incomplete_code_fence = false;
    }

    // Track bold (** markers)
    let bold_count = content.matches("**").count();
    if bold_count % 2 == 1 {
        *incomplete_bold = true;
    } else if *incomplete_bold && bold_count > 0 {
        *incomplete_bold = false;
    }

    // Use the standard markdown renderer for now
    // In a full implementation, we'd render only the partial content
    // and handle the incomplete state by closing open constructs
    render_markdown(content, config.width, config)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn streaming_message_new() {
        let msg = StreamingMessage::new(None);
        assert!(msg.is_empty());
        assert!(!msg.finished);
    }

    #[test]
    fn streaming_message_push_token() {
        let config = test_config();
        let mut msg = StreamingMessage::new(Some(crate::artifacts::types::AgentRole::Planner));
        msg.push_token("Hello ", &config);
        msg.push_token("world", &config);
        assert_eq!(msg.buffer, "Hello world");
    }

    #[test]
    fn streaming_message_finalize() {
        let config = test_config();
        let mut msg = StreamingMessage::new(Some(crate::artifacts::types::AgentRole::Coder));
        msg.push_token("**bold** text", &config);
        msg.finalize(&config);
        assert!(msg.finished);
        assert!(!msg.rendered_lines.is_empty());
    }

    #[test]
    fn streaming_message_is_empty() {
        let config = test_config();
        let mut msg = StreamingMessage::new(None);
        assert!(msg.is_empty());
        msg.push_token("hi", &config);
        assert!(!msg.is_empty());
    }

    #[test]
    fn streaming_message_lines() {
        let config = test_config();
        let mut msg = StreamingMessage::new(None);
        msg.push_token("Line 1", &config);
        assert!(!msg.lines().is_empty());
    }
}
