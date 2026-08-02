use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{AppState, Page, PageId};
use crate::display::theme;

pub struct ConfigPage {
    selected_field: usize,
}

impl ConfigPage {
    pub fn new() -> Self {
        Self { selected_field: 0 }
    }
}

impl Page for ConfigPage {
    fn title(&self) -> &str {
        "config"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(5),    // config form
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(
                " config",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {}", state.project_path.display()),
                Style::default().fg(theme::FG_DIM),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Config form
        let form_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" niki.toml ");

        let mut form_lines: Vec<Line> = Vec::new();

        // General section
        form_lines.push(Line::from(Span::styled(
            "  GENERAL",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        form_lines.push(Line::from(vec![
            Span::styled(
                "    max_revision_rounds    ",
                Style::default().fg(theme::FG),
            ),
            Span::styled(
                format!("[ {} ]", state.config.general.max_revision_rounds),
                Style::default().fg(theme::AMBER),
            ),
        ]));
        form_lines.push(Line::from(vec![
            Span::styled(
                "    output_dir             ",
                Style::default().fg(theme::FG),
            ),
            Span::styled(
                format!("[ {} ]", state.config.general.output_dir),
                Style::default().fg(theme::AMBER),
            ),
        ]));
        form_lines.push(Line::from(""));

        // Agents section
        form_lines.push(Line::from(Span::styled(
            "  AGENTS",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )));

        let agents = vec![
            ("Planner", &state.config.agents.planner),
            ("Coder", &state.config.agents.coder),
            ("Tester", &state.config.agents.tester),
            ("Reviewer", &state.config.agents.reviewer),
        ];

        for (name, agent) in &agents {
            form_lines.push(Line::from(vec![
                Span::styled(
                    format!("    {:<10}", name),
                    Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("provider [ {:<10} ]", agent.provider),
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    format!("model [ {:<20} ]", agent.model),
                    Style::default().fg(theme::FG),
                ),
            ]));
        }
        form_lines.push(Line::from(""));

        // Sandbox section (Podman/Docker)
        form_lines.push(Line::from(Span::styled(
            "  SANDBOX",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        form_lines.push(Line::from(vec![
            Span::styled("    base_image       ", Style::default().fg(theme::FG)),
            Span::styled(
                format!("[ {} ]", state.config.docker.base_image),
                Style::default().fg(theme::AMBER),
            ),
        ]));
        form_lines.push(Line::from(vec![
            Span::styled("    memory_limit     ", Style::default().fg(theme::FG)),
            Span::styled(
                format!("[ {} ]", state.config.docker.memory_limit),
                Style::default().fg(theme::AMBER),
            ),
            Span::styled("   cpu_limit ", Style::default().fg(theme::FG)),
            Span::styled(
                format!("[ {} ]", state.config.docker.cpu_limit),
                Style::default().fg(theme::AMBER),
            ),
        ]));
        form_lines.push(Line::from(""));

        // Pipeline section
        form_lines.push(Line::from(Span::styled(
            "  PIPELINE",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        form_lines.push(Line::from(vec![
            Span::styled("    topology         ", Style::default().fg(theme::FG)),
            Span::styled(
                format!("[ {:?} ]", state.config.pipeline.topology),
                Style::default().fg(theme::AMBER),
            ),
        ]));
        form_lines.push(Line::from(""));

        // Security section
        form_lines.push(Line::from(Span::styled(
            "  SECURITY",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        form_lines.push(Line::from(vec![
            Span::styled("    enabled          ", Style::default().fg(theme::FG)),
            Span::styled(
                format!(
                    "[ {} ]",
                    if state.config.security.enabled {
                        "x"
                    } else {
                        " "
                    }
                ),
                Style::default().fg(if state.config.security.enabled {
                    theme::GREEN
                } else {
                    theme::FG_DIM
                }),
            ),
        ]));
        form_lines.push(Line::from(""));

        // Parallel section
        form_lines.push(Line::from(Span::styled(
            "  PARALLEL",
            Style::default()
                .fg(theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        form_lines.push(Line::from(vec![
            Span::styled("    enabled          ", Style::default().fg(theme::FG)),
            Span::styled(
                format!(
                    "[ {} ]",
                    if state.config.parallel.enabled {
                        "x"
                    } else {
                        " "
                    }
                ),
                Style::default().fg(if state.config.parallel.enabled {
                    theme::GREEN
                } else {
                    theme::FG_DIM
                }),
            ),
            Span::styled("   coder_count ", Style::default().fg(theme::FG)),
            Span::styled(
                format!("[ {} ]", state.config.parallel.coder_count),
                Style::default().fg(theme::AMBER),
            ),
        ]));

        frame.render_widget(Paragraph::new(form_lines).block(form_block), chunks[1]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [Tab] next field   [Esc] back",
            Style::default().fg(theme::FG_DIM),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Tab => {
                self.selected_field = (self.selected_field + 1) % 12;
                true
            }
            KeyCode::BackTab => {
                self.selected_field = if self.selected_field == 0 {
                    11
                } else {
                    self.selected_field - 1
                };
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
