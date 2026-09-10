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
///
/// When `highlight` is `false` (used while a message is still streaming) code
/// blocks are rendered without syntax highlighting. This is the first phase of a
/// two-phase render: the raw text appears instantly (low TTFT), then the final
/// message is re-rendered with `highlight = true` so the syntax colors land in
/// one stable pass instead of flickering back to plain mid-stream.
pub fn render_markdown(
    input: &str,
    width: usize,
    config: &MessageRenderConfig,
    highlight: bool,
) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(input, options);
    let mut renderer = MarkdownRenderer::new(width, config, highlight);
    renderer.run(parser);
    renderer.finish()
}

/// Wrap link text in an OSC 8 hyperlink sequence (TUI-021). Terminals with
/// support linkify it; width accounting treats the escapes as zero-width
/// only if the terminal does — long linked lines may wrap early, which beats
/// an unlinkable URL dump.
fn osc8_link(text: &str, url: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// Plain (un-highlighted) code block for two-phase streaming render.
fn render_code_block_plain(
    code: &str,
    _width: usize,
    config: &MessageRenderConfig,
) -> Vec<Line<'static>> {
    code.lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(config.border_color),
            ))
        })
        .collect()
}

/// Internal markdown renderer state.
struct MarkdownRenderer<'a> {
    lines: Vec<Line<'static>>,
    current_line: Line<'static>,
    config: &'a MessageRenderConfig,
    width: usize,
    highlight: bool,
    in_code_block: bool,
    code_lang: String,
    code_content: String,
    in_list: bool,
    list_index: usize,
    /// Active `[text](url)` destination, if any (TUI-021 hyperlink support).
    link_url: Option<String>,
    /// Inside a table cell: text accumulates into the cell, not the line.
    in_table: bool,
    in_table_cell: bool,
    table_rows: Vec<Vec<String>>,
}

impl<'a> MarkdownRenderer<'a> {
    fn new(width: usize, config: &'a MessageRenderConfig, highlight: bool) -> Self {
        Self {
            lines: Vec::new(),
            current_line: Line::default(),
            config,
            width,
            highlight,
            in_code_block: false,
            code_lang: String::new(),
            code_content: String::new(),
            in_list: false,
            list_index: 0,
            link_url: None,
            in_table: false,
            in_table_cell: false,
            table_rows: Vec::new(),
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
                // Defer emission to the text/end handlers so the link text
                // comes first (TUI-021).
                self.link_url = Some(dest_url.to_string());
            }
            Tag::Table(_aligns) => {
                // TUI-032: collect rows, lay out at End (width-aware).
                self.push_current_line();
                self.in_table = true;
                self.table_rows.clear();
            }
            Tag::TableHead | Tag::TableRow => {
                if self.in_table {
                    self.table_rows.push(Vec::new());
                }
            }
            Tag::TableCell if self.in_table => {
                if let Some(row) = self.table_rows.last_mut() {
                    row.push(String::new());
                }
                self.in_table_cell = true;
            }
            Tag::TableCell => {}
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
                let code_lines = if self.highlight {
                    render_code_block(&self.code_content, &self.code_lang, self.width, self.config)
                } else {
                    render_code_block_plain(&self.code_content, self.width, self.config)
                };
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
            TagEnd::Table => {
                self.in_table = false;
                self.in_table_cell = false;
                self.render_table();
            }
            TagEnd::TableHead | TagEnd::TableRow => {}
            TagEnd::TableCell => {
                self.in_table_cell = false;
            }
            TagEnd::Link => {
                // Hyperlink terminals already linkified the text inline;
                // otherwise fall back to an explicit URL suffix.
                if let Some(url) = self.link_url.take() {
                    if !self.config.hyperlinks {
                        self.current_line.push_span(Span::styled(
                            format!(" ({url})"),
                            Style::default()
                                .fg(self.config.primary_color)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                    }
                }
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

    /// Lay out collected table rows within `width` (TUI-032). Columns share
    /// the width budget; cells truncate with an ellipsis. Too narrow for even
    /// minimal columns → raw `| a | b |` fallback lines, truncated to width.
    fn render_table(&mut self) {
        use unicode_width::UnicodeWidthStr;
        let rows = std::mem::take(&mut self.table_rows);
        if rows.is_empty() {
            return;
        }
        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if cols == 0 {
            return;
        }
        // Natural column widths (display cells, not bytes).
        let mut widths = vec![0usize; cols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.width());
            }
        }
        // Each row renders as "│ c0 │ c1 │" → separators cost 3*n + 1.
        let chrome = 3 * cols + 1;
        let avail = self.width.saturating_sub(chrome);
        // Columns narrower than 3 cells are unreadable — fall back to raw.
        if avail < 3 * cols {
            // Too narrow: raw fallback, hard-truncated.
            for row in &rows {
                let raw = format!("| {} |", row.join(" | "));
                let line = crate::display::theme::truncate_str(&raw, self.width);
                self.lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(self.config.text_color),
                )));
            }
            return;
        }
        // Shrink widest columns first until the budget fits.
        if widths.iter().sum::<usize>() > avail {
            while widths.iter().sum::<usize>() > avail {
                let mut widest = 0;
                for (i, w) in widths.iter().enumerate() {
                    if *w > widths[widest] && *w > 1 {
                        widest = i;
                    }
                }
                if widths[widest] <= 1 {
                    break;
                }
                widths[widest] -= 1;
            }
        }
        for (ri, row) in rows.iter().enumerate() {
            let mut spans = vec![Span::styled(
                "│ ".to_string(),
                Style::default().fg(self.config.border_color),
            )];
            for (i, max_w) in widths.iter().enumerate() {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let fit = crate::display::theme::truncate_str(cell, *max_w);
                let pad = max_w.saturating_sub(fit.width());
                spans.push(Span::styled(
                    fit,
                    Style::default().fg(if ri == 0 {
                        self.config.primary_color
                    } else {
                        self.config.text_color
                    }),
                ));
                spans.push(Span::styled(" ".repeat(pad), Style::default()));
                spans.push(Span::styled(
                    " │ ".to_string(),
                    Style::default().fg(self.config.border_color),
                ));
            }
            // Trim the trailing space of the last separator ("│ " → "│").
            if let Some(last) = spans.pop() {
                let mut text = last.content.into_owned();
                text.pop();
                spans.push(Span::styled(
                    text,
                    Style::default().fg(self.config.border_color),
                ));
            }
            self.lines.push(Line::from(spans));
            // Header separator after the first row.
            if ri == 0 {
                let total: usize = widths.iter().sum::<usize>() + chrome;
                self.lines.push(Line::from(Span::styled(
                    format!("├{}┤", "─".repeat(total.saturating_sub(2))),
                    Style::default().fg(self.config.border_color),
                )));
            }
        }
    }

    fn handle_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_content.push_str(text);
            return;
        }

        // Table cells accumulate raw text for width-aware layout at End.
        if self.in_table_cell {
            if let Some(row) = self.table_rows.last_mut() {
                if let Some(cell) = row.last_mut() {
                    if !cell.is_empty() {
                        cell.push(' ');
                    }
                    cell.push_str(text);
                }
            }
            return;
        }

        // Inside a link on a hyperlink terminal, wrap each word in OSC 8.
        let link_url = if self.config.hyperlinks {
            self.link_url.clone()
        } else {
            None
        };

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
            let shown = match &link_url {
                Some(url) => osc8_link(word, url),
                None => word.to_string(),
            };
            self.current_line.push_span(Span::styled(
                shown,
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
        if self.in_table_cell {
            if let Some(row) = self.table_rows.last_mut() {
                if let Some(cell) = row.last_mut() {
                    if !cell.is_empty() {
                        cell.push(' ');
                    }
                    cell.push('`');
                    cell.push_str(code);
                    cell.push('`');
                }
            }
            return;
        }
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
        } else {
            // SEGMENT_RESET: terminate every line with a reset span so styled
            // content cannot bleed into the next line or adjacent widget.
            for line in &mut self.lines {
                line.spans.push(Span::styled("", Style::default()));
            }
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
            hyperlinks: false,
        }
    }

    #[test]
    fn render_plain_text() {
        let config = test_config();
        let lines = render_markdown("Hello world", 80, &config, true);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_heading() {
        let config = test_config();
        let lines = render_markdown("# Title", 80, &config, true);
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
        let lines = render_markdown("Use `cargo build`", 80, &config, true);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_code_block() {
        let config = test_config();
        let input = "```rust\nfn main() {}\n```";
        let lines = render_markdown(input, 80, &config, true);
        assert!(lines.len() >= 2); // at least border + code line
    }

    #[test]
    fn render_list() {
        let config = test_config();
        let input = "- item 1\n- item 2";
        let lines = render_markdown(input, 80, &config, true);
        assert!(!lines.is_empty());
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
        let lines = render_markdown("", 80, &config, true);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 0);
    }

    #[test]
    fn render_blockquote() {
        let config = test_config();
        let lines = render_markdown("> Quote", 80, &config, true);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_rule() {
        let config = test_config();
        let lines = render_markdown("---", 80, &config, true);
        assert!(
            lines
                .iter()
                .any(|l| { l.spans.iter().any(|s| s.content.contains('─')) })
        );
    }

    fn flat_text(lines: &[ratatui::text::Line]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn render_link_without_hyperlinks_shows_url() {
        let config = test_config();
        assert!(!config.hyperlinks);
        let lines = render_markdown("[docs](https://example.com/x)", 80, &config, true);
        let text = flat_text(&lines);
        assert!(text.contains("docs"), "{text}");
        assert!(text.contains("https://example.com/x"), "{text}");
        assert!(!text.contains("\x1b]8"), "{text}");
    }

    #[test]
    fn render_link_with_hyperlinks_emits_osc8() {
        let mut config = test_config();
        config.hyperlinks = true;
        let lines = render_markdown("[docs](https://example.com/x)", 80, &config, true);
        let text = flat_text(&lines);
        assert!(text.contains("\x1b]8;;https://example.com/x"), "{text}");
        assert!(text.contains("docs"), "{text}");
        // URL appears once (inside the sequence), not as a visible suffix.
        assert_eq!(text.matches("https://example.com/x").count(), 1);
    }

    #[test]
    fn render_table_fits_width() {
        let config = test_config();
        let md = "| name | value |\n| --- | --- |\n| alpha | 1 |\n| beta-long-name | 22 |";
        let lines = render_markdown(md, 40, &config, true);
        let text = flat_text(&lines);
        assert!(text.contains("alpha"), "{text}");
        assert!(text.contains("│"), "{text}");
        for line in &lines {
            let w: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(w <= 40, "overflow: {line:?}");
        }
    }

    #[test]
    fn render_table_narrow_falls_back() {
        let config = test_config();
        let md = "| name | value |\n| --- | --- |\n| alpha | 1 |";
        let lines = render_markdown(md, 10, &config, true);
        let text = flat_text(&lines);
        assert!(text.contains("alpha"), "{text}");
        for line in &lines {
            let w: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(w <= 10, "overflow: {line:?}");
        }
    }
}
