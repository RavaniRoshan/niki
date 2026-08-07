use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::{AppState, Page, PageId};
use crate::display::theme;

pub struct ArtifactsPage {
    selected: usize,
}

impl Default for ArtifactsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactsPage {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Page for ArtifactsPage {
    fn title(&self) -> &str {
        "artifacts"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // header
                Constraint::Min(5),    // content (split left/right)
                Constraint::Length(1), // footer
            ])
            .split(area);

        // Header
        let header = Line::from(vec![
            Span::styled(
                " artifacts",
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

        // Content split
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[1]);

        // File tree (left)
        let tree_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .title(" FILES ");

        let mut tree_lines: Vec<Line> = Vec::new();
        let artifacts_dir = state
            .artifacts_dir
            .as_ref()
            .map(|p| p.display().to_string());
        let dir_name = artifacts_dir.as_deref().unwrap_or(".niki/<id>/");

        tree_lines.push(Line::from(Span::styled(
            format!("  {}/", dir_name),
            Style::default()
                .fg(theme::BLUE())
                .add_modifier(Modifier::BOLD),
        )));

        // Static file entries (representative of what NIKI produces)
        let entries = vec![
            ("├── report.md", "human-readable summary", true),
            ("├── changes.patch", "unified diff", true),
            ("├── cost.json", "per-agent cost breakdown", true),
            ("├── artifacts/", "", false),
            ("│   ├── 01_planner.json", "TaskSpec ✓", true),
            ("│   ├── 02_coder.json", "CodeDiff ✓", true),
            ("│   ├── 03_tester.json", "TestReport ✓", true),
            ("│   └── 04_reviewer.json", "ReviewVerdict ✓", true),
            ("└── logs/", "", false),
            ("    ├── planner.log", "", true),
            ("    ├── coder.log", "", true),
            ("    ├── tester.log", "", true),
            ("    └── reviewer.log", "", true),
        ];

        for (i, (name, desc, is_file)) in entries.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(theme::fg_color())
                    .add_modifier(Modifier::BOLD)
            } else if *is_file {
                Style::default().fg(theme::fg_color())
            } else {
                Style::default().fg(theme::BLUE())
            };

            let mut spans = vec![Span::styled(format!("  {}", name), style)];
            if !desc.is_empty() {
                spans.push(Span::styled(
                    format!("   ── {}", desc),
                    Style::default().fg(theme::fg_dim()),
                ));
            }
            tree_lines.push(Line::from(spans));
        }

        frame.render_widget(
            Paragraph::new(tree_lines)
                .block(tree_block)
                .wrap(Wrap { trim: false }),
            content_chunks[0],
        );

        // Preview pane (right)
        let preview_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_dim()))
            .title(" PREVIEW ");

        let preview_lines = if let Some(diff) = &state.diff_content {
            // Show first 50 lines of diff as preview
            diff.lines()
                .take(50)
                .map(|l| {
                    let style = if l.starts_with('+') {
                        Style::default().fg(theme::GREEN())
                    } else if l.starts_with('-') {
                        Style::default().fg(theme::RED())
                    } else if l.starts_with("@") {
                        Style::default().fg(theme::AMBER())
                    } else {
                        Style::default().fg(theme::fg_color())
                    };
                    Line::from(Span::styled(l.to_string(), style))
                })
                .collect::<Vec<_>>()
        } else {
            vec![Line::from(Span::styled(
                "  Select a file to preview",
                Style::default().fg(theme::fg_dim()),
            ))]
        };

        frame.render_widget(
            Paragraph::new(preview_lines)
                .block(preview_block)
                .wrap(Wrap { trim: false }),
            content_chunks[1],
        );

        // Footer
        let footer = Line::from(vec![Span::styled(
            " [j/k] navigate   [Esc] back",
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
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected = (self.selected + 1).min(11);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                true
            }
            KeyCode::Char('h') => {
                state.current_page = PageId::History;
                true
            }
            _ => false,
        }
    }
}
