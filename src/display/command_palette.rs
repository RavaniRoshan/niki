use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::pages::{AppState, PageId};
use crate::display::components::list_cursor::ListCursor;
use crate::display::theme;

pub struct CommandPalette {
    /// Universal list cursor shared with the slash menu / permission modal.
    pub cursor: ListCursor,
    pub items: Vec<PaletteItem>,
}

pub struct PaletteItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub page: Option<PageId>,
    pub action: PaletteAction,
}

#[derive(Clone, PartialEq)]
pub enum PaletteAction {
    Navigate(PageId),
    PauseResume,
    CycleTheme,
    Quit,
    None,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        let items = vec![
            PaletteItem {
                label: "pipeline",
                shortcut: "p",
                page: Some(PageId::Pipeline),
                action: PaletteAction::Navigate(PageId::Pipeline),
            },
            PaletteItem {
                label: "agents",
                shortcut: "a",
                page: Some(PageId::Agents),
                action: PaletteAction::Navigate(PageId::Agents),
            },
            PaletteItem {
                label: "diff",
                shortcut: "d",
                page: Some(PageId::Diff),
                action: PaletteAction::Navigate(PageId::Diff),
            },
            PaletteItem {
                label: "verdict",
                shortcut: "v",
                page: Some(PageId::Verdict),
                action: PaletteAction::Navigate(PageId::Verdict),
            },
            PaletteItem {
                label: "cost",
                shortcut: "c",
                page: Some(PageId::Cost),
                action: PaletteAction::Navigate(PageId::Cost),
            },
            PaletteItem {
                label: "artifacts",
                shortcut: "f",
                page: Some(PageId::Artifacts),
                action: PaletteAction::Navigate(PageId::Artifacts),
            },
            PaletteItem {
                label: "history",
                shortcut: "h",
                page: Some(PageId::History),
                action: PaletteAction::Navigate(PageId::History),
            },
            PaletteItem {
                label: "test log",
                shortcut: "l",
                page: Some(PageId::TestLog),
                action: PaletteAction::Navigate(PageId::TestLog),
            },
            PaletteItem {
                label: "config",
                shortcut: ",",
                page: Some(PageId::Config),
                action: PaletteAction::Navigate(PageId::Config),
            },
            PaletteItem {
                label: "help",
                shortcut: "?",
                page: Some(PageId::Help),
                action: PaletteAction::Navigate(PageId::Help),
            },
            PaletteItem {
                label: "pause / resume",
                shortcut: "space",
                page: None,
                action: PaletteAction::PauseResume,
            },
            PaletteItem {
                label: "theme: cycle",
                shortcut: "t",
                page: None,
                action: PaletteAction::CycleTheme,
            },
            PaletteItem {
                label: "quit",
                shortcut: "q",
                page: None,
                action: PaletteAction::Quit,
            },
        ];
        let cursor = ListCursor::new(items.len());
        Self { cursor, items }
    }

    /// Currently highlighted row.
    pub fn selected(&self) -> usize {
        self.cursor.selected
    }

    pub fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc => {
                state.show_command_palette = false;
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor.prev();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor.next();
                true
            }
            KeyCode::Enter => self.execute_selected(state),
            KeyCode::Char(c) => {
                if let Some(idx) = self
                    .items
                    .iter()
                    .position(|i| i.shortcut == c.to_string().as_str())
                {
                    self.cursor.set_selected(idx);
                    self.execute_selected(state)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Move the highlight to a hovered row (mouse). Returns `true` on change.
    pub fn hover(&mut self, idx: usize) -> bool {
        self.cursor.hover(idx)
    }

    /// Select and run the clicked row (mouse click-to-select).
    pub fn click(&mut self, idx: usize, state: &mut AppState) -> bool {
        match self.cursor.click(idx) {
            Some(_) => self.execute_selected(state),
            None => false,
        }
    }

    fn execute_selected(&self, state: &mut AppState) -> bool {
        let Some(idx) = self.cursor.submit() else {
            return false;
        };
        let item = &self.items[idx];
        state.show_command_palette = false;
        match &item.action {
            PaletteAction::Navigate(page) => {
                state.current_page = *page;
                true
            }
            PaletteAction::PauseResume => {
                state.paused = !state.paused;
                true
            }
            PaletteAction::CycleTheme => {
                use crate::config::types::ThemePreference;
                let new_pref = match state.config.ui.theme {
                    ThemePreference::Dark => ThemePreference::Light,
                    ThemePreference::Light => ThemePreference::Auto,
                    ThemePreference::Auto => ThemePreference::Dark,
                };
                let mode = match new_pref {
                    ThemePreference::Dark => crate::display::theme::ThemeMode::Dark,
                    ThemePreference::Light => crate::display::theme::ThemeMode::Light,
                    ThemePreference::Auto => crate::display::theme::ThemeMode::Auto,
                };
                crate::display::theme::set_mode(mode);
                state.config.ui.theme = new_pref;
                let _ = crate::config::types::NikiConfig::save_theme(new_pref);
                true
            }
            PaletteAction::Quit => {
                state.modal = Some(super::pages::Modal::Confirm {
                    title: "Quit NIKI?".to_string(),
                    message: "The pipeline will continue in the background.".to_string(),
                });
                true
            }
            PaletteAction::None => false,
        }
    }
}

/// Geometry of the palette popup — shared by the renderer and the hit-test so
/// mouse hover/click always match what is on screen.
pub fn popup_rect(palette: &CommandPalette, area: Rect) -> Rect {
    let popup_width = 42.min(area.width.saturating_sub(4));
    let popup_height = (palette.items.len() as u16 + 4).min(area.height.saturating_sub(4));
    let x = area.width.saturating_sub(popup_width) / 2;
    let y = area.height.saturating_sub(popup_height) / 2;
    Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    }
}

/// Hit-test a mouse position against the palette rows, returning the row index.
pub fn click_index(palette: &CommandPalette, area: Rect, x: u16, y: u16) -> Option<usize> {
    let popup = popup_rect(palette, area);
    let inner_left = popup.x + 1;
    let inner_right = popup.x + popup.width.saturating_sub(1);
    let inner_top = popup.y + 1;
    let inner_bottom = popup.y + popup.height.saturating_sub(1);
    if x < inner_left || x >= inner_right || y < inner_top || y >= inner_bottom {
        return None;
    }
    let idx = (y - inner_top) as usize;
    if idx < palette.items.len() {
        Some(idx)
    } else {
        None
    }
}

pub fn render_command_palette(frame: &mut Frame, palette: &CommandPalette, area: Rect) {
    let popup_area = popup_rect(palette, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border_color()))
        .title(Span::styled(
            " Commands ",
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD),
        ));

    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in palette.items.iter().enumerate() {
        let is_selected = i == palette.selected();
        let prefix = if is_selected { "▸ " } else { "  " };
        let item_style = if is_selected {
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg_color())
        };
        let shortcut_style = if is_selected {
            Style::default()
                .fg(theme::fg_color())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg_dim())
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, item_style),
            Span::styled(format!("{:<20}", item.label), item_style),
            Span::styled(format!("[{}]", item.shortcut), shortcut_style),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(block), popup_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn test_state() -> AppState {
        let config = crate::config::NikiConfig::default();
        AppState::new("test".to_string(), config, ".".into())
    }

    #[test]
    fn palette_cursor_wraps() {
        let mut palette = CommandPalette::new();
        let mut state = test_state();
        assert_eq!(palette.selected(), 0);
        // Up from the first row wraps to the last (universal cursor semantics).
        palette.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &mut state);
        assert_eq!(palette.selected(), palette.items.len() - 1);
        palette.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &mut state);
        assert_eq!(palette.selected(), 0);
    }

    #[test]
    fn palette_click_index_maps_rows() {
        let palette = CommandPalette::new();
        let area = Rect::new(0, 0, 100, 40);
        let popup = popup_rect(&palette, area);
        // First row sits just inside the top border.
        assert_eq!(
            click_index(&palette, area, popup.x + 2, popup.y + 1),
            Some(0)
        );
        assert_eq!(
            click_index(&palette, area, popup.x + 2, popup.y + 3),
            Some(2)
        );
        // Border rows and columns are not selectable.
        assert_eq!(click_index(&palette, area, popup.x, popup.y + 1), None);
        assert_eq!(click_index(&palette, area, popup.x + 2, popup.y), None);
    }

    #[test]
    fn palette_click_runs_row() {
        let mut palette = CommandPalette::new();
        let mut state = test_state();
        state.show_command_palette = true;
        // Row 0 navigates to the Pipeline page.
        assert!(palette.click(0, &mut state));
        assert_eq!(state.current_page, PageId::Pipeline);
        assert!(!state.show_command_palette);
        // Out-of-range clicks are ignored.
        assert!(!palette.click(999, &mut state));
    }
}
