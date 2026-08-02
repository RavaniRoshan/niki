use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{AppState, Page, PageId};
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

pub struct CostPage {
    scroll_offset: u16,
}

impl CostPage {
    pub fn new() -> Self {
        Self { scroll_offset: 0 }
    }
}

impl Page for CostPage {
    fn title(&self) -> &str {
        "cost"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Length(1), // separator
                Constraint::Min(5),    // cost table
                Constraint::Length(1), // separator
                Constraint::Length(8), // bar chart
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(
                " cost",
                Style::default()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if state.branch_name.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", state.branch_name)
                },
                Style::default().fg(theme::FG_DIM),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), chunks[0]);

        // Separator
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  ──────────────────────────────────────────────────────────────────────",
                Style::default().fg(theme::BORDER),
            )),
            chunks[1],
        );

        // Cost table
        let table_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" COST BREAKDOWN ");

        let mut table_lines: Vec<Line> = Vec::new();
        // Header row
        table_lines.push(Line::from(vec![
            Span::styled(
                "  AGENT       ",
                Style::default()
                    .fg(theme::FG_DIM)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "MODEL                 ",
                Style::default()
                    .fg(theme::FG_DIM)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "TOKENS   ",
                Style::default()
                    .fg(theme::FG_DIM)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "COST     ",
                Style::default()
                    .fg(theme::FG_DIM)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "LATENCY",
                Style::default()
                    .fg(theme::FG_DIM)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        table_lines.push(Line::from(Span::styled(
            "  ──────────  ────────────────────  ───────  ────────  ────────",
            Style::default().fg(theme::BORDER),
        )));

        // Agent rows
        let mut total_tokens = 0u32;
        let mut total_cost = 0.0f64;
        let mut total_latency = 0u64;

        for stage in &state.stages {
            let glyph = theme::role_glyph(stage.role);
            let name = theme::role_name(stage.role);
            let color = theme::role_color(stage.role);
            let tokens = stage.input_tokens + stage.output_tokens;
            total_tokens += tokens;
            total_cost += stage.cost_usd;
            total_latency += stage.latency_ms;

            table_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} {:<9}", glyph, name),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<20}", "anthropic/claude-sonnet-4"),
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    format!("{:<8}", fmt_tokens(tokens)),
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    format!("${:<7.4}", stage.cost_usd),
                    Style::default().fg(theme::GREEN),
                ),
                Span::styled(
                    fmt_duration(stage.latency_ms),
                    Style::default().fg(theme::FG),
                ),
            ]));
        }

        // Total row
        table_lines.push(Line::from(Span::styled(
            "  ──────────  ────────────────────  ───────  ────────  ────────",
            Style::default().fg(theme::BORDER),
        )));
        table_lines.push(Line::from(vec![
            Span::styled(
                "  TOTAL      ",
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled("                     ", Style::default()),
            Span::styled(
                format!("{:<8}", fmt_tokens(total_tokens)),
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("${:<7.4}", total_cost),
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                fmt_duration(total_latency),
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
        ]));

        frame.render_widget(
            Paragraph::new(table_lines)
                .block(table_block)
                .wrap(Wrap { trim: false }),
            chunks[2],
        );

        // Separator
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  ──────────────────────────────────────────────────────────────────────",
                Style::default().fg(theme::BORDER),
            )),
            chunks[3],
        );

        // Bar chart
        let bar_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(" PER-AGENT COST ");

        let mut bar_lines: Vec<Line> = Vec::new();
        let max_cost = state
            .stages
            .iter()
            .map(|s| s.cost_usd)
            .fold(0.0f64, f64::max);

        for stage in &state.stages {
            let glyph = theme::role_glyph(stage.role);
            let name = theme::role_name(stage.role);
            let color = theme::role_color(stage.role);
            let bar_width = if max_cost > 0.0 {
                ((stage.cost_usd / max_cost) * 20.0) as usize
            } else {
                0
            };
            let bar: String = "█".repeat(bar_width);
            let empty: String = "░".repeat(20 - bar_width);

            bar_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} {:<9}", glyph, name),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(bar, Style::default().fg(color)),
                Span::styled(empty, Style::default().fg(theme::FG_DIM)),
                Span::styled(
                    format!(" ${:.4}", stage.cost_usd),
                    Style::default().fg(theme::FG),
                ),
            ]));
        }

        frame.render_widget(Paragraph::new(bar_lines).block(bar_block), chunks[4]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [Esc] back",
            Style::default().fg(theme::FG_DIM),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[5]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Char('v') => {
                state.current_page = PageId::Verdict;
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
            _ => false,
        }
    }
}
