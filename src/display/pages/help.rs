use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{AppState, Page, PageId};
use crate::display::theme;

struct HelpSection {
    title: &'static str,
    items: Vec<(&'static str, &'static str)>,
    collapsed: bool,
}

impl HelpSection {
    fn new(title: &'static str, items: Vec<(&'static str, &'static str)>) -> Self {
        Self {
            title,
            items,
            collapsed: false,
        }
    }
}

pub struct HelpPage {
    sections: Vec<HelpSection>,
    selected_section: usize,
    scroll_offset: u16,
}

impl Default for HelpPage {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpPage {
    pub fn new() -> Self {
        Self {
            sections: vec![
                HelpSection::new(
                    "GLOBAL",
                    vec![
                        ("[q] quit", "Quit NIKI"),
                        (
                            "[Esc] close/back",
                            "Close current modal or return to previous page",
                        ),
                        ("[?] this help", "Toggle this help page"),
                        ("[Ctrl+P] commands", "Open command palette"),
                        ("[Ctrl+T] theme", "Cycle light/dark/auto themes"),
                    ],
                ),
                HelpSection::new(
                    "PAGES",
                    vec![
                        ("[p] pipeline", "View pipeline stage cards and status"),
                        ("[a] agents", "View agent transcripts and token usage"),
                        ("[d] diff", "View code changes with line numbers"),
                        ("[v] verdict", "View reviewer verdict and report"),
                        ("[c] cost", "View token usage and cost breakdown"),
                        ("[f] artifacts", "Browse generated artifacts"),
                        ("[h] history", "View past runs from .niki/tasks/"),
                        ("[,] config", "View and edit niki.toml settings"),
                    ],
                ),
                HelpSection::new(
                    "RUN",
                    vec![
                        ("[Space] pause/resume", "Pause or resume live stream"),
                        ("[g/G] top/bottom", "Scroll to top or bottom of stream"),
                        ("[j/k] scroll", "Scroll up/down line by line"),
                    ],
                ),
                HelpSection::new(
                    "PIPELINE / AGENTS",
                    vec![
                        ("[j/k] next/prev", "Navigate stages or agents"),
                        ("[Tab] next agent", "Switch between agent tabs"),
                        ("[Enter] select", "Select a stage card or entry"),
                    ],
                ),
                HelpSection::new(
                    "DIFF",
                    vec![
                        ("[j/k] scroll", "Scroll up/down through diff"),
                        ("[g/G] top/bottom", "Jump to top or bottom of diff"),
                        ("[r] annot", "Toggle inline annotations"),
                        ("[n] lines", "Toggle line numbers"),
                    ],
                ),
            ],
            selected_section: 0,
            scroll_offset: 0,
        }
    }
}

impl Page for HelpPage {
    fn title(&self) -> &str {
        "help"
    }

    fn render(&self, frame: &mut Frame, area: Rect, _state: &AppState) {
        if area.height < 6 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(5),    // content
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![Span::styled(
            " help",
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD),
        )]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Collect lines for rendering with scroll
        let mut help_lines: Vec<Line> = Vec::new();

        for (i, section) in self.sections.iter().enumerate() {
            let is_selected = i == self.selected_section;
            let toggle = if section.collapsed { "▸" } else { "▾" };
            let toggle_color = if is_selected {
                theme::border_active()
            } else {
                theme::fg_dim()
            };

            help_lines.push(Line::from(vec![
                Span::styled(format!("  {} ", toggle), Style::default().fg(toggle_color)),
                Span::styled(
                    section.title,
                    Style::default()
                        .fg(theme::BLUE())
                        .add_modifier(if is_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]));

            if !section.collapsed {
                for (key, desc) in &section.items {
                    help_lines.push(Line::from(vec![
                        Span::styled(
                            format!("    {:<20}  ", key),
                            Style::default().fg(theme::fg_color()),
                        ),
                        Span::styled(*desc, Style::default().fg(theme::fg_dim())),
                    ]));
                }
            }
        }

        let help_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" KEY BINDINGS ");

        let total_lines = help_lines.len() as u16;
        let view_h = chunks[1].height.saturating_sub(2);
        let max_scroll = total_lines.saturating_sub(view_h);
        let scroll = self.scroll_offset.min(max_scroll);

        frame.render_widget(
            Paragraph::new(help_lines)
                .block(help_block)
                .scroll((scroll, 0)),
            chunks[1],
        );

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] navigate sections   [Enter] toggle   [?] or [Esc] close help",
            Style::default().fg(theme::fg_dim()),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_section + 1 < self.sections.len() {
                    self.selected_section += 1;
                    self.scroll_offset = 0;
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_section = self.selected_section.saturating_sub(1);
                self.scroll_offset = 0;
                true
            }
            KeyCode::Char('J') => {
                // Page down
                self.scroll_offset = (self.scroll_offset + 5).min(255);
                true
            }
            KeyCode::Char('K') => {
                // Page up
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                true
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.sections[self.selected_section].collapsed =
                    !self.sections[self.selected_section].collapsed;
                true
            }
            _ => false,
        }
    }
}

impl HelpPage {
    /// Handle a mouse click on the help page.
    pub fn handle_click(&mut self, _mouse_col: u16, mouse_row: u16, area: Rect) -> bool {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(5),    // content
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Check if click is in the content area
        if mouse_row < chunks[1].y || mouse_row >= chunks[1].y + chunks[1].height {
            return false;
        }

        // Calculate which section was clicked
        let content_row = mouse_row - chunks[1].y - 1; // Account for border
        let scroll = self.scroll_offset as usize;
        let absolute_row = content_row as usize + scroll;

        // Find which section this row belongs to
        let mut current_row = 0;
        for (i, section) in self.sections.iter_mut().enumerate() {
            if current_row == absolute_row {
                // Clicked on a section header
                self.selected_section = i;
                section.collapsed = !section.collapsed;
                return true;
            }
            current_row += 1; // Section header

            if !section.collapsed {
                current_row += section.items.len();
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NikiConfig;
    use std::path::PathBuf;

    fn test_state() -> AppState {
        AppState::new(
            "test task".to_string(),
            NikiConfig::default(),
            PathBuf::from("."),
        )
    }

    #[test]
    fn help_sections_exist() {
        let page = HelpPage::new();
        assert!(!page.sections.is_empty());
        assert!(page.sections.len() >= 4);
    }

    #[test]
    fn sections_start_expanded() {
        let page = HelpPage::new();
        for s in &page.sections {
            assert!(!s.collapsed);
        }
    }

    #[test]
    fn toggle_collapses_section() {
        let mut page = HelpPage::new();
        let idx = page.selected_section;
        page.sections[idx].collapsed = !page.sections[idx].collapsed;
        assert!(page.sections[idx].collapsed);
    }

    #[test]
    fn navigation_within_bounds() {
        let mut page = HelpPage::new();
        let max = page.sections.len();
        for _ in 0..(max + 5) {
            page.handle_key(KeyCode::Char('j').into(), &mut test_state());
        }
        assert!(page.selected_section < max);
        // Go back up
        for _ in 0..(max + 5) {
            page.handle_key(KeyCode::Char('k').into(), &mut test_state());
        }
        assert_eq!(page.selected_section, 0);
    }
}
