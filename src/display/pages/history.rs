use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::display::theme;
use super::{AppState, Page, PageId};

pub struct HistoryPage {
    selected: usize,
}

impl HistoryPage {
    pub fn new() -> Self {
        Self {
            selected: 0,
        }
    }
}

impl Page for HistoryPage {
    fn title(&self) -> &str {
        "history"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(5),   // history table
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(" history", Style::default().fg(theme::BLUE).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" · {}", state.project_path.display()),
                Style::default().fg(theme::FG_DIM),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // History table
        let table_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" PAST RUNS ");

        let mut table_lines: Vec<Line> = Vec::new();

        // Header row
        table_lines.push(Line::from(vec![
            Span::styled("  ID         ", Style::default().fg(theme::FG_DIM).add_modifier(Modifier::BOLD)),
            Span::styled("TASK                              ", Style::default().fg(theme::FG_DIM).add_modifier(Modifier::BOLD)),
            Span::styled("VERDICT     ", Style::default().fg(theme::FG_DIM).add_modifier(Modifier::BOLD)),
            Span::styled("WHEN", Style::default().fg(theme::FG_DIM).add_modifier(Modifier::BOLD)),
        ]));
        table_lines.push(Line::from(Span::styled(
            "  ────────   ──────────────────────────────────  ──────────  ─────",
            Style::default().fg(theme::BORDER),
        )));

        // Sample history entries (in real implementation, these would come from .niki/ directory)
        let entries = vec![
            ("6d281d6d", "Add GET /health endpoint", "approved", "2m ago", theme::GREEN),
            ("a91f3c02", "Refactor auth middleware", "changes", "1h ago", theme::AMBER),
            ("4b7e9d18", "Add input validation", "failed", "3h ago", theme::RED),
            ("2c1a8f55", "Migrate to ESM modules", "approved", "1d ago", theme::GREEN),
        ];

        for (i, (id, task, verdict, when, verdict_color)) in entries.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::FG)
            };

            table_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<8}  ", id),
                    Style::default().fg(theme::BLUE),
                ),
                Span::styled(
                    format!("{:<32}  ", task),
                    style,
                ),
                Span::styled(
                    format!("{:<10}  ", verdict),
                    Style::default().fg(*verdict_color),
                ),
                Span::styled(
                    when.to_string(),
                    Style::default().fg(theme::FG_DIM),
                ),
            ]));
        }

        frame.render_widget(
            Paragraph::new(table_lines).block(table_block),
            chunks[1],
        );

        // Footer
        let footer = Line::from(vec![
            Span::styled(" [j/k] navigate   [Esc] back", Style::default().fg(theme::FG_DIM)),
        ]);
        frame.render_widget(Paragraph::new(footer), chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected = (self.selected + 1).min(3);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                true
            }
            KeyCode::Char('f') => {
                state.current_page = PageId::Artifacts;
                true
            }
            KeyCode::Char('p') => {
                state.current_page = PageId::Pipeline;
                true
            }
            _ => false,
        }
    }
}
