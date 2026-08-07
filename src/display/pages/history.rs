use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{AppState, Page, PageId};
use crate::display::theme;
use crate::orchestrator::state::TaskRecord;
use std::path::PathBuf;

pub struct HistoryPage {
    selected: usize,
}

struct HistoryEntry {
    id: String,
    task: String,
    verdict: String,
    when: String,
    verdict_color: ratatui::style::Color,
    branch: String,
}

fn load_history_entries(project_path: &std::path::Path) -> Vec<HistoryEntry> {
    let config = crate::config::NikiConfig::load(project_path).unwrap_or_default();
    let tasks_dir = project_path.join(&config.general.output_dir).join("tasks");

    let mut records: Vec<(PathBuf, TaskRecord)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Ok(content) = std::fs::read_to_string(path.join("task.json"))
                    && let Ok(record) = serde_json::from_str::<TaskRecord>(&content) {
                        records.push((path, record));
                    }
        }
    }

    // Sort newest first
    records.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

    records
        .into_iter()
        .take(20)
        .map(|(_, record)| {
            let id = record.task_id.to_string();
            let id_short = &id[..id.len().min(8)];
            let task = record.description.clone();
            let (verdict, verdict_color) = match record.status {
                crate::orchestrator::state::TaskStatus::Completed => {
                    ("approved".to_string(), theme::success())
                }
                crate::orchestrator::state::TaskStatus::Failed { .. } => {
                    ("failed".to_string(), theme::error())
                }
                crate::orchestrator::state::TaskStatus::Running => {
                    ("running".to_string(), theme::warning())
                }
                crate::orchestrator::state::TaskStatus::Cancelled => {
                    ("cancelled".to_string(), theme::fg_dim())
                }
            };
            let when = format_time_ago(record.created_at);
            let branch = record.branch.clone().unwrap_or_default();

            HistoryEntry {
                id: id_short.to_string(),
                task,
                verdict,
                when,
                verdict_color,
                branch,
            }
        })
        .collect()
}

fn format_time_ago(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let elapsed = (now - dt).num_seconds();
    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86400)
    }
}

impl Default for HistoryPage {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryPage {
    pub fn new() -> Self {
        Self {
            selected: 0,
        }
    }
}

impl Page for HistoryPage {
    fn title(&self) -> &str {
        "history"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            return;
        }

        let entries = load_history_entries(&state.project_path);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(5),    // history table
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(
                " history",
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

        // History table
        let table_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" PAST RUNS ");

        let mut table_lines: Vec<Line> = Vec::new();

        // Header row
        table_lines.push(Line::from(vec![
            Span::styled(
                "  ID         ",
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "TASK                              ",
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "VERDICT     ",
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "WHEN       ",
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "BRNCH",
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        table_lines.push(Line::from(Span::styled(
            "  ────────   ──────────────────────────────────  ──────────  ───────  ───────",
            Style::default().fg(theme::border_color()),
        )));

        if entries.is_empty() {
            table_lines.push(Line::from(vec![
                Span::styled(
                    "  No past runs found in ",
                    Style::default().fg(theme::fg_dim()),
                ),
                Span::styled(
                    format!("{}", state.project_path.join(&state.config.general.output_dir).join("tasks").display()),
                    Style::default().fg(theme::fg_color()),
                ),
            ]));
        } else {
            for (i, entry) in entries.iter().enumerate() {
                let is_selected = i == self.selected;
                let style = if is_selected {
                    Style::default().fg(theme::fg_color()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::fg_color())
                };

                table_lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:<8}  ", entry.id),
                        Style::default().fg(theme::BLUE()),
                    ),
                    Span::styled(
                        format!("{:<32}  ", &entry.task[..entry.task.len().min(32)]),
                        style,
                    ),
                    Span::styled(
                        format!("{:<10}  ", entry.verdict),
                        Style::default().fg(entry.verdict_color),
                    ),
                    Span::styled(
                        format!("{:<9}  ", entry.when),
                        Style::default().fg(theme::fg_dim()),
                    ),
                    Span::styled(
                        format!("{:<6}", &entry.branch[..entry.branch.len().min(6)]),
                        if is_selected {
                            Style::default().fg(theme::fg_color()).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme::fg_dim())
                        },
                    ),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(table_lines).block(table_block), chunks[1]);

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] navigate   [Enter] open   [Esc] back",
            Style::default().fg(theme::fg_dim()),
        )]);
        frame.render_widget(Paragraph::new(footer), chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        let entries = load_history_entries(&state.project_path);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !entries.is_empty() {
                    self.selected = (self.selected + 1).min(entries.len() - 1);
                }
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                true
            }
            KeyCode::Enter => {
                if let Some(entry) = entries.get(self.selected) {
                    state.branch_name = entry.branch.clone();
                }
                state.current_page = PageId::Run;
                true
            }
            KeyCode::Char('f') => {
                state.current_page = PageId::Artifacts;
                true
            }
            KeyCode::Char('p') => {
                state.current_page = PageId::Pipeline;
                true
            }
            _ => false,
        }
    }
}
