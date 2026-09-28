//! The provider sheet: see and change which provider and model each agent uses.
//!
//! A multi-agent harness is only as good as the routing between its agents, and
//! routing was the one thing a user could not touch without editing TOML. The
//! common case — "the planner is too slow, point it at the local model" — meant
//! leaving the product.
//!
//! Two panels, because the interesting question is rarely "what is the API
//! key" (that belongs in the environment) and almost always "which of my
//! agents is pointed where".
//!
//! API keys are deliberately **not** editable here. A key typed into a form
//! ends up in the frame buffer, in shell history and in any transcript of the
//! session; `niki auth login` puts it in the OS keyring for exactly that
//! reason. The sheet says where the key comes from and leaves it there.

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::config::edit;
use crate::config::types::AgentConfig;
use crate::display::sheets::{self, SheetOutcome};
use crate::display::state::AppState;

/// The agents, in the order the pipeline runs them.
pub const AGENTS: &[&str] = &crate::config::types::AgentsConfig::NAMES;

#[derive(Debug)]
pub struct ProviderSheet {
    /// Row within the agent list.
    pub cursor: usize,
    /// Which field of the selected agent is being typed into.
    editing: Option<Field>,
    buffer: String,
    /// Whether the buffer still holds the original value.
    ///
    /// `begin_edit` seeds the buffer with what is there now, so a user can see
    /// what they are changing. But then the first character typed has to
    /// REPLACE it: appending to `claude-sonnet-4-20250514` to make
    /// `qwen2.5-coder:7b` means pressing Backspace 28 times first, and there is
    /// no select-all in this form.
    seeded: bool,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Provider,
    Model,
}

impl Default for ProviderSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderSheet {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            editing: None,
            buffer: String::new(),
            seeded: false,
            status: None,
        }
    }

    pub fn title(&self) -> String {
        "Providers".to_string()
    }

    pub fn hint(&self) -> String {
        if self.editing.is_some() {
            "type · enter commit · esc cancel".to_string()
        } else {
            "↑↓ agent · space edit provider · enter edit model · esc close".to_string()
        }
    }

    fn agent(&self) -> &'static str {
        AGENTS[self.cursor.min(AGENTS.len() - 1)]
    }

    /// The current routing for the selected agent, read from the live config.
    fn routing<'a>(&self, state: &'a AppState) -> &'a AgentConfig {
        state
            .config
            .agents
            .agent_named(self.agent())
            .expect("AGENTS comes from AgentsConfig::NAMES, so every name resolves")
    }

    fn set_field(&mut self, state: &AppState, field: Field, value: &str) -> Result<()> {
        let path = format!("agents.{}.{}", self.agent(), field_path(field));
        let value = value.trim();
        if value.is_empty() {
            return Ok(());
        }
        edit::set_value(
            &state.project_path.join("niki.toml"),
            &path,
            toml_edit::Value::from(value),
        )?;
        self.status = Some(format!(
            "{}: {} = {}",
            self.agent(),
            field_path(field),
            value
        ));
        Ok(())
    }

    fn begin_edit(&mut self, state: &AppState, field: Field) {
        self.buffer = match field {
            Field::Provider => self.routing(state).provider.clone(),
            Field::Model => self.routing(state).model.clone(),
        };
        self.editing = Some(field);
        self.seeded = true;
    }

    pub fn on_key(&mut self, key: KeyEvent, state: &mut AppState) -> Result<Option<SheetOutcome>> {
        if let Some(field) = self.editing {
            match key.code {
                KeyCode::Esc => {
                    self.editing = None;
                    self.status = Some("cancelled".into());
                }
                KeyCode::Enter => {
                    let buffer = std::mem::take(&mut self.buffer);
                    self.editing = None;
                    self.seeded = false;
                    if let Err(e) = self.set_field(state, field, &buffer) {
                        self.status = Some(format!("could not save: {e}"));
                    }
                }
                KeyCode::Backspace => {
                    if self.seeded {
                        // Backspacing into a seeded value clears it wholesale
                        // rather than deleting one character at a time from the
                        // end of a string the user never typed.
                        self.buffer.clear();
                        self.seeded = false;
                    } else {
                        self.buffer.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if self.seeded {
                        self.buffer.clear();
                        self.seeded = false;
                    }
                    self.buffer.push(c);
                }
                _ => {}
            }
            return Ok(None);
        }

        match key.code {
            _ if sheets::is_cancel(key) => return Ok(Some(SheetOutcome::Cancelled)),
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = sheets::move_cursor(self.cursor, 1, AGENTS.len());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = sheets::move_cursor(self.cursor, -1, AGENTS.len());
            }
            KeyCode::Char(' ') => self.begin_edit(state, Field::Provider),
            KeyCode::Enter => self.begin_edit(state, Field::Model),
            _ => {}
        }
        Ok(None)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let inner = sheets::chrome(frame, area, &self.title(), &self.hint(), true);
        if inner.height == 0 {
            return;
        }

        let name_w = AGENTS.iter().map(|a| a.len()).max().unwrap_or(12);
        let mut lines: Vec<Line> = vec![Line::from(Span::styled(
            format!("  {:<name_w$}  {:<22}  {}", "agent", "provider", "model"),
            Style::default().fg(crate::display::theme::fg_dim()),
        ))];

        let rows = (inner.height as usize).saturating_sub(2).min(AGENTS.len());
        let scroll = if self.cursor >= rows {
            self.cursor + 1 - rows
        } else {
            0
        };

        for (i, name) in AGENTS.iter().enumerate().skip(scroll).take(rows) {
            let cfg = state
                .config
                .agents
                .agent_named(name)
                .expect("the list comes from AgentsConfig::NAMES");
            let selected = i == self.cursor;
            let style = if selected {
                Style::default()
                    .fg(crate::display::theme::accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::display::theme::fg_color())
            };
            let marker = if selected {
                sheets::cursor_glyph()
            } else {
                " "
            };
            let editing = if selected { self.editing } else { None };

            let provider = match editing {
                Some(Field::Provider) => format!("{}▏", self.buffer),
                _ => cfg.provider.clone(),
            };
            let model = match editing {
                Some(Field::Model) => format!("{}▏", self.buffer),
                _ => cfg.model.clone(),
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(format!("{name:<name_w$}  "), style),
                Span::styled(
                    format!("{provider:<22}  "),
                    Style::default().fg(crate::display::theme::fg_color()),
                ),
                Span::styled(model, Style::default().fg(crate::display::theme::fg_dim())),
            ]));
        }

        if (inner.height as usize) > rows + 1 {
            lines.push(Line::from(Span::styled(
                "  API keys come from the environment or the OS keyring (niki auth login) — \n                 not from here, so they never pass through the frame buffer.",
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
            if let Some(status) = &self.status {
                lines.push(Line::from(Span::styled(
                    format!("  {status}"),
                    Style::default().fg(crate::display::theme::fg_dim()),
                )));
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn field_path(f: Field) -> &'static str {
    match f {
        Field::Provider => "provider",
        Field::Model => "model",
    }
}
