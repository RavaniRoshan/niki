use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{AppState, Page, PageId};
use crate::display::theme;

pub struct HelpPage;

impl HelpPage {
    pub fn new() -> Self {
        Self
    }
}

impl Page for HelpPage {
    fn title(&self) -> &str {
        "help"
    }

    fn render(&self, frame: &mut Frame, area: Rect, _state: &AppState) {
        if area.height < 10 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(8),    // help content
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![Span::styled(
            " help",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Help content - two column layout
        let help_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" KEY BINDINGS ");

        let help_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  GLOBAL",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(
                    "    [q] quit                        ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "  [?] this help                    ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "    [Esc] close/back                ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "                                  ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  PAGES",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(
                    "    [p] pipeline    [a] agents      ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "  [d] diff        [v] verdict      ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "    [c] cost        [f] artifacts   ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "  [h] history                      ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  RUN",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(
                    "    [Space] pause/resume stream     ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "  [g/G] top/bottom of stream      ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "    [j/k] scroll up/down            ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "                                  ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  PIPELINE / AGENTS",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(
                    "    [j/k] next/prev stage           ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "  [Tab] next agent               ",
                    Style::default().fg(theme::FG),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "    [Esc] back                     ",
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    "                                  ",
                    Style::default().fg(theme::FG),
                ),
            ]),
        ];

        frame.render_widget(Paragraph::new(help_lines).block(help_block), chunks[1]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [Esc] or [?] close help",
            Style::default().fg(theme::FG_DIM),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            _ => false,
        }
    }
}
