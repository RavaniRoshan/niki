//! The settings sheet: an editable form that writes `niki.toml`.
//!
//! The Config *page* has existed and is read-only — Tab moves a cursor across
//! fifteen fields, and no value can be changed or saved. A user who wants to
//! change a setting has to leave the product, find the file, and edit TOML by
//! hand. For a product whose pitch is that you never have to leave your
//! terminal, that is the gap this closes.
//!
//! Three kinds of field, because three kinds of change make sense:
//!
//!   Bool   — Space toggles. A checkbox the user can see the state of.
//!   Enum   — Space cycles, Left/Right step. Only the values that exist.
//!   Text   — Enter starts editing, characters go to the buffer, Enter commits.
//!
//! Nothing is written until the user commits. Edits are held in a pending map
//! and flushed on accept, so Esc really does mean "I changed nothing" — which is
//! the only way a destructive default in a settings form is tolerable.

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::config::edit::{self, ConfigScope};
use crate::display::sheets::{self, SheetOutcome};
use crate::display::state::AppState;

/// What kind of change a setting accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Bool,
    Enum(&'static [&'static str]),
    Text,
    /// A non-negative integer, edited as text and validated on commit.
    Number,
}

/// One editable setting.
#[derive(Debug, Clone, Copy)]
pub struct Setting {
    /// Dotted path in `niki.toml`.
    pub path: &'static str,
    /// What the user sees.
    pub label: &'static str,
    pub kind: FieldKind,
    /// One line explaining what this does, shown on the selected row.
    pub help: &'static str,
}

/// The settings the TUI owns.
///
/// Deliberately a curated list, not a reflection of `NikiConfig`. A form that
/// listed every field would be a settings file with a scrollbar; these are the
/// ones a person actually changes, and each is something NIKI can honour at
/// runtime rather than only on the next run.
pub const SETTINGS: &[Setting] = &[
    Setting {
        path: "ui.theme",
        label: "Theme",
        kind: FieldKind::Enum(&["auto", "dark", "light"]),
        help: "Colour theme. 'auto' follows the terminal's background.",
    },
    Setting {
        path: "general.max_revision_rounds",
        label: "Max revision rounds",
        kind: FieldKind::Number,
        help: "How many times the Coder may be asked to revise before the run reports what it has.",
    },
    Setting {
        path: "general.max_diff_lines",
        label: "Max diff lines",
        kind: FieldKind::Number,
        help: "Diff lines shown before truncation. 0 means no limit.",
    },
    Setting {
        path: "general.max_context_chars",
        label: "Max context chars",
        kind: FieldKind::Number,
        help: "Per-stage context budget in characters. Larger costs more.",
    },
    Setting {
        path: "general.spend_cap_usd",
        label: "Spend cap (USD)",
        kind: FieldKind::Number,
        help: "A run stops before it exceeds this. 0 means no cap.",
    },
    Setting {
        path: "general.output_dir",
        label: "Output directory",
        kind: FieldKind::Text,
        help: "Where run artifacts, tasks and history are written.",
    },
    Setting {
        path: "docker.backend",
        label: "Sandbox backend",
        kind: FieldKind::Enum(&["worktree", "docker", "podman"]),
        help: "worktree needs no container runtime. docker/podman give isolation.",
    },
    Setting {
        path: "docker.base_image",
        label: "Sandbox image",
        kind: FieldKind::Text,
        help: "Container image for the docker and podman backends.",
    },
    Setting {
        path: "docker.memory_limit",
        label: "Container memory",
        kind: FieldKind::Text,
        help: "Memory limit applied to the sandbox container.",
    },
    Setting {
        path: "docker.cpu_limit",
        label: "Container CPUs",
        kind: FieldKind::Number,
        help: "CPU limit applied to the sandbox container.",
    },
    Setting {
        path: "permissions.mode",
        label: "Permission mode",
        kind: FieldKind::Enum(&["manual", "auto", "dontask", "bypass"]),
        help: "How tool calls that need approval are handled. bypass runs commands unrestricted.",
    },
    Setting {
        path: "permissions.fail_closed_headless",
        label: "Fail closed (headless)",
        kind: FieldKind::Bool,
        help: "Deny a command needing approval when there is no TUI to ask, instead of allowing it.",
    },
    Setting {
        path: "permissions.auto_approve",
        label: "Auto-approve",
        kind: FieldKind::Bool,
        help: "Approve tool calls without asking. Read-only tools are always allowed.",
    },
    Setting {
        path: "ui.animations",
        label: "Animations",
        kind: FieldKind::Bool,
        help: "Motion such as the activity spinner and notice fades.",
    },
];

/// The form.
#[derive(Debug)]
pub struct SettingsSheet {
    /// Which row the cursor is on.
    pub cursor: usize,
    /// Path -> the text the user typed or the cycled-to value, not yet written.
    ///
    /// Public because "what has this form staged, and has any of it reached
    /// disk yet" is exactly what a test has to be able to ask, and a private
    /// field would push every such test to infer it from rendered output.
    pub pending: std::collections::BTreeMap<&'static str, String>,
    /// The row being typed into, if any.
    editing: Option<usize>,
    /// Where the typed cursor is.
    input_cursor: usize,
    /// The row to scroll to, so the cursor stays visible.
    scroll: usize,
    /// A message shown under the list.
    pub status: Option<String>,
}

impl Default for SettingsSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsSheet {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            pending: std::collections::BTreeMap::new(),
            editing: None,
            input_cursor: 0,
            scroll: 0,
            status: None,
        }
    }

    pub fn title(&self) -> String {
        let n = self.pending.len();
        if n == 0 {
            "Settings".to_string()
        } else {
            format!("Settings — {n} unsaved")
        }
    }

    pub fn hint(&self) -> String {
        if self.editing.is_some() {
            "type · enter commit · esc cancel field".to_string()
        } else {
            "↑↓ move · space change · enter save · esc discard and close".to_string()
        }
    }

    /// The value to show: the pending edit if there is one, else the file's.
    fn shown(&self, project_path: &std::path::Path) -> Option<String> {
        let s = SETTINGS[self.cursor];
        if let Some(p) = self.pending.get(s.path) {
            return Some(p.clone());
        }
        edit::get_value(&project_path.join("niki.toml"), s.path)
            .ok()
            .flatten()
            .map(|i| render_item(&i))
    }

    fn shown_for(&self, project_path: &std::path::Path, index: usize) -> Option<String> {
        let s = SETTINGS[index];
        if let Some(p) = self.pending.get(s.path) {
            return Some(p.clone());
        }
        edit::get_value(&project_path.join("niki.toml"), s.path)
            .ok()
            .flatten()
            .map(|i| render_item(&i))
    }

    /// Start editing the current row.
    fn begin_edit(&mut self, project_path: &std::path::Path) {
        self.input_cursor = self
            .shown(project_path)
            .map(|v| v.chars().count())
            .unwrap_or(0);
        self.editing = Some(self.cursor);
    }

    /// Commit the row being typed into, into `pending` (not to disk).
    fn commit_field(&mut self, buffer: &str) {
        if let Some(idx) = self.editing.take() {
            let s = SETTINGS[idx];
            let cleaned = buffer.trim().to_string();
            if s.kind == FieldKind::Number
                && !cleaned.is_empty()
                && !cleaned.chars().all(|c| c.is_ascii_digit() || c == '.')
            {
                // Unreachable through the keyboard — the keystroke filter
                // refuses non-numeric characters before they get here — but kept
                // so a programmatic caller cannot stage a value NIKI will fail
                // to parse on the next run.
                self.pending.remove(s.path);
                self.status = Some(format!("{} must be a number", s.label));
            } else {
                self.pending.insert(s.path, cleaned);
                self.status = None;
            }
        }
    }

    /// Apply a change to the current row without entering edit mode.
    fn nudge(&mut self, project_path: &std::path::Path, direction: isize) {
        let s = SETTINGS[self.cursor];
        match s.kind {
            FieldKind::Bool => {
                let now = self
                    .shown(project_path)
                    .map(|v| v == "true")
                    .unwrap_or(false);
                self.pending
                    .insert(s.path, if now { "false".into() } else { "true".into() });
                self.status = None;
            }
            FieldKind::Enum(values) => {
                let current = self
                    .shown(project_path)
                    .and_then(|v| values.iter().position(|c| *c == v).map(|i| i as isize))
                    .unwrap_or(0);
                let next = sheets::move_cursor(current as usize, direction, values.len());
                self.pending.insert(s.path, values[next].to_string());
                self.status = None;
                if s.path == "ui.theme" {
                    let name = values[next];
                    // Preview the moment it is chosen, so the user is looking
                    // at the thing they are selecting rather than reading a
                    // name. `ThemeMode::from_str` maps anything unrecognised to
                    // Auto, which is why the enum above lists only real modes.
                    crate::display::theme::set_mode(crate::display::theme::ThemeMode::from_str(
                        name,
                    ));
                }
            }
            _ => self.begin_edit(project_path),
        }
    }

    /// Write every pending change. Returns how many keys were written.
    pub fn commit(&mut self, project_path: &std::path::Path) -> Result<usize> {
        let path = project_path.join("niki.toml");
        let mut written = 0;
        let mut last_error: Option<String> = None;

        for (key, value) in std::mem::take(&mut self.pending) {
            let parsed = parse_for(
                &SETTINGS
                    .iter()
                    .find(|s| s.path == key)
                    .map(|s| s.kind)
                    .unwrap_or(FieldKind::Text),
                &value,
            );
            match edit::set_value(&path, key, parsed) {
                Ok(()) => written += 1,
                Err(e) => last_error = Some(format!("{key}: {e}")),
            }
        }

        // Theme is the one setting that must also take effect in this process,
        // or the form would report a change the user cannot see.
        if written > 0 {
            if let Ok(Some(item)) = edit::get_value(&path, "ui.theme") {
                crate::display::theme::set_mode(crate::display::theme::ThemeMode::from_str(
                    &render_item(&item),
                ));
            }
        }

        self.status = match last_error {
            Some(e) => Some(format!("{written} saved, but {e}")),
            None if written > 0 => Some(format!(
                "saved {written} to {}",
                ConfigScope::Project.label()
            )),
            None => None,
        };
        Ok(written)
    }

    pub fn on_key(&mut self, key: KeyEvent, state: &mut AppState) -> Result<Option<SheetOutcome>> {
        let project_path = state.project_path.clone();

        // Editing mode owns every key except the two that leave it.
        if self.editing.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.editing = None;
                    self.status = Some("field edit cancelled".into());
                }
                KeyCode::Enter => {
                    let buffer = self.shown(&project_path).unwrap_or_default();
                    self.commit_field(&buffer);
                }
                KeyCode::Backspace => {
                    if let Some(p) = self.pending.get_mut(SETTINGS[self.cursor].path) {
                        p.pop();
                    }
                }
                KeyCode::Char(c) => {
                    let setting = SETTINGS[self.cursor];
                    // A numeric field refuses non-numeric keystrokes rather
                    // than accepting them and failing at save time. Validating
                    // on commit could not work: the characters are already in
                    // the buffer by then, so "twelve" would be staged and the
                    // check would see exactly the thing it was meant to
                    // reject. Refusing the keystroke also tells the user
                    // immediately, which is the only moment it is useful.
                    if setting.kind == FieldKind::Number && !c.is_ascii_digit() && c != '.' {
                        self.status = Some(format!(
                            "{} takes a number — that key was ignored",
                            setting.label
                        ));
                    } else {
                        let path = setting.path;
                        let entry = self.pending.entry(path).or_default();
                        entry.push(c);
                        self.input_cursor = entry.chars().count();
                        self.status = None;
                    }
                }
                _ => {}
            }
            return Ok(None);
        }

        match key.code {
            _ if sheets::is_cancel(key) => {
                // Unsaved changes are discarded, and the user is told so rather
                // than discovering it later.
                if !self.pending.is_empty() {
                    self.pending.clear();
                    self.status = Some("discarded unsaved changes".into());
                    return Ok(None);
                }
                return Ok(Some(SheetOutcome::Cancelled));
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.commit(&project_path)?;
            }
            KeyCode::Down | KeyCode::Char('j')
                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.cursor = sheets::move_cursor(self.cursor, 1, SETTINGS.len());
            }
            KeyCode::Up | KeyCode::Char('k') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = sheets::move_cursor(self.cursor, -1, SETTINGS.len());
            }
            // One verb per key, which is the only way a form's behaviour is
            // predictable without reading the source.
            //
            //   space  change this value (toggle / cycle / start typing)
            //   enter  save everything staged
            //   esc    discard the draft, then close
            //
            // The previous mapping had Enter mean "save if dirty, otherwise
            // start editing", which reads as a rule and is not one: pressing
            // Enter on a clean form did nothing at all, and the characters that
            // followed were dropped because no field was being edited.
            KeyCode::Right => self.nudge(&project_path, 1),
            KeyCode::Left => self.nudge(&project_path, -1),
            KeyCode::Char(' ') => self.nudge(&project_path, 1),
            KeyCode::Enter => {
                if self.pending.is_empty() {
                    self.status = Some("nothing to save".into());
                } else {
                    self.commit(&project_path)?;
                }
            }
            _ => {}
        }
        Ok(None)
    }

    /// A nested sheet finished. A plain form has nothing to collapse, but the
    /// hook exists so a form that opens a picker can.
    pub fn on_child_accepted(&mut self) {}

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let project_path = state.project_path.clone();
        let inner = sheets::chrome(frame, area, &self.title(), &self.hint(), true);
        if inner.height == 0 {
            return;
        }

        let rows = (inner.height as usize).min(SETTINGS.len());
        // Keep the cursor inside the window.
        let scroll = if self.cursor < self.scroll {
            self.cursor
        } else if self.cursor >= self.scroll + rows {
            self.cursor + 1 - rows
        } else {
            self.scroll
        };

        let mut lines: Vec<Line> = Vec::with_capacity(rows + 2);
        for (i, setting) in SETTINGS.iter().enumerate().skip(scroll).take(rows) {
            let selected = i == self.cursor;
            let editing = self.editing == Some(i);
            let value = self.shown_for(&project_path, i);
            let changed = self.pending.contains_key(setting.path);

            let marker = if selected {
                sheets::cursor_glyph()
            } else {
                " "
            };
            let name_style = if selected {
                Style::default()
                    .fg(crate::display::theme::accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::display::theme::fg_color())
            };
            let value_style = if changed {
                Style::default()
                    .fg(crate::display::theme::success())
                    .add_modifier(Modifier::BOLD)
            } else if editing {
                Style::default().fg(crate::display::theme::accent())
            } else {
                Style::default().fg(crate::display::theme::fg_dim())
            };

            let label_width = SETTINGS.iter().map(|s| s.label.len()).max().unwrap_or(24);
            let label = format!("{:<width$}", setting.label, width = label_width);
            let shown = match (&value, editing) {
                (_, true) => format!("{}▏", value.unwrap_or_default()),
                (Some(v), false) => v.clone(),
                (None, false) => "(unset)".to_string(),
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), name_style),
                Span::styled(label, name_style),
                Span::raw("  "),
                Span::styled(shown, value_style),
            ]));
        }

        if (inner.height as usize) > rows {
            lines.push(Line::from(""));
            let help = SETTINGS[self.cursor].help;
            lines.push(Line::from(Span::styled(
                help.to_string(),
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

/// Render a config item for display.
fn render_item(item: &toml_edit::Item) -> String {
    if let Some(s) = item.as_str() {
        s.to_string()
    } else if let Some(b) = item.as_bool() {
        b.to_string()
    } else if let Some(i) = item.as_integer() {
        i.to_string()
    } else if let Some(f) = item.as_float() {
        f.to_string()
    } else {
        item.to_string()
    }
}

/// Parse the user's text into the value shape the field declares.
fn parse_for(kind: &FieldKind, value: &str) -> toml_edit::Value {
    match kind {
        FieldKind::Bool => toml_edit::Value::from(value == "true"),
        FieldKind::Number => match value.parse::<i64>() {
            Ok(i) => toml_edit::Value::from(i),
            // The UI rejects non-numeric input before this point; a value that
            // gets here anyway is written as a float rather than dropped.
            Err(_) => value
                .parse::<f64>()
                .map(toml_edit::Value::from)
                .unwrap_or_else(|_| toml_edit::Value::from(value)),
        },
        _ => toml_edit::Value::from(value),
    }
}
