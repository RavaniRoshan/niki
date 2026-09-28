//! The theme picker.
//!
//! `/theme` used to *cycle*: press it, get a different theme, press it again,
//! get another. There is no way to see the list, no way to know what is
//! available, and no way to go back to the one you had without cycling all the
//! way round. Codex and opencode both present a list with a live preview,
//! because "pick one of twelve" and "cycle through twelve" are different
//! interactions and only one of them is usable.

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::display::sheets::{self, SheetOutcome};
use crate::display::state::AppState;
use crate::display::theme::{self, ThemeMode};

/// The themes a user can choose, in the order they are shown.
///
/// These are the three the product actually implements (`ThemeMode`), not a
/// list of palettes it has never heard of. The first draft of this file
/// advertised twelve — nord, dracula, catppuccin and friends — and every one of
/// them beyond the first three would have silently fallen back to `Auto`,
/// because `ThemeMode` matches dark/light/auto and nothing else. A picker that
/// offers themes the product cannot apply is worse than a cycling key: it
/// teaches the user that the list is not to be trusted.
///
/// Adding a real palette is a separate change: a `ThemeMode` variant, the
/// colours, and then a line here.
pub const THEMES: &[(&str, &str)] = &[
    ("auto", "Auto — follow whatever the terminal reports."),
    ("dark", "Dark — the Kiln palette on a dark background."),
    ("light", "Light — the Kiln palette on a light background."),
];

/// Map a theme name to the mode that selects it.
///
/// `ThemeMode` is the product's own dark/light/auto switch; named palettes are
/// a separate registry, so a name that is not a mode falls back to `Auto`
/// rather than silently picking a different theme.
fn mode_for(name: &str) -> ThemeMode {
    ThemeMode::from_str(name)
}

#[derive(Debug)]
pub struct ThemeSheet {
    pub cursor: usize,
    /// The theme to restore if the user backs out.
    original: ThemeMode,
    /// Applied to the process, whether or not it was written to the file.
    applied: bool,
    pub status: Option<String>,
}

impl Default for ThemeSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeSheet {
    pub fn new() -> Self {
        let current = theme::current_mode().as_str();
        let cursor = THEMES.iter().position(|(n, _)| *n == current).unwrap_or(0);
        Self {
            cursor,
            original: theme::current_mode(),
            applied: false,
            status: None,
        }
    }

    pub fn title(&self) -> String {
        "Theme".to_string()
    }

    pub fn hint(&self) -> String {
        "↑↓ preview · enter apply and save · esc cancel".to_string()
    }

    fn selected_name(&self) -> &'static str {
        THEMES[self.cursor].0
    }

    /// Put the process into the selected theme, so the picker previews.
    fn preview(&mut self) {
        theme::set_mode(mode_for(self.selected_name()));
        self.applied = true;
    }

    fn commit(&mut self, state: &mut AppState) -> Result<()> {
        let name = self.selected_name();
        let path = state.project_path.join("niki.toml");
        crate::config::edit::set_value(&path, "ui.theme", toml_edit::Value::from(name))?;
        theme::set_mode(mode_for(name));
        self.status = Some(format!("theme set to {name}"));
        Ok(())
    }

    pub fn on_key(&mut self, key: KeyEvent, state: &mut AppState) -> Result<Option<SheetOutcome>> {
        match key.code {
            _ if sheets::is_cancel(key) => {
                // A preview the user saw and then rejected must not linger.
                theme::set_mode(self.original);
                return Ok(Some(SheetOutcome::Cancelled));
            }
            ratatui::crossterm::event::KeyCode::Down
            | ratatui::crossterm::event::KeyCode::Char('j') => {
                self.cursor = sheets::move_cursor(self.cursor, 1, THEMES.len());
                self.preview();
            }
            ratatui::crossterm::event::KeyCode::Up
            | ratatui::crossterm::event::KeyCode::Char('k') => {
                self.cursor = sheets::move_cursor(self.cursor, -1, THEMES.len());
                self.preview();
            }
            ratatui::crossterm::event::KeyCode::Enter => {
                if let Err(e) = self.commit(state) {
                    self.status = Some(format!("could not save: {e}"));
                    return Ok(None);
                }
                return Ok(Some(SheetOutcome::Accepted));
            }
            _ => {}
        }
        Ok(None)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let _ = state;
        let inner = sheets::chrome(frame, area, &self.title(), &self.hint(), true);
        if inner.height == 0 {
            return;
        }

        let body_height = inner.height.saturating_sub(2) as usize;
        let scroll = if self.cursor >= body_height {
            self.cursor + 1 - body_height
        } else {
            0
        };

        let mut lines: Vec<Line> = Vec::new();
        for (i, (name, desc)) in THEMES.iter().enumerate().skip(scroll).take(body_height) {
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
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(format!("{name:<12}"), style),
                Span::styled(
                    (*desc).to_string(),
                    Style::default().fg(crate::display::theme::fg_dim()),
                ),
            ]));
        }

        if (inner.height as usize) > body_height {
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
