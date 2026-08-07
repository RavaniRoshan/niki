use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::theme;
use super::pages::{AppState, PageId};

pub struct CommandPalette {
    pub selected: usize,
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
            PaletteItem { label: "pipeline", shortcut: "p", page: Some(PageId::Pipeline), action: PaletteAction::Navigate(PageId::Pipeline) },
            PaletteItem { label: "agents", shortcut: "a", page: Some(PageId::Agents), action: PaletteAction::Navigate(PageId::Agents) },
            PaletteItem { label: "diff", shortcut: "d", page: Some(PageId::Diff), action: PaletteAction::Navigate(PageId::Diff) },
            PaletteItem { label: "verdict", shortcut: "v", page: Some(PageId::Verdict), action: PaletteAction::Navigate(PageId::Verdict) },
            PaletteItem { label: "cost", shortcut: "c", page: Some(PageId::Cost), action: PaletteAction::Navigate(PageId::Cost) },
            PaletteItem { label: "artifacts", shortcut: "f", page: Some(PageId::Artifacts), action: PaletteAction::Navigate(PageId::Artifacts) },
            PaletteItem { label: "history", shortcut: "h", page: Some(PageId::History), action: PaletteAction::Navigate(PageId::History) },
            PaletteItem { label: "test log", shortcut: "l", page: Some(PageId::TestLog), action: PaletteAction::Navigate(PageId::TestLog) },
            PaletteItem { label: "config", shortcut: ",", page: Some(PageId::Config), action: PaletteAction::Navigate(PageId::Config) },
            PaletteItem { label: "help", shortcut: "?", page: Some(PageId::Help), action: PaletteAction::Navigate(PageId::Help) },
            PaletteItem { label: "pause / resume", shortcut: "space", page: None, action: PaletteAction::PauseResume },
            PaletteItem { label: "theme: cycle", shortcut: "t", page: None, action: PaletteAction::CycleTheme },
            PaletteItem { label: "quit", shortcut: "q", page: None, action: PaletteAction::Quit },
        ];
        Self { selected: 0, items }
    }

    pub fn handle_key(&mut self, key: KeyEvent, state: &mut AppState) -> bool {
        match key.code {
            KeyCode::Esc => {
                state.show_command_palette = false;
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                } else {
                    self.selected = self.items.len() - 1;
                }
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1) % self.items.len();
                true
            }
            KeyCode::Enter => {
                self.execute_selected(state)
            }
            KeyCode::Char(c) => {
                if let Some(idx) = self.items.iter().position(|i| i.shortcut == c.to_string().as_str()) {
                    self.selected = idx;
                    self.execute_selected(state)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn execute_selected(&self, state: &mut AppState) -> bool {
        let item = &self.items[self.selected];
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

pub fn render_command_palette(frame: &mut Frame, palette: &CommandPalette, area: Rect) {
    let popup_width = 42.min(area.width - 4);
    let popup_height = (palette.items.len() as u16 + 4).min(area.height - 4);
    let x = (area.width - popup_width) / 2;
    let y = (area.height - popup_height) / 2;

    let popup_area = Rect { x, y, width: popup_width, height: popup_height };
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
        let is_selected = i == palette.selected;
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
