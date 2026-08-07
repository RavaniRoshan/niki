use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{AppState, Page, PageId, RunState};
use crate::display::theme;

pub struct VerdictPage {
    scroll_offset: u16,
}

impl Default for VerdictPage {
    fn default() -> Self {
        Self::new()
    }
}

impl VerdictPage {
    pub fn new() -> Self {
        Self { scroll_offset: 0 }
    }
}

impl Page for VerdictPage {
    fn title(&self) -> &str {
        "verdict"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 6 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Length(5), // verdict tile
                Constraint::Min(3),    // report content
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(
                " verdict",
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

        // Verdict tile
        let (verdict_text, verdict_color) = match state.run_state {
            RunState::AwaitingApproval => ("A P P R O V E D", theme::GREEN()),
            RunState::Failed => ("F A I L E D", theme::RED()),
            RunState::Running => ("I N   P R O G R E S S", theme::AMBER()),
            RunState::Cancelled => ("C A N C E L L E D", theme::fg_dim()),
            _ => ("N O   V E R D I C T", theme::fg_dim()),
        };

        let verdict_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(verdict_color))
            .title(" VERDICT ");

        let mut verdict_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("   ◆  {}", verdict_text),
                Style::default()
                    .fg(verdict_color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        // Add scores if available
        if let Some(review_stage) = state.stages.iter().rev().find(|s| {
            s.role == crate::artifacts::types::AgentRole::Reviewer
                && s.status == super::StageStatus::Done
        })
            && let Some(score_line) = review_stage.summary.first() {
                verdict_lines.push(Line::from(Span::styled(
                    format!("   {}", score_line),
                    Style::default().fg(theme::fg_color()),
                )));
            }

        frame.render_widget(
            Paragraph::new(verdict_lines).block(verdict_block),
            chunks[1],
        );

        // Report content
        let report_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" REPORT ");

        if let Some(report) = &state.report_content {
            let mut lines: Vec<Line> = Vec::new();
            for line in report.lines() {
                let style = if line.starts_with("#") {
                    Style::default()
                        .fg(theme::BLUE())
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("##") {
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD)
                } else if line.starts_with("-") || line.starts_with("*") {
                    Style::default().fg(theme::fg_color())
                } else if line.contains("$") {
                    Style::default().fg(theme::GREEN())
                } else {
                    Style::default().fg(theme::fg_color())
                };
                lines.push(Line::from(Span::styled(line.to_string(), style)));
            }

            let total_lines = lines.len() as u16;
            let view_h = chunks[2].height.saturating_sub(2);
            let max_scroll = total_lines.saturating_sub(view_h);
            let scroll = self.scroll_offset.min(max_scroll);

            frame.render_widget(
                Paragraph::new(lines)
                    .block(report_block)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                chunks[2],
            );
        } else {
            frame.render_widget(
                Paragraph::new("  Report will appear after pipeline completes")
                    .block(report_block)
                    .style(Style::default().fg(theme::fg_dim())),
                chunks[2],
            );
        }

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] scroll   [g/G] top/bottom   [d]iff   [Esc] back",
            Style::default().fg(theme::fg_dim()),
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
            KeyCode::Char('d') => {
                state.current_page = PageId::Diff;
                true
            }
            KeyCode::Char('c') => {
                state.current_page = PageId::Cost;
                true
            }
            _ => false,
        }
    }
}
