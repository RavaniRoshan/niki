use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{AppState, Page, StageStatus};
use crate::display::theme;

pub struct RunPage {
    scroll_offset: u16,
    auto_scroll: bool,
}

impl Default for RunPage {
    fn default() -> Self {
        Self::new()
    }
}

impl RunPage {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            auto_scroll: true,
        }
    }
}

impl Page for RunPage {
    fn title(&self) -> &str {
        "home"
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        if area.height < 8 {
            let msg = Paragraph::new("Terminal too small").style(Style::default().fg(theme::RED()));
            frame.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // task card
                Constraint::Length(1), // command line
                Constraint::Min(4),    // pipeline output
                Constraint::Length(1), // separator
                Constraint::Length(2), // results + working tree
                Constraint::Length(1), // footer
            ])
            .split(area);

        // ── Task card (bordered box + description + tags) ──────────────────
        // Render task card with subtle border
        let card_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border_color()))
            .style(Style::default().bg(theme::bg_elevated()));
        frame.render_widget(card_block, chunks[0]);

        // Task card content layout
        let card_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title + description
                Constraint::Length(1), // tags
            ])
            .margin(1)
            .split(chunks[0]);

        // Title line: "Build" + description + paused indicator
        let desc = &state.description;
        let desc_width = (card_chunks[0].width as usize).saturating_sub(10);
        let truncated = theme::truncate_str_ellipsis(desc, desc_width);

        let mut title_spans = vec![
            Span::styled(
                "Build ",
                Style::default()
                    .fg(theme::fg_color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncated, Style::default().fg(theme::fg_color())),
        ];
        if state.paused {
            title_spans.push(Span::styled(
                "  ■ PAUSED",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let title_line = Line::from(title_spans);
        frame.render_widget(Paragraph::new(title_line), card_chunks[0]);

        // Tags line (bordered badges).
        //
        // These were the literals "sandbox" and "docker", printed on every run
        // regardless of `--backend`. A worktree run — the backend the zero-setup
        // install uses, and the one the setup wizard writes into `niki.toml` on
        // any machine without Podman — was labelled as running in Docker.
        let tags: Vec<String> = vec![
            "sandbox".to_string(),
            match state.config.docker.backend {
                crate::sandbox::SandboxBackend::Docker => "docker".to_string(),
                crate::sandbox::SandboxBackend::Worktree => "worktree".to_string(),
            },
        ];
        let mut tag_spans = vec![Span::styled("  ", Style::default())];
        for tag in &tags {
            tag_spans.push(Span::styled(
                format!(" {} ", tag),
                Style::default()
                    .fg(theme::fg_dim())
                    .add_modifier(Modifier::BOLD),
            ));
            tag_spans.push(Span::styled(" ", Style::default()));
        }
        frame.render_widget(Paragraph::new(Line::from(tag_spans)), card_chunks[1]);

        // ── Command line ──────────────────────────────────────────────────
        let cmd_line = Line::from(vec![
            Span::styled("$ ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                format!("niki run \"{}\"", state.description),
                Style::default().fg(theme::fg_color()),
            ),
            // The real project path. This was the literal "./my-app", on every
            // run, in every project.
            Span::styled(
                format!(" --project {}", state.project_path.display()),
                Style::default().fg(theme::fg_dim()),
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
                    let detail = tail
                        .unwrap_or("working...")
                        .chars()
                        .take(60)
                        .collect::<String>();
                    (detail, color)
                }
                StageStatus::Done => {
                    let summary = stage.summary.first().map(|s| s.as_str()).unwrap_or("done");
                    (summary.to_string(), theme::success())
                }
                StageStatus::Failed => {
                    let summary = stage
                        .summary
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("failed");
                    (summary.to_string(), theme::RED())
                }
                StageStatus::Queued => ("queued".to_string(), theme::fg_dim()),
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
        }

        // Add queued roles that haven't started yet
        let started_roles: Vec<_> = state.stages.iter().map(|s| s.role).collect();
        let primary_roles = [
            crate::artifacts::types::AgentRole::Planner,
            crate::artifacts::types::AgentRole::Coder,
            crate::artifacts::types::AgentRole::Tester,
            crate::artifacts::types::AgentRole::Reviewer,
        ];
        // Only once a run exists. This synthesised four `· queued` agents
        // whenever `state.stages` was empty — which, before `/run` existed,
        // was always. The Run page is one Tab from every user, so a fresh
        // session showed a pipeline that had been planned and was about to
        // start, for a run nobody had asked for.
        if !started_roles.is_empty() {
            for role in &primary_roles {
                if !started_roles.contains(role) {
                    pipeline_lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", theme::role_glyph(*role)),
                            Style::default().fg(theme::fg_dim()),
                        ),
                        Span::styled(
                            theme::role_name(*role).to_string(),
                            Style::default().fg(theme::fg_dim()),
                        ),
                        Span::styled(" · queued", Style::default().fg(theme::fg_dim())),
                    ]));
                }
            }
        }

        // Add checkmark if all primary roles completed successfully
        let all_done = primary_roles.iter().all(|role| {
            started_roles.contains(role)
                && !state
                    .stages
                    .iter()
                    .any(|s| s.role == *role && matches!(s.status, StageStatus::Failed))
        });
        if all_done && !started_roles.is_empty() {
            pipeline_lines.push(Line::from(vec![Span::styled(
                "✓",
                Style::default()
                    .fg(theme::success())
                    .add_modifier(Modifier::BOLD),
            )]));
        }

        // ── Tool execution cards (Claude Code parity) ─────────────────
        if !state.tool_cards.is_empty() {
            pipeline_lines.push(Line::from(vec![Span::styled("", Style::default())]));
            pipeline_lines.push(Line::from(vec![Span::styled(
                "  Recent tool calls:",
                Style::default().fg(theme::fg_subtle()),
            )]));
            // Show last 3 tool cards as compact one-liners
            for card in state.tool_cards.iter().rev().take(3).rev() {
                let glyph = card.status_glyph();
                let color = card.status_color();
                // The summary is model- and tool-authored text — a command, a
                // path, a grep pattern — sliced at a byte offset computed from
                // a column budget. `echo "café — résumé"` was enough to panic
                // the Run page.
                let summary = theme::truncate_str_ellipsis(&card.summary, 50);
                pipeline_lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(
                        format!("{} ", glyph),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<8}", card.tool_name),
                        Style::default().fg(theme::fg_bright()),
                    ),
                    Span::styled(summary, Style::default().fg(theme::fg_dim())),
                    if let Some(timing) = card.timing() {
                        Span::styled(
                            format!(" ({})", timing),
                            Style::default().fg(theme::fg_subtle()),
                        )
                    } else {
                        Span::raw("")
                    },
                ]));
            }
            if state.tool_cards.len() > 3 {
                pipeline_lines.push(Line::from(vec![Span::styled(
                    format!("    ... and {} more", state.tool_cards.len() - 3),
                    Style::default().fg(theme::fg_subtle()),
                )]));
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

        // Floating "N new" chip — shows when scrolled up and new content below
        let pending = if !self.auto_scroll && scroll < max_scroll {
            (max_scroll - scroll).min(99)
        } else {
            0
        };
        if pending > 0 {
            let chip_x = area.width.saturating_sub(12).max(1);
            let chip_y = chunks[2].y + chunks[2].height.saturating_sub(2);
            let chip_area = Rect {
                x: chip_x,
                y: chip_y,
                width: 10.min(area.width - chip_x),
                height: 1,
            };
            let chip = Paragraph::new(Line::from(vec![Span::styled(
                format!(" {} NEW ", pending),
                Style::default()
                    .fg(theme::bg_color())
                    .bg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            )]));
            frame.render_widget(chip, chip_area);
        }

        // ── Separator ─────────────────────────────────────────────────────
        let separator = Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::border_dim()),
        ));
        frame.render_widget(Paragraph::new(separator), chunks[3]);

        // ── Results ───────────────────────────────────────────────────────
        // No invented branch. This rendered "niki/xxxxx" whenever no run had
        // produced one — a ref that does not exist, presented with the same
        // weight as a real one.
        let branch_str = state.branch_name.clone();

        let result_spans = vec![
            Span::styled("branch ", Style::default().fg(theme::fg_color())),
            Span::styled(
                branch_str,
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " · report.md · changes.patch",
                Style::default().fg(theme::fg_color()),
            ),
        ];
        frame.render_widget(Paragraph::new(Line::from(result_spans)), chunks[4]);

        // Working tree line. This printed "untouched" in success green,
        // unconditionally and without ever looking: a repo with four hundred
        // dirty files was reported as untouched. It is now shown only when a
        // run has actually finished, and it reports what was done rather than
        // asserting a property nothing measured.
        let working_tree = if state.finished {
            Line::from(vec![
                Span::styled("working tree: ", Style::default().fg(theme::fg_dim())),
                Span::styled(
                    "diff applied for review",
                    Style::default().fg(theme::fg_subtle()),
                ),
            ])
        } else {
            Line::from(Span::styled("", Style::default()))
        };
        let wt_area = Rect {
            x: chunks[4].x,
            y: chunks[4].y + 1,
            width: chunks[4].width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(working_tree), wt_area);

        // ── Footer — only 3 keybindings (matching reference) ─────────────
        let footer = Line::from(vec![
            Span::styled(
                "tab ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("switch agent  ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                "ctrl-p ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("commands  ", Style::default().fg(theme::fg_dim())),
            Span::styled(
                "esc ",
                Style::default()
                    .fg(theme::fg_bright())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("cancel run", Style::default().fg(theme::fg_dim())),
        ]);
        frame.render_widget(Paragraph::new(footer), chunks[5]);
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Char(' ') => {
                state.paused = !state.paused;
                true
            }
            KeyCode::Char('g') => {
                self.scroll_offset = 0;
                if state
                    .stages
                    .iter()
                    .any(|s| s.status == StageStatus::Running)
                {
                    self.auto_scroll = false;
                }
                true
            }
            KeyCode::Char('G') => {
                self.scroll_offset = u16::MAX;
                self.auto_scroll = true;
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                self.auto_scroll = false;
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                self.auto_scroll = false;
                true
            }
            _ => false,
        }
    }
}
