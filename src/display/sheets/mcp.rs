//! The MCP sheet: see and change the servers this run may talk to.
//!
//! `/mcp` printed "configured via niki.toml [mcp] section. Use /config to
//! edit" — which is the stub shape. Now that `/config` exists as a real editor,
//! the honest options were to delete the command or to make it do the thing it
//! says. It does the thing.
//!
//! MCP servers are the one part of NIKI's reach that a user extends on purpose:
//! adding a server is how you give an agent a capability NIKI does not have.
//! Making that a file edit is the same mistake as making settings a file edit.

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::config::edit;
use crate::display::sheets::{self, SheetOutcome};
use crate::display::state::AppState;

#[derive(Debug)]
pub struct McpSheet {
    /// Index into `state.config.mcp.servers`.
    pub cursor: usize,
    /// The field being typed into for the selected server.
    editing: Option<Field>,
    buffer: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Command,
    Url,
}

impl Default for McpSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSheet {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            editing: None,
            buffer: String::new(),
            status: None,
        }
    }

    pub fn title(&self, state: &AppState) -> String {
        let n = state.config.mcp.servers.len();
        if n == 0 {
            "MCP — no servers configured".to_string()
        } else {
            format!("MCP — {n} server{}", if n == 1 { "" } else { "s" })
        }
    }

    pub fn hint(&self) -> String {
        if self.editing.is_some() {
            "type · enter commit · esc cancel".to_string()
        } else {
            "↑↓ server · space edit command · enter edit url · d disable · esc close".to_string()
        }
    }

    fn selected<'a>(
        &self,
        state: &'a AppState,
    ) -> Option<&'a crate::config::types::McpServerConfigEntry> {
        state.config.mcp.servers.get(self.cursor)
    }

    fn begin_edit(&mut self, state: &AppState, field: Field) {
        let Some(server) = self.selected(state) else {
            self.status = Some("no servers configured".into());
            return;
        };
        self.buffer = match field {
            Field::Command => server.command.clone().unwrap_or_default(),
            Field::Url => server.url.clone().unwrap_or_default(),
        };
        self.editing = Some(field);
    }

    /// Flip the selected server's `enabled`, which is the most common change
    /// and the one that does not involve typing a path.
    fn toggle(&mut self, state: &mut AppState) -> Result<()> {
        let Some(server) = self.selected(state) else {
            return Ok(());
        };
        let name = server.name.clone();
        let next = !server.enabled;
        let path = state.project_path.join("niki.toml");
        // `[[mcp.servers]]` is an array of tables, so the key is written at the
        // document root with an index-qualified path. Writing `mcp.servers.0.…`
        // would silently create a *table* called "0" instead.
        let doc = edit::read_doc(&path)?;
        let Some(index) = doc
            .get("mcp")
            .and_then(|m| m.get("servers"))
            .and_then(|s| s.as_array_of_tables())
            .and_then(|arr| {
                arr.iter()
                    .position(|t| t.get("name").and_then(|n| n.as_str()) == Some(&name))
            })
        else {
            self.status = Some(format!("{name} is not in {path:?}"));
            return Ok(());
        };
        edit::set_value(
            &path,
            &format!("mcp.servers.{index}.enabled"),
            toml_edit::Value::from(next),
        )?;
        state.config.mcp.servers[self.cursor].enabled = next;
        self.status = Some(format!(
            "{name} {}",
            if next { "enabled" } else { "disabled" }
        ));
        Ok(())
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
                    let Some(server) = self.selected(state) else {
                        return Ok(None);
                    };
                    let name = server.name.clone();
                    let path = state.project_path.join("niki.toml");
                    let key_path = format!("mcp.servers.{}.{field}", self.cursor);
                    match edit::set_value(&path, &key_path, toml_edit::Value::from(buffer.trim())) {
                        Ok(()) => self.status = Some(format!("{name}: {field} saved")),
                        Err(e) => self.status = Some(format!("could not save: {e}")),
                    }
                }
                KeyCode::Backspace => {
                    self.buffer.pop();
                }
                KeyCode::Char(c) => self.buffer.push(c),
                _ => {}
            }
            return Ok(None);
        }

        let count = state.config.mcp.servers.len();
        match key.code {
            _ if sheets::is_cancel(key) => return Ok(Some(SheetOutcome::Cancelled)),
            KeyCode::Down | KeyCode::Char('j') if count > 0 => {
                self.cursor = sheets::move_cursor(self.cursor, 1, count);
            }
            KeyCode::Up | KeyCode::Char('k') if count > 0 => {
                self.cursor = sheets::move_cursor(self.cursor, -1, count);
            }
            KeyCode::Char(' ') => self.begin_edit(state, Field::Command),
            KeyCode::Enter => self.begin_edit(state, Field::Url),
            KeyCode::Char('d') | KeyCode::Char('D') => self.toggle(state)?,
            _ => {}
        }
        Ok(None)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let inner = sheets::chrome(frame, area, &self.title(state), &self.hint(), true);
        if inner.height == 0 {
            return;
        }

        let servers = &state.config.mcp.servers;
        let mut lines: Vec<Line> = Vec::new();

        if servers.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No MCP servers configured.",
                Style::default().fg(crate::display::theme::fg_color()),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Add one under [mcp] in niki.toml:",
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
            lines.push(Line::from(Span::styled(
                "    [[mcp.servers]]",
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
            lines.push(Line::from(Span::styled(
                "    name = \"filesystem\"",
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
            lines.push(Line::from(Span::styled(
                "    command = \"npx\"",
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
            lines.push(Line::from(Span::styled(
                "    args = [\"-y\", \"@modelcontextprotocol/server-filesystem\", \".\"]",
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
        } else {
            let rows = (inner.height as usize).saturating_sub(2).min(servers.len());
            let scroll = if self.cursor >= rows {
                self.cursor + 1 - rows
            } else {
                0
            };
            let editing = if self.cursor >= scroll && self.cursor < scroll + rows {
                self.editing
            } else {
                None
            };

            for (i, server) in servers.iter().enumerate().skip(scroll).take(rows) {
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
                let toggle = if server.enabled { "on " } else { "off" };
                let toggle_style = if server.enabled {
                    Style::default().fg(crate::display::theme::success())
                } else {
                    Style::default().fg(crate::display::theme::fg_dim())
                };
                let target = match (editing, i == self.cursor) {
                    (Some(Field::Command), true) => format!("{}▏", self.buffer),
                    (Some(Field::Url), true) => format!("{}▏", self.buffer),
                    _ => server
                        .command
                        .clone()
                        .or_else(|| server.url.clone())
                        .unwrap_or_else(|| "(no command or url)".to_string()),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{marker} "), style),
                    Span::styled(format!("{:<12} ", server.name), style),
                    Span::styled(format!("{toggle} "), toggle_style),
                    Span::styled(target, Style::default().fg(crate::display::theme::fg_dim())),
                ]));
            }
        }

        if let Some(status) = &self.status
            && (inner.height as usize) > lines.len() + 1
        {
            lines.push(Line::from(Span::styled(
                format!("  {status}"),
                Style::default().fg(crate::display::theme::fg_dim()),
            )));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Field::Command => "command",
            Field::Url => "url",
        })
    }
}
