use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{AppState, Page, PageId};
use crate::display::theme;

pub struct DiffPage {
    scroll_offset: u16,
    show_annotations: bool,
    line_numbers: bool,
}

impl Default for DiffPage {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffPage {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            show_annotations: true,
            line_numbers: true,
        }
    }
}

impl Page for DiffPage {
    fn title(&self) -> &str {
        "diff"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 6 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Length(2), // file info
                Constraint::Min(3),    // diff content
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header — use clay orange for page title (Claude Code style)
        let header = Line::from(vec![
            Span::styled(
                " diff",
                Style::default()
                    .fg(theme::fg_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if state.branch_name.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", state.branch_name)
                },
                Style::default().fg(theme::fg_dim()),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // File info
        if let Some(diff) = &state.diff_content {
            let files: Vec<&str> = diff
                .lines()
                .filter(|l| l.starts_with("diff --git"))
                .map(|l| {
                    l.strip_prefix("diff --git a/")
                        .unwrap_or(l)
                        .split(" b/")
                        .next()
                        .unwrap_or(l)
                })
                .collect();
            let adds: usize = diff
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count();
            let dels: usize = diff
                .lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                .count();

            let info = Line::from(vec![
                Span::styled(
                    format!("  {} files", files.len()),
                    Style::default()
                        .fg(theme::fg_color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("   +{} -{}", adds, dels),
                    Style::default().fg(theme::GREEN()),
                ),
            ]);
            frame.render_widget(Paragraph::new(info), chunks[1]);
        } else {
            frame.render_widget(
                Paragraph::new("  No diff available").style(Style::default().fg(theme::fg_dim())),
                chunks[1],
            );
        }

        // Diff content with line numbers and word-level highlighting
        let diff_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" CHANGES ");

        if let Some(diff) = &state.diff_content {
            let lines =
                render_diff_with_line_numbers(diff, self.line_numbers, self.show_annotations);

            let total_lines = lines.len() as u16;
            let view_h = chunks[2].height.saturating_sub(2);
            let max_scroll = total_lines.saturating_sub(view_h);
            let scroll = self.scroll_offset.min(max_scroll);

            frame.render_widget(
                Paragraph::new(lines)
                    .block(diff_block)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                chunks[2],
            );
        } else {
            frame.render_widget(
                Paragraph::new("  No diff produced by Coder")
                    .block(diff_block)
                    .style(Style::default().fg(theme::fg_dim())),
                chunks[2],
            );
        }

        // Footer
        let footer = Line::from(vec![
            Span::styled(
                " j/k",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" scroll  ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                "g/G",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" top/bot  ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                "n",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" lines:{}  ", if self.line_numbers { "on" } else { "off" }),
                Style::default().fg(theme::fg_dim()),
            ),
            Span::styled(
                "r",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" annot  ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" back", Style::default().fg(theme::fg_dim())),
        ]);
        frame.render_widget(Paragraph::new(footer), chunks[3]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                true
            }
            KeyCode::Char('g') => {
                self.scroll_offset = 0;
                true
            }
            KeyCode::Char('G') => {
                self.scroll_offset = u16::MAX;
                true
            }
            KeyCode::Char('r') => {
                self.show_annotations = !self.show_annotations;
                true
            }
            KeyCode::Char('n') => {
                self.line_numbers = !self.line_numbers;
                true
            }
            KeyCode::Char('v') => {
                state.current_page = PageId::Verdict;
                true
            }
            _ => false,
        }
    }
}

/// Render a unified diff with line numbers and proper Claude Code styling.
fn render_diff_with_line_numbers(
    diff: &str,
    show_line_numbers: bool,
    _show_annotations: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut old_line = 0u32;
    let mut new_line = 0u32;

    for raw_line in diff.lines() {
        if raw_line.starts_with("diff --git") || raw_line.starts_with("index ") {
            // File header — dim + bold
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )));
        } else if raw_line.starts_with("---") || raw_line.starts_with("+++") {
            // File name header
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            )));
        } else if raw_line.starts_with("@@") {
            // Hunk header — extract line numbers
            if let Some((o, n)) = parse_hunk_header(raw_line) {
                old_line = o;
                new_line = n;
            }
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(theme::DIFF_HUNK()),
            )));
        } else if let Some(content) = raw_line.strip_prefix('+') {
            // Added line — green on diff bg
            let mut spans = Vec::new();
            if show_line_numbers {
                spans.push(Span::styled(
                    format!("{:>4} ", new_line),
                    Style::default().fg(theme::fg_dim()),
                ));
            }
            spans.push(Span::styled(
                format!("+{}", content),
                Style::default()
                    .fg(theme::DIFF_ADD_FG())
                    .bg(theme::DIFF_ADD_BG()),
            ));
            lines.push(Line::from(spans));
            new_line += 1;
        } else if let Some(content) = raw_line.strip_prefix('-') {
            // Removed line — red on diff bg
            let mut spans = Vec::new();
            if show_line_numbers {
                spans.push(Span::styled(
                    format!("{:>4} ", old_line),
                    Style::default().fg(theme::fg_dim()),
                ));
            }
            spans.push(Span::styled(
                format!("-{}", content),
                Style::default()
                    .fg(theme::DIFF_DEL_FG())
                    .bg(theme::DIFF_DEL_BG()),
            ));
            lines.push(Line::from(spans));
            old_line += 1;
        } else {
            // Context line
            let mut spans = Vec::new();
            if show_line_numbers {
                spans.push(Span::styled(
                    format!("{:>4} {:>4} ", old_line, new_line),
                    Style::default().fg(theme::fg_dim()),
                ));
            }
            spans.push(Span::styled(
                format!(" {}", raw_line),
                Style::default().fg(theme::fg_color()),
            ));
            lines.push(Line::from(spans));
            old_line += 1;
            new_line += 1;
        }
    }

    lines
}

/// Parse a @@ -old,count +new,count @@ hunk header, returning (old_start, new_start).
fn parse_hunk_header(header: &str) -> Option<(u32, u32)> {
    // Format: @@ -old_start,old_count +new_start,new_count @@
    let rest = header.strip_prefix("@@ -")?;
    let old_part = rest.split(' ').next()?;
    let old_start = old_part.split(',').next()?.parse::<u32>().ok()?;

    let after_old = rest.split('+').nth(1)?;
    let new_part = after_old.split(' ').next()?;
    let new_start = new_part.split(',').next()?.parse::<u32>().ok()?;

    Some((old_start, new_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hunk_header_basic() {
        let (old, new) = parse_hunk_header("@@ -10,7 +15,9 @@").unwrap();
        assert_eq!(old, 10);
        assert_eq!(new, 15);
    }

    #[test]
    fn parse_hunk_header_no_count() {
        let (old, new) = parse_hunk_header("@@ -1 +1 @@").unwrap();
        assert_eq!(old, 1);
        assert_eq!(new, 1);
    }

    #[test]
    fn parse_hunk_header_invalid() {
        assert!(parse_hunk_header("not a hunk").is_none());
    }

    #[test]
    fn render_diff_produces_lines() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc123..def456 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 use std::io;
+use std::env;
 
 fn main() {
+    let args: Vec<String> = env::args().collect();
     println!(\"hello\");
 }";
        let lines = render_diff_with_line_numbers(diff, true, true);
        assert!(!lines.is_empty());
        // Should have: 2 header lines, 2 file name lines, 1 hunk, 6 content lines = 11
        assert!(lines.len() >= 8);
    }
}
