use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::display::theme;
use super::{AppState, Page, PageId, StageStatus};

pub struct RunPage {
    scroll_offset: u16,
}

impl RunPage {
    pub fn new() -> Self {
        Self { scroll_offset: 0 }
    }
}

impl Page for RunPage {
    fn title(&self) -> &str {
        "home"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            let msg = Paragraph::new("Terminal too small").style(Style::default().fg(theme::RED));
            frame.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // task card
                Constraint::Length(1), // command line
                Constraint::Min(4),   // pipeline output
                Constraint::Length(1), // separator
                Constraint::Length(2), // results + working tree
                Constraint::Length(1), // footer
            ])
            .split(area);

        // ── Task card (left accent bar + description + tags) ──────────────
        let task_card_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1), // accent bar
                Constraint::Min(1),   // content
            ])
            .split(chunks[0]);

        // Left accent bar (green vertical line)
        let accent_bar = Paragraph::new("│").style(Style::default().fg(theme::GREEN));
        frame.render_widget(accent_bar, task_card_chunks[0]);

        // Task card content
        let desc = &state.description;
        let desc_width = (task_card_chunks[1].width as usize).saturating_sub(20);
        let truncated: String = desc.chars().take(desc_width).collect();

        let card_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title + description
                Constraint::Length(1), // tags
            ])
            .split(task_card_chunks[1]);

        // Title line: "Build" + description + paused indicator
        let mut title_spans = vec![
            Span::styled(
                "Build ",
                Style::default().fg(theme::FG_BRIGHT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncated,
                Style::default().fg(theme::FG),
            ),
        ];
        if state.paused {
            title_spans.push(Span::styled(
                "  ■ PAUSED",
                Style::default().fg(theme::AMBER).add_modifier(Modifier::BOLD),
            ));
        }
        let title_line = Line::from(title_spans);
        frame.render_widget(Paragraph::new(title_line), card_chunks[0]);

        // Tags line
        let tags = if state.stages.is_empty() {
            vec!["sandbox", "docker"]
        } else {
            vec!["sandbox", "docker"]
        };
        let mut tag_spans = vec![Span::styled("  ", Style::default())];
        for tag in &tags {
            tag_spans.push(Span::styled(
                format!(" {} ", tag),
                Style::default()
                    .fg(theme::FG_DIM)
                    .add_modifier(Modifier::BOLD),
            ));
            tag_spans.push(Span::styled(" ", Style::default()));
        }
        frame.render_widget(Paragraph::new(Line::from(tag_spans)), card_chunks[1]);

        // ── Command line ──────────────────────────────────────────────────
        let cmd_line = Line::from(vec![
            Span::styled("$ ", Style::default().fg(theme::FG_DIM)),
            Span::styled(
                format!("niki run \"{}\"", state.description),
                Style::default().fg(theme::FG),
            ),
            Span::styled(
                " --project ./my-app",
                Style::default().fg(theme::FG_DIM),
            ),
        ]);
        frame.render_widget(Paragraph::new(cmd_line), chunks[1]);

        // ── Pipeline output ───────────────────────────────────────────────
        let mut pipeline_lines: Vec<Line> = Vec::new();

        for stage in &state.stages {
            let color = theme::role_color(stage.role);
            let name = theme::role_name(stage.role);
            let glyph = theme::role_glyph(stage.role);

            let (status_text, status_color) = match stage.status {
                StageStatus::Running => {
                    let tail = stage.stream.lines().rev().find(|l| !l.trim().is_empty());
                    let detail = tail.unwrap_or("working...").chars().take(60).collect::<String>();
                    (detail, color)
                }
                StageStatus::Done => {
                    let summary = stage.summary.first().map(|s| s.as_str()).unwrap_or("done");
                    (summary.to_string(), theme::GREEN)
                }
                StageStatus::Failed => {
                    let summary = stage.summary.first().map(|s| s.as_str()).unwrap_or("failed");
                    (summary.to_string(), theme::RED)
                }
                StageStatus::Queued => ("queued".to_string(), theme::FG_DIM),
            };

            pipeline_lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", glyph),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    name.to_string(),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", status_text),
                    Style::default().fg(status_color),
                ),
            ]));

            // Show files line for Planner stage (like reference)
            if stage.role == crate::artifacts::types::AgentRole::Planner
                && stage.status == StageStatus::Done
            {
                pipeline_lines.push(Line::from(vec![
                    Span::styled(
                        "  files: ",
                        Style::default().fg(theme::FG_DIM),
                    ),
                    Span::styled(
                        "src/routes/health.ts · tests/health.test.ts",
                        Style::default().fg(theme::FG_DIM),
                    ),
                ]));
            }
        }

        // Add queued roles that haven't started yet
        let started_roles: Vec<_> = state.stages.iter().map(|s| s.role).collect();
        let primary_roles = [
            crate::artifacts::types::AgentRole::Planner,
            crate::artifacts::types::AgentRole::Coder,
            crate::artifacts::types::AgentRole::Tester,
            crate::artifacts::types::AgentRole::Reviewer,
        ];
        for role in &primary_roles {
            if !started_roles.contains(role) {
                pipeline_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", theme::role_glyph(*role)),
                        Style::default().fg(theme::FG_DIM),
                    ),
                    Span::styled(
                        theme::role_name(*role).to_string(),
                        Style::default().fg(theme::FG_DIM),
                    ),
                    Span::styled(" · queued", Style::default().fg(theme::FG_DIM)),
                ]));
            }
        }

        // Scroll support
        let total_lines = pipeline_lines.len() as u16;
        let view_h = chunks[2].height;
        let max_scroll = total_lines.saturating_sub(view_h);
        let scroll = self.scroll_offset.min(max_scroll);

        frame.render_widget(
            Paragraph::new(pipeline_lines).scroll((scroll, 0)),
            chunks[2],
        );

        // ── Separator ─────────────────────────────────────────────────────
        let separator = Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::BORDER_DIM),
        ));
        frame.render_widget(Paragraph::new(separator), chunks[3]);

        // ── Results ───────────────────────────────────────────────────────
        let branch_str = if state.branch_name.is_empty() {
            "niki/xxxxx".to_string()
        } else {
            state.branch_name.clone()
        };

        let result_spans = vec![
            Span::styled("branch ", Style::default().fg(theme::FG)),
            Span::styled(
                branch_str.clone(),
                Style::default().fg(theme::FG_BRIGHT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · report.md · changes.patch", Style::default().fg(theme::FG)),
        ];
        frame.render_widget(Paragraph::new(Line::from(result_spans)), chunks[4]);

        // Working tree line
        let working_tree = Line::from(vec![
            Span::styled("working tree: ", Style::default().fg(theme::FG_DIM)),
            Span::styled("untouched", Style::default().fg(theme::GREEN)),
        ]);
        // Render working tree in the same area, offset down
        let wt_area = Rect {
            x: chunks[4].x,
            y: chunks[4].y + 1,
            width: chunks[4].width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(working_tree), wt_area);

        // ── Footer ────────────────────────────────────────────────────────
        let footer = Line::from(vec![
            Span::styled("space ", Style::default().fg(theme::FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled("pause  ", Style::default().fg(theme::FG_DIM)),
            Span::styled("j/k ", Style::default().fg(theme::FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled("scroll  ", Style::default().fg(theme::FG_DIM)),
            Span::styled("q ", Style::default().fg(theme::FG_BRIGHT).add_modifier(Modifier::BOLD)),
            Span::styled("quit", Style::default().fg(theme::FG_DIM)),
        ]);
        frame.render_widget(Paragraph::new(footer), chunks[5]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                state.modal = Some(super::Modal::Confirm {
                    title: "Quit NIKI?".to_string(),
                    message: "The pipeline will continue in the background.".to_string(),
                });
                true
            }
            KeyCode::Char('p') => { state.current_page = PageId::Pipeline; true }
            KeyCode::Char('a') => { state.current_page = PageId::Agents; true }
            KeyCode::Char('d') => { state.current_page = PageId::Diff; true }
            KeyCode::Char('v') => { state.current_page = PageId::Verdict; true }
            KeyCode::Char('c') => { state.current_page = PageId::Cost; true }
            KeyCode::Char('f') => { state.current_page = PageId::Artifacts; true }
            KeyCode::Char('h') => { state.current_page = PageId::History; true }
            KeyCode::Char('?') => { state.current_page = PageId::Help; true }
            KeyCode::Char(' ') => {
                state.paused = !state.paused;
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
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                true
            }
            _ => false,
        }
    }
}
