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
    selected_section: usize,
}

impl Default for ConfigPage {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigPage {
    /// How many fields the form has, derived from the form itself.
    ///
    /// Public so a test can assert the cycle has no dead stops, which is the
    /// only way to catch a count that has drifted from the list of fields.
    pub fn field_count(state: &AppState) -> usize {
        build_form(state).1.len()
    }

    pub fn new() -> Self {
        Self {
            selected_field: 0,
            selected_section: 0,
        }
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
                    .fg(theme::fg_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {}", state.project_path.display()),
                Style::default().fg(theme::fg_dim()),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Config form — focus ring on active section
        let form_border = if self.selected_section == 0 {
            Style::default().fg(theme::border_active())
        } else {
            Style::default().fg(theme::border_color())
        };
        let form_block = Block::default()
            .borders(Borders::ALL)
            .border_style(form_border)
            .title(format!(
                " {} ",
                if self.selected_section == 0 {
                    "▸ niki.toml "
                } else {
                    " niki.toml "
                }
            ));

        let (mut form_lines, field_rows) = build_form(state);
        if let Some(row) = field_rows.get(self.selected_field).copied() {
            form_lines[row] = mark_selected(form_lines[row].clone());
        }
        frame.render_widget(Paragraph::new(form_lines).block(form_block), chunks[1]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [Tab] next field   [Esc] back",
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
            // Counted from the form that is actually drawn, not typed. The
            // hand-typed `15` was against 10 fields and a loop over the
            // agents, so `Tab` had five dead stops before it wrapped.
            KeyCode::Tab => {
                let n = Self::field_count(state).max(1);
                self.selected_field = (self.selected_field + 1) % n;
                true
            }
            KeyCode::BackTab => {
                let n = Self::field_count(state).max(1);
                self.selected_field = if self.selected_field == 0 {
                    n - 1
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

fn build_form(state: &AppState) -> (Vec<Line<'static>>, Vec<usize>) {
    let mut form_lines: Vec<Line<'static>> = Vec::new();
    let mut field_rows: Vec<usize> = Vec::new();

    // General section
    form_lines.push(Line::from(Span::styled(
        "  GENERAL",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    max_revision_rounds    ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!("[ {} ]", state.config.general.max_revision_rounds),
                Style::default().fg(theme::warning()),
            ),
        ]),
    );
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    output_dir             ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!("[ {} ]", state.config.general.output_dir),
                Style::default().fg(theme::warning()),
            ),
        ]),
    );
    form_lines.push(Line::from(""));

    // Agents section
    form_lines.push(Line::from(Span::styled(
        "  AGENTS",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));

    let agents = vec![
        ("Planner", &state.config.agents.planner),
        ("Coder", &state.config.agents.coder),
        ("Tester", &state.config.agents.tester),
        ("Reviewer", &state.config.agents.reviewer),
    ];

    for (name, agent) in &agents {
        push_field(
            &mut form_lines,
            &mut field_rows,
            Line::from(vec![
                Span::styled(
                    format!("    {:<10}", name),
                    Style::default()
                        .fg(theme::fg_color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("provider [ {:<10} ]", agent.provider),
                    Style::default().fg(theme::fg_color()),
                ),
                Span::styled(
                    format!("model [ {:<20} ]", agent.model),
                    Style::default().fg(theme::fg_color()),
                ),
            ]),
        );
    }
    form_lines.push(Line::from(""));

    // Sandbox section (Podman/Docker)
    form_lines.push(Line::from(Span::styled(
        "  SANDBOX",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    base_image       ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!("[ {} ]", state.config.docker.base_image),
                Style::default().fg(theme::warning()),
            ),
        ]),
    );
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    memory_limit     ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!("[ {} ]", state.config.docker.memory_limit),
                Style::default().fg(theme::warning()),
            ),
            Span::styled("   cpu_limit ", Style::default().fg(theme::fg_color())),
            Span::styled(
                format!("[ {} ]", state.config.docker.cpu_limit),
                Style::default().fg(theme::warning()),
            ),
        ]),
    );
    form_lines.push(Line::from(""));

    // Pipeline section
    form_lines.push(Line::from(Span::styled(
        "  PIPELINE",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    topology         ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!("[ {:?} ]", state.config.pipeline.topology),
                Style::default().fg(theme::warning()),
            ),
        ]),
    );
    form_lines.push(Line::from(""));

    // Security section
    form_lines.push(Line::from(Span::styled(
        "  SECURITY",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    enabled          ",
                Style::default().fg(theme::fg_color()),
            ),
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
                    theme::success()
                } else {
                    theme::fg_dim()
                }),
            ),
        ]),
    );
    form_lines.push(Line::from(""));

    // Parallel section
    form_lines.push(Line::from(Span::styled(
        "  PARALLEL",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    enabled          ",
                Style::default().fg(theme::fg_color()),
            ),
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
                    theme::success()
                } else {
                    theme::fg_dim()
                }),
            ),
            Span::styled("   coder_count ", Style::default().fg(theme::fg_color())),
            Span::styled(
                format!("[ {} ]", state.config.parallel.coder_count),
                Style::default().fg(theme::warning()),
            ),
        ]),
    );

    // Theme section
    form_lines.push(Line::from(Span::styled(
        "  THEME",
        Style::default()
            .fg(theme::BLUE())
            .add_modifier(Modifier::BOLD),
    )));
    let theme_name = match state.config.ui.theme {
        crate::config::types::ThemePreference::Auto => "auto",
        crate::config::types::ThemePreference::Dark => "dark",
        crate::config::types::ThemePreference::Light => "light",
    };
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    theme             ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!("[ {} ]", theme_name),
                Style::default().fg(theme::accent()),
            ),
        ]),
    );
    push_field(
        &mut form_lines,
        &mut field_rows,
        Line::from(vec![
            Span::styled(
                "    tips              ",
                Style::default().fg(theme::fg_color()),
            ),
            Span::styled(
                format!(
                    "[ {} ]",
                    if state.config.ui.tips.enabled {
                        "on"
                    } else {
                        "off"
                    }
                ),
                Style::default().fg(if state.config.ui.tips.enabled {
                    theme::success()
                } else {
                    theme::fg_dim()
                }),
            ),
        ]),
    );
    form_lines.push(Line::from(""));

    (form_lines, field_rows)
}

/// Push a form field, remembering which row it landed on.
fn push_field<'a>(lines: &mut Vec<Line<'a>>, field_rows: &mut Vec<usize>, line: Line<'a>) {
    field_rows.push(lines.len());
    lines.push(line);
}

/// Mark a row as the selected field, so a key that moves a cursor also moves
/// something the user can see.
fn mark_selected<'a>(line: Line<'a>) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = vec![Span::styled(
        " ▸ ",
        Style::default()
            .fg(theme::border_active())
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(line.spans);
    Line::from(spans)
}
