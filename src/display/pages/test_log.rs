use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{AppState, Page, PageId};
use crate::display::theme;

pub struct TestLogPage {
    scroll_offset: u16,
}

impl TestLogPage {
    pub fn new() -> Self {
        Self { scroll_offset: 0 }
    }
}

impl Page for TestLogPage {
    fn title(&self) -> &str {
        "test_log"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 6 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);

        let header = Line::from(vec![
            Span::styled(
                " test log",
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

        let log_content = state
            .test_log
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("No test output available");

        let mut lines: Vec<Line> = Vec::new();
        for line in log_content.lines() {
            let stripped = line.strip_prefix("   ").unwrap_or(line);
            if stripped.starts_with("test result:") {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line.to_string(), Style::default().fg(theme::GREEN()).add_modifier(Modifier::BOLD)),
                ]));
            } else if stripped.starts_with("running") || stripped.starts_with("Doc-tests") {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line.to_string(), Style::default().fg(theme::fg_dim())),
                ]));
            } else if stripped.starts_with("test ") && stripped.contains(" ... ok") {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line.to_string(), Style::default().fg(theme::GREEN())),
                ]));
            } else if stripped.starts_with("Running") || stripped.starts_with("running 0 tests") {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line.to_string(), Style::default().fg(theme::AMBER())),
                ]));
            } else if line.trim().is_empty() {
                lines.push(Line::from(""));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line.to_string(), Style::default().fg(theme::fg_dim())),
                ]));
            }
        }

        let total_lines = lines.len() as u16;
        let view_h = chunks[1].height.saturating_sub(2);
        let max_scroll = total_lines.saturating_sub(view_h);
        let scroll = self.scroll_offset.min(max_scroll);

        let log_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" TEST OUTPUT ");

        frame.render_widget(
            Paragraph::new(lines)
                .block(log_block)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            chunks[1],
        );

        let footer = Line::from(vec![Span::styled(
            " [j/k] scroll   [g/G] top/bottom   [Esc] back",
            Style::default().fg(theme::fg_dim()),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[2]);
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
            _ => false,
        }
    }
}
