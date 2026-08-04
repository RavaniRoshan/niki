use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Wrap};

use super::{AppState, Page, PageId, StageStatus};
use crate::display::theme;

fn fmt_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn fmt_duration(ms: u64) -> String {
    let secs = ms / 1000;
    format!("{}.{:01}s", secs, (ms % 1000) / 100)
}

pub struct AgentsPage {
    selected_tab: usize,
    scroll_offset: u16,
}

impl AgentsPage {
    pub fn new() -> Self {
        Self {
            selected_tab: 0,
            scroll_offset: 0,
        }
    }
}

impl Page for AgentsPage {
    fn title(&self) -> &str {
        "agents"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Length(1), // tabs
                Constraint::Length(3), // metadata
                Constraint::Min(3),    // transcript
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![Span::styled(
            " agents",
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD),
        )]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Tabs
        let tab_titles: Vec<Line> = state
            .stages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let color = if i == self.selected_tab {
                    theme::border_active()
                } else {
                    theme::fg_dim()
                };
                Line::from(vec![Span::styled(
                    format!("{} {}", theme::role_glyph(s.role), theme::role_name(s.role)),
                    Style::default()
                        .fg(color)
                        .add_modifier(if i == self.selected_tab {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                )])
            })
            .collect();

        if !tab_titles.is_empty() {
            let tabs = Tabs::new(tab_titles)
                .select(self.selected_tab)
                .highlight_style(
                    Style::default()
                        .fg(theme::BLUE())
                        .add_modifier(Modifier::BOLD),
                )
                .divider("|");
            frame.render_widget(tabs, chunks[1]);
        } else {
            frame.render_widget(
                Paragraph::new("  No agents have started yet")
                    .style(Style::default().fg(theme::fg_dim())),
                chunks[1],
            );
        }

        // Metadata
        if let Some(stage) = state.stages.get(self.selected_tab) {
            let glyph = theme::role_glyph(stage.role);
            let name = theme::role_name(stage.role);
            let color = theme::role_color(stage.role);
            let elapsed = stage
                .start
                .map(|s| {
                    let ms = s.elapsed().as_millis() as u64;
                    fmt_duration(ms)
                })
                .unwrap_or_else(|| "?".to_string());

            let meta = Line::from(vec![
                Span::styled(
                    format!("  {} {} ", glyph, name),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "· {} ",
                        match stage.status {
                            StageStatus::Running => "running",
                            StageStatus::Done => "done",
                            StageStatus::Failed => "failed",
                            StageStatus::Queued => "queued",
                        }
                    ),
                    Style::default().fg(match stage.status {
                        StageStatus::Running => color,
                        StageStatus::Done => theme::GREEN(),
                        StageStatus::Failed => theme::RED(),
                        StageStatus::Queued => theme::fg_dim(),
                    }),
                ),
                Span::styled(
                    format!("· {} ", elapsed),
                    Style::default().fg(theme::fg_dim()),
                ),
                Span::styled(
                    format!("· in:{} ", fmt_tokens(stage.input_tokens)),
                    Style::default().fg(theme::fg_dim()),
                ),
                Span::styled(
                    format!("out:{} ", fmt_tokens(stage.output_tokens)),
                    Style::default().fg(theme::fg_dim()),
                ),
                Span::styled(
                    format!("· ${:.4}", stage.cost_usd),
                    Style::default().fg(theme::fg_dim()),
                ),
            ]);
            frame.render_widget(Paragraph::new(meta), chunks[2]);
        } else {
            frame.render_widget(Paragraph::new(""), chunks[2]);
        }

        // Transcript
        let transcript_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" TRANSCRIPT ");

        if let Some(stage) = state.stages.get(self.selected_tab) {
            let mut lines: Vec<Line> = Vec::new();

            // Show summary if done
            if stage.status == StageStatus::Done && !stage.summary.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "  Summary:",
                    Style::default()
                        .fg(theme::fg_dim())
                        .add_modifier(Modifier::BOLD),
                )]));
                for summ in &stage.summary {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(summ.clone(), Style::default().fg(theme::fg_color())),
                    ]));
                }
                lines.push(Line::from(""));
            }

            // Show stream content
            if !stage.full_transcript.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "  Output:",
                    Style::default()
                        .fg(theme::fg_dim())
                        .add_modifier(Modifier::BOLD),
                )]));
                for line in stage.full_transcript.lines() {
                    let style = if line.starts_with('+') {
                        Style::default().fg(theme::GREEN())
                    } else if line.starts_with('-') {
                        Style::default().fg(theme::RED())
                    } else {
                        Style::default().fg(theme::fg_color())
                    };
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(line.to_string(), style),
                    ]));
                }
            } else if stage.status == StageStatus::Running && !stage.stream.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "  Live:",
                    Style::default()
                        .fg(theme::fg_dim())
                        .add_modifier(Modifier::BOLD),
                )]));
                let tail: String = stage
                    .stream
                    .lines()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                for line in tail.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(line.to_string(), Style::default().fg(theme::fg_color())),
                    ]));
                }
            } else if stage.status == StageStatus::Failed {
                lines.push(Line::from(vec![Span::styled(
                    "  Error:",
                    Style::default().fg(theme::RED()).add_modifier(Modifier::BOLD),
                )]));
                for summ in &stage.summary {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(summ.clone(), Style::default().fg(theme::RED())),
                    ]));
                }
            }

            let total_lines = lines.len() as u16;
            let view_h = chunks[3].height.saturating_sub(2);
            let max_scroll = total_lines.saturating_sub(view_h);
            let scroll = self.scroll_offset.min(max_scroll);

            frame.render_widget(
                Paragraph::new(lines)
                    .block(transcript_block)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                chunks[3],
            );
        } else {
            frame.render_widget(
                Paragraph::new("  Select an agent tab to view transcript")
                    .block(transcript_block)
                    .style(Style::default().fg(theme::fg_dim())),
                chunks[3],
            );
        }

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [Tab/Shift+Tab] prev/next   [Esc] back",
            Style::default().fg(theme::fg_dim()),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[4]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Tab => {
                if !state.stages.is_empty() {
                    self.selected_tab = (self.selected_tab + 1) % state.stages.len();
                    self.scroll_offset = 0;
                }
                true
            }
            KeyCode::BackTab => {
                if !state.stages.is_empty() {
                    self.selected_tab = if self.selected_tab == 0 {
                        state.stages.len() - 1
                    } else {
                        self.selected_tab - 1
                    };
                    self.scroll_offset = 0;
                }
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
            _ => false,
        }
    }
}
