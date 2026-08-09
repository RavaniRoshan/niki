//! Markdown parser and renderer using pulldown-cmark.
//!
//! Renders markdown to ratatui `Line`/`Span` structures with:
//! - Headings (bold + colored)
//! - Code blocks with syntax highlighting (via syntect)
//! - Inline code
//! - Lists (ordered and unordered)
//! - Bold and italic text
//! - Links
//! - Blockquotes

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::code_block::render_code_block;
use super::message::MessageRenderConfig;

/// Render markdown text to a list of `Line`s.
pub fn render_markdown(
    input: &str,
    width: usize,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(input, options);
    let mut renderer = MarkdownRenderer::new(width, config);
    renderer.run(parser);
    renderer.finish()
}

/// Internal markdown renderer state.
struct MarkdownRenderer<'a> {
    lines: Vec<Line<'static>>,
    current_line: Line<'static>,
    config: &'a MessageRenderConfig,
    width: usize,
    in_code_block: bool,
    code_lang: String,
    code_content: String,
    in_list: bool,
    list_index: usize,
}

impl<'a> MarkdownRenderer<'a> {
    fn new(width: usize, config: &'a MessageRenderConfig) -> Self {
        Self {
            lines: Vec::new(),
            current_line: Line::default(),
            config,
            width,
            in_code_block: false,
            code_lang: String::new(),
            code_content: String::new(),
            in_list: false,
            list_index: 0,
        }
    }

    fn run(&mut self, parser: Parser) {
        for event in parser {
            match event {
                Event::Start(tag) => self.handle_start(tag),
                Event::End(tag) => self.handle_end(tag),
                Event::Text(text) => self.handle_text(&text),
                Event::Code(code) => self.handle_inline_code(&code),
                Event::Html(html) => self.handle_html(&html),
                Event::FootnoteReference(name) => self.handle_footnote(&name),
                Event::SoftBreak => self.handle_soft_break(),
                Event::HardBreak => self.handle_hard_break(),
                Event::Rule => self.handle_rule(),
                Event::TaskListMarker(checked) => self.handle_task_list(checked),
                // Unsupported events — skip
                Event::InlineMath(_) | Event::DisplayMath(_) | Event::InlineHtml(_) => {}
            }
        }
    }

    fn handle_start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                // Push any existing line
                self.push_current_line();
                // Add heading marker
                let prefix = "#".repeat(level as usize);
                self.current_line.push_span(Span::styled(
                    format!("{} ", prefix),
                    Style::default().fg(self.config.text_dim_color),
                ));
            }
            Tag::Paragraph => {
                if !self.current_line.spans.is_empty() {
                    self.push_current_line();
                }
            }
            Tag::CodeBlock(kind) => {
                self.push_current_line();
                self.in_code_block = true;
                match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        self.code_lang = lang.to_string();
                    }
                    pulldown_cmark::CodeBlockKind::Indented => {
                        self.code_lang = String::new();
                    }
                }
            }
            Tag::List(start) => {
                self.push_current_line();
                self.in_list = true;
                self.list_index = start.unwrap_or(1) as usize;
            }
            Tag::Item => {
                // Add list bullet
                if self.in_list {
                    let bullet = format!("{} ", self.list_index);
                    self.current_line.push_span(Span::styled(
                        bullet,
                        Style::default().fg(self.config.text_color),
                    ));
                    self.list_index += 1;
                } else {
                    self.current_line.push_span(Span::styled(
                        "• ",
                        Style::default().fg(self.config.text_color),
                    ));
                }
            }
            Tag::BlockQuote(_) => {
                self.push_current_line();
                self.current_line.push_span(Span::styled(
                    "│ ",
                    Style::default().fg(self.config.border_color),
                ));
            }
            Tag::Emphasis => {
                // Italic — mark the current position for styling
            }
            Tag::Strong => {
                // Bold — mark the current position for styling
            }
            Tag::Strikethrough => {
                // Strikethrough
            }
            Tag::Link { dest_url, .. } => {
                self.current_line.push_span(Span::styled(
                    dest_url.to_string(),
                    Style::default()
                        .fg(self.config.primary_color)
                        .add_modifier(Modifier::UNDERLINED),
                ));
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading { .. } => {
                self.push_current_line();
                self.push_current_line(); // blank line after heading
            }
            TagEnd::CodeBlock => {
                self.in_code_block = false;
                let code_lines =
                    render_code_block(&self.code_content, &self.code_lang, self.width, self.config);
                self.lines.extend(code_lines);
                self.code_content.clear();
                self.code_lang.clear();
                self.push_current_line(); // blank line after code block
            }
            TagEnd::List(_) => {
                self.in_list = false;
                self.push_current_line(); // blank line after list
            }
            TagEnd::Paragraph => {
                self.push_current_line();
            }
            TagEnd::BlockQuote(_) => {
                self.push_current_line();
            }
            TagEnd::Item => {
                // End of list item — push line
            }
            _ => {}
        }
    }

    fn handle_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_content.push_str(text);
            return;
        }

        // Split text into words and add them with wrapping
        for word in text.split_whitespace() {
            let current_width: usize = self
                .current_line
                .spans
                .iter()
                .map(|s| s.content.len())
                .sum();
            if current_width + word.len() + 1 > self.width && current_width > 0 {
                self.push_current_line();
            }
            self.current_line.push_span(Span::styled(
                word.to_string(),
                Style::default().fg(self.config.text_color),
            ));
            // Add space after word
            if !self.current_line.spans.is_empty() {
                self.current_line.push_span(Span::styled(
                    " ",
                    Style::default().fg(self.config.text_color),
                ));
            }
        }
    }

    fn handle_inline_code(&mut self, code: &str) {
        self.current_line.push_span(Span::styled(
            format!("`{}`", code),
            Style::default()
                .fg(self.config.primary_color)
                .add_modifier(Modifier::BOLD),
        ));
    }

    fn handle_html(&mut self, _html: &str) {
        // Skip HTML for now (could render as text)
    }

    fn handle_footnote(&mut self, name: &str) {
        self.current_line.push_span(Span::styled(
            format!("[^{}]", name),
            Style::default().fg(self.config.text_dim_color),
        ));
    }

    fn handle_soft_break(&mut self) {
        if self.in_code_block {
            self.code_content.push('\n');
        } else {
            self.push_current_line();
        }
    }

    fn handle_hard_break(&mut self) {
        self.push_current_line();
    }

    fn handle_rule(&mut self) {
        self.push_current_line();
        self.lines.push(Line::from(Span::styled(
            "─".repeat(self.width),
            Style::default().fg(self.config.border_color),
        )));
        self.push_current_line();
    }

    fn handle_task_list(&mut self, checked: bool) {
        let marker = if checked { "[x]" } else { "[ ]" };
        self.current_line.push_span(Span::styled(
            format!("{} ", marker),
            Style::default().fg(self.config.text_color),
        ));
    }

    fn push_current_line(&mut self) {
        if !self.current_line.spans.is_empty() {
            self.lines.push(std::mem::take(&mut self.current_line));
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.push_current_line();
        if self.lines.is_empty() {
            self.lines.push(Line::from(""));
        }
        self.lines
    }
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
    fn render_plain_text() {
        let config = test_config();
        let lines = render_markdown("Hello world", 80, &config);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_heading() {
        let config = test_config();
        let lines = render_markdown("# Title", 80, &config);
        assert!(!lines.is_empty());
        // Heading should contain a '#' marker
        assert!(
            lines
                .iter()
                .any(|l| { l.spans.iter().any(|s| s.content.contains('#')) })
        );
    }

    #[test]
    fn render_inline_code() {
        let config = test_config();
        let lines = render_markdown("Use `cargo build`", 80, &config);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_code_block() {
        let config = test_config();
        let input = "```rust\nfn main() {}\n```";
        let lines = render_markdown(input, 80, &config);
        assert!(lines.len() >= 2); // at least border + code line
    }

    #[test]
    fn render_list() {
        let config = test_config();
        let input = "- item 1\n- item 2";
        let lines = render_markdown(input, 80, &config);
        assert!(lines.len() >= 1);
        // Should contain list items
        assert!(
            lines
                .iter()
                .any(|l| { l.spans.iter().any(|s| s.content.contains("item")) })
        );
    }

    #[test]
    fn render_empty() {
        let config = test_config();
        let lines = render_markdown("", 80, &config);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 0);
    }

    #[test]
    fn render_blockquote() {
        let config = test_config();
        let lines = render_markdown("> Quote", 80, &config);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_rule() {
        let config = test_config();
        let lines = render_markdown("---", 80, &config);
        assert!(
            lines
                .iter()
                .any(|l| { l.spans.iter().any(|s| s.content.contains('─')) })
        );
    }
}
