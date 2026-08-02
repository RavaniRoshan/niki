use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

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
                .fg(theme::BLUE)
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
            Span::styled("  MODE ", Style::default().fg(theme::FG_DIM)),
            Span::styled(
                mode,
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   SECURITY ", Style::default().fg(theme::FG_DIM)),
            Span::styled(
                security,
                Style::default().fg(if state.config.security.enabled {
                    theme::GREEN
                } else {
                    theme::FG_DIM
                }),
            ),
            Span::styled("   PARALLEL ", Style::default().fg(theme::FG_DIM)),
            Span::styled(&parallel, Style::default().fg(theme::FG)),
        ]);
        frame.render_widget(Paragraph::new(info), chunks[1]);

        // Flowchart
        let mut flow_lines: Vec<Line> = Vec::new();
        flow_lines.push(Line::from(""));
        flow_lines.push(Line::from(vec![
            Span::styled("         task ────▶ ", Style::default().fg(theme::FG_DIM)),
            Span::styled(
                format!(
                    "{} Planner",
                    theme::role_glyph(crate::artifacts::types::AgentRole::Planner)
                ),
                Style::default()
                    .fg(theme::role_color(
                        crate::artifacts::types::AgentRole::Planner,
                    ))
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                       │",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                  TaskSpec",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                       ▼",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "              ┌────────────┐",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![
            Span::styled("              │", Style::default().fg(theme::FG_DIM)),
            Span::styled(
                format!(
                    " {} Coder ",
                    theme::role_glyph(crate::artifacts::types::AgentRole::Coder)
                ),
                Style::default()
                    .fg(theme::role_color(crate::artifacts::types::AgentRole::Coder))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("│ ──diff──▶", Style::default().fg(theme::FG_DIM)),
        ]));
        flow_lines.push(Line::from(vec![Span::styled(
            "              └────────────┘           ▼",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                                 ┌────────────┐",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![
            Span::styled(
                "                                 │",
                Style::default().fg(theme::FG_DIM),
            ),
            Span::styled(
                format!(
                    " {} Tester ",
                    theme::role_glyph(crate::artifacts::types::AgentRole::Tester)
                ),
                Style::default()
                    .fg(theme::role_color(
                        crate::artifacts::types::AgentRole::Tester,
                    ))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("│ ──tests──▶", Style::default().fg(theme::FG_DIM)),
        ]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                                 └────────────┘           ▼",
            Style::default().fg(theme::FG_DIM),
        )]));

        if state.config.red_blue.enabled {
            flow_lines.push(Line::from(vec![Span::styled(
                "                                            ┌────────────┐",
                Style::default().fg(theme::FG_DIM),
            )]));
            flow_lines.push(Line::from(vec![
                Span::styled(
                    "                                            │",
                    Style::default().fg(theme::FG_DIM),
                ),
                Span::styled(
                    format!(
                        " {} Red ",
                        theme::role_glyph(crate::artifacts::types::AgentRole::Red)
                    ),
                    Style::default()
                        .fg(theme::role_color(crate::artifacts::types::AgentRole::Red))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("│ ──challenges──▶", Style::default().fg(theme::FG_DIM)),
            ]));
            flow_lines.push(Line::from(vec![Span::styled(
                "                                            └────────────┘           ▼",
                Style::default().fg(theme::FG_DIM),
            )]));
        }

        flow_lines.push(Line::from(vec![Span::styled(
            "                                       ┌────────────┐",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![
            Span::styled(
                "                                       │",
                Style::default().fg(theme::FG_DIM),
            ),
            Span::styled(
                format!(
                    " {} Reviewer ",
                    theme::role_glyph(crate::artifacts::types::AgentRole::Reviewer)
                ),
                Style::default()
                    .fg(theme::role_color(
                        crate::artifacts::types::AgentRole::Reviewer,
                    ))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("│", Style::default().fg(theme::FG_DIM)),
        ]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                                       └─────┬──────┘",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                                             │",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                                             ▼",
            Style::default().fg(theme::FG_DIM),
        )]));
        flow_lines.push(Line::from(vec![Span::styled(
            "                                       niki/<id> branch",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::BOLD),
        )]));

        let flow_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" FLOW ");
        frame.render_widget(
            Paragraph::new(flow_lines)
                .block(flow_block)
                .wrap(Wrap { trim: false }),
            chunks[2],
        );

        // Agent models — highlight selected stage
        let roles = [
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
        let model_lines: Vec<Line> = roles
            .iter()
            .enumerate()
            .map(|(i, role)| {
                let selected = i == self.selected_stage;
                let prefix = if selected { "▸" } else { " " };
                let model_style = if selected {
                    Style::default()
                        .fg(theme::FG_BRIGHT)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::FG)
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
            .border_style(Style::default().fg(theme::BORDER))
            .title(" MODELS ");
        frame.render_widget(Paragraph::new(model_lines).block(model_block), chunks[3]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] next/prev   [Esc] back",
            Style::default().fg(theme::FG_DIM),
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
                self.selected_stage = (self.selected_stage + 1).min(3);
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
