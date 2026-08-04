use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{AppState, Page, PageId};
use crate::display::theme;

pub struct PipelinePage {
    selected_stage: usize,
}

impl PipelinePage {
    pub fn new() -> Self {
        Self { selected_stage: 0 }
    }
}

impl Page for PipelinePage {
    fn title(&self) -> &str {
        "pipeline"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Length(1), // mode info
                Constraint::Min(5),    // flowchart
                Constraint::Length(3), // agent models
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![Span::styled(
            " pipeline",
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD),
        )]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Mode info
        let mode = if state.config.pipeline.stages.is_empty() {
            "default"
        } else {
            "user-defined"
        };
        let parallel = if state.config.parallel.enabled {
            format!("parallel({})", state.config.parallel.coder_count)
        } else {
            "off".to_string()
        };
        let security = if state.config.security.enabled {
            "on"
        } else {
            "off"
        };
        let info = Line::from(vec![
            Span::styled("  MODE ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                mode,
                Style::default().fg(theme::fg_color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   SECURITY ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                security,
                Style::default().fg(if state.config.security.enabled {
                    theme::GREEN()
                } else {
                    theme::fg_dim()
                }),
            ),
            Span::styled("   PARALLEL ", Style::default().fg(theme::fg_dim())),
            Span::styled(&parallel, Style::default().fg(theme::fg_color())),
        ]);
        frame.render_widget(Paragraph::new(info), chunks[1]);

        // ── Card grid (replaces ASCII flowchart) ──────────────────────────
        // Responsive: 4-up (>=104 cols), 2-up (>=80 cols), stacked (<80 cols)
        let primary_roles = [
            crate::artifacts::types::AgentRole::Planner,
            crate::artifacts::types::AgentRole::Coder,
            crate::artifacts::types::AgentRole::Tester,
            crate::artifacts::types::AgentRole::Reviewer,
        ];
        let agent_configs = [
            &state.config.agents.planner,
            &state.config.agents.coder,
            &state.config.agents.tester,
            &state.config.agents.reviewer,
        ];

        let role_descs = [
            "Breaks task into structured spec before code",
            "Implements changes in an isolated worktree",
            "Runs test suite and reports failures",
            "Independent verification of changes",
        ];

        let cols: usize = if area.width >= 104 {
            4
        } else if area.width >= 80 {
            2
        } else {
            1
        };

        // Count started stages to limit selection
        let started_roles: Vec<_> = state.stages.iter().map(|s| s.role).collect();
        let started_count = primary_roles
            .iter()
            .filter(|r| started_roles.contains(r))
            .count();
        let max_select = started_count.max(1).min(4);
        let selected = self.selected_stage.min(max_select - 1);

        // Compute card dimensions
        let card_gap: u16 = 2;
        let total_gap = card_gap * (cols as u16 - 1);
        let avail_width = area.width.saturating_sub(total_gap + 2);
        let card_width = (avail_width / cols as u16).max(20).min(40);
        let card_height: u16 = 5;
        let card_gap_v: u16 = 1;

        let total_cards = primary_roles.len() as u16;
        let rows = (total_cards + cols as u16 - 1) / cols as u16;
        let total_height = card_height * rows + card_gap_v * (rows - 1);
        let top_pad = (chunks[2].height.saturating_sub(total_height)) / 2;

        for (i, role) in primary_roles.iter().enumerate() {
            let col = (i % cols) as u16;
            let row = (i / cols) as u16;

            let x = 1 + col * (card_width + card_gap);
            let y = chunks[2].y + top_pad + row * (card_height + card_gap_v);

            if y + card_height > chunks[2].y + chunks[2].height {
                continue;
            }

            let card_area = Rect {
                x,
                y,
                width: card_width,
                height: card_height,
            };

            let is_selected = i == selected;
            let stage = state.stages.iter().find(|s| s.role == *role);
            let status_glyph = match stage {
                Some(s) => match s.status {
                    super::StageStatus::Running => "▶",
                    super::StageStatus::Done => "✓",
                    super::StageStatus::Failed => "✗",
                    super::StageStatus::Queued => "·",
                },
                None => "·",
            };
            let status_color = match stage {
                Some(s) => match s.status {
                    super::StageStatus::Running => theme::warning(),
                    super::StageStatus::Done => theme::success(),
                    super::StageStatus::Failed => theme::error(),
                    super::StageStatus::Queued => theme::fg_dim(),
                },
                None => theme::fg_dim(),
            };

            let border_style = if is_selected {
                Style::default().fg(theme::border_active())
            } else {
                Style::default().fg(theme::border_color())
            };

            let glyph = theme::role_glyph(*role);
            let name = theme::role_name(*role);
            let role_color = theme::role_color(*role);
            let cfg = agent_configs[i];
            let desc = role_descs[i];

            let desc_width = (card_width as usize).saturating_sub(6);
            let truncated_desc = theme::truncate_str(desc, desc_width);

            let card_lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {} {}", glyph, name),
                        Style::default()
                            .fg(role_color)
                            .add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(format!(" {} {}", status_glyph, status_glyph), Style::default().fg(status_color)),
                ]),
                Line::from(Span::styled(format!(" {}", truncated_desc), Style::default().fg(theme::fg_dim()))),
                Line::from(Span::styled(
                    format!(" provider: {}", cfg.provider),
                    Style::default().fg(theme::fg_color()),
                )),
            ];

            let card_block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .style(Style::default().bg(theme::bg_elevated()));

            frame.render_widget(
                Paragraph::new(card_lines).block(card_block),
                card_area,
            );
        }

        // Agent models — highlight selected stage
        let model_lines: Vec<Line> = primary_roles
            .iter()
            .enumerate()
            .map(|(i, role)| {
                let selected = i == self.selected_stage;
                let prefix = if selected { "▸" } else { " " };
                let model_style = if selected {
                    Style::default()
                        .fg(theme::fg_bright())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::fg_color())
                };
                let cfg = agent_configs[i];
                Line::from(vec![
                    Span::styled(
                        format!("{} ◈ {}   ", prefix, theme::role_name(*role)),
                        Style::default()
                            .fg(theme::role_color(*role))
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(format!("{}/{}", cfg.provider, cfg.model), model_style),
                ])
            })
            .collect();
        let model_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" MODELS ");
        frame.render_widget(Paragraph::new(model_lines).block(model_block), chunks[3]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] next/prev   [Esc] back",
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
             KeyCode::Char('j') | KeyCode::Down => {
                let primary_roles = [
                    crate::artifacts::types::AgentRole::Planner,
                    crate::artifacts::types::AgentRole::Coder,
                    crate::artifacts::types::AgentRole::Tester,
                    crate::artifacts::types::AgentRole::Reviewer,
                ];
                let started_roles: Vec<_> = state.stages.iter().map(|s| s.role).collect();
                let started_count = primary_roles
                    .iter()
                    .filter(|r| started_roles.contains(r))
                    .count();
                let max_index = started_count.max(1).min(4) - 1;
                if self.selected_stage < max_index {
                    self.selected_stage += 1;
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_stage = self.selected_stage.saturating_sub(1);
                true
            }
            KeyCode::Char('a') => {
                state.current_page = PageId::Agents;
                true
            }
            KeyCode::Char(',') => {
                state.current_page = PageId::Config;
                true
            }
            _ => false,
        }
    }
}
