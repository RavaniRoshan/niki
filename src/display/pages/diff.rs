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
}

impl DiffPage {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            show_annotations: true,
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

        // Header
        let header = Line::from(vec![
            Span::styled(
                " diff",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if state.branch_name.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", state.branch_name)
                },
                Style::default().fg(theme::FG_DIM),
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
                    Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("   +{} -{}", adds, dels),
                    Style::default().fg(theme::GREEN),
                ),
            ]);
            frame.render_widget(Paragraph::new(info), chunks[1]);
        } else {
            frame.render_widget(
                Paragraph::new("  No diff available").style(Style::default().fg(theme::FG_DIM)),
                chunks[1],
            );
        }

        // Diff content
        let diff_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" CHANGES ");

        if let Some(diff) = &state.diff_content {
            let mut lines: Vec<Line> = Vec::new();
            for line in diff.lines() {
                let style = if line.starts_with("diff --git") || line.starts_with("index ") {
                    Style::default()
                        .fg(theme::BLUE)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("---") || line.starts_with("+++") {
                    Style::default()
                        .fg(theme::FG_DIM)
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("@@") {
                    Style::default().fg(theme::AMBER)
                } else if line.starts_with('+') {
                    Style::default().fg(theme::GREEN).bg(theme::DIFF_ADD_BG)
                } else if line.starts_with('-') {
                    Style::default().fg(theme::RED).bg(theme::DIFF_DEL_BG)
                } else {
                    Style::default().fg(theme::FG)
                };
                lines.push(Line::from(Span::styled(line.to_string(), style)));
            }

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
                    .style(Style::default().fg(theme::FG_DIM)),
                chunks[2],
            );
        }

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] scroll   [g/G] top/bottom   [r] annotations   [Esc] back",
            Style::default().fg(theme::FG_DIM),
        )]);
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
            KeyCode::Char('v') => {
                state.current_page = PageId::Verdict;
                true
            }
            _ => false,
        }
    }
}
