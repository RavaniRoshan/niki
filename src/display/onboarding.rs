use std::fs;
use std::path::Path;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::display::theme;

const STATE_FILE: &str = ".niki/state.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingPage {
    Welcome,
    AuthSecurity,
    TerminalSetup,
    Telemetry,
    Help,
}

impl OnboardingPage {
    pub const ALL: &'static [OnboardingPage] = &[
        OnboardingPage::Welcome,
        OnboardingPage::AuthSecurity,
        OnboardingPage::TerminalSetup,
        OnboardingPage::Telemetry,
        OnboardingPage::Help,
    ];

    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|p| p == self).unwrap_or(0)
    }

    pub fn next(&self) -> Option<OnboardingPage> {
        let i = self.index();
        Self::ALL.get(i + 1).copied()
    }

    pub fn prev(&self) -> Option<OnboardingPage> {
        let i = self.index();
        if i == 0 { None } else { Some(Self::ALL[i - 1]) }
    }

    pub fn title(&self) -> &'static str {
        match self {
            OnboardingPage::Welcome => "Welcome to Niki",
            OnboardingPage::AuthSecurity => "Authentication & Security",
            OnboardingPage::TerminalSetup => "Terminal Setup",
            OnboardingPage::Telemetry => "Telemetry",
            OnboardingPage::Help => "Getting Help",
        }
    }
}

#[derive(Debug)]
pub struct OnboardingModal {
    pub page: OnboardingPage,
    pub dont_show_again: bool,
}

impl Default for OnboardingModal {
    fn default() -> Self {
        Self::new()
    }
}

impl OnboardingModal {
    pub fn new() -> Self {
        Self {
            page: OnboardingPage::Welcome,
            dont_show_again: true,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.page.next().is_none()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> OnboardingAction {
        match key.code {
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                if let Some(next) = self.page.next() {
                    self.page = next;
                    OnboardingAction::None
                } else {
                    OnboardingAction::Finish
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(prev) = self.page.prev() {
                    self.page = prev;
                }
                OnboardingAction::None
            }
            KeyCode::Char('n') => {
                if let Some(next) = self.page.next() {
                    self.page = next;
                    OnboardingAction::None
                } else {
                    OnboardingAction::Finish
                }
            }
            KeyCode::Char('p') => {
                if let Some(prev) = self.page.prev() {
                    self.page = prev;
                }
                OnboardingAction::None
            }
            KeyCode::Char('s') => {
                self.dont_show_again = !self.dont_show_again;
                OnboardingAction::None
            }
            KeyCode::Enter => {
                if self.is_complete() {
                    OnboardingAction::Finish
                } else if let Some(next) = self.page.next() {
                    self.page = next;
                    OnboardingAction::None
                } else {
                    OnboardingAction::Finish
                }
            }
            KeyCode::Esc => OnboardingAction::Skip,
            _ => OnboardingAction::None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let popup_width = 64.min(area.width.saturating_sub(4));
        let popup_height = (area.height.saturating_sub(4)).min(24);
        let x = (area.width.saturating_sub(popup_width)) / 2;
        let y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect {
            x,
            y,
            width: popup_width,
            height: popup_height,
        };

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BLUE()))
            .title(Span::styled(
                format!(" {} ", self.page.title()),
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        if inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(inner);

        let content_lines = match self.page {
            OnboardingPage::Welcome => self.render_welcome(),
            OnboardingPage::AuthSecurity => self.render_auth_security(),
            OnboardingPage::TerminalSetup => self.render_terminal_setup(),
            OnboardingPage::Telemetry => self.render_telemetry(),
            OnboardingPage::Help => self.render_help_page(),
        };

        let truncated: Vec<Line> = content_lines
            .into_iter()
            .take(chunks[0].height as usize)
            .collect();

        frame.render_widget(Paragraph::new(truncated), chunks[0]);

        let page_idx = self.page.index() + 1;
        let total = OnboardingPage::ALL.len();

        let mut nav_spans = vec![
            Span::styled("  [←/→] prev/next", Style::default().fg(theme::fg_dim())),
            Span::styled(
                format!("   {} {}", page_idx, total),
                Style::default().fg(theme::fg_dim()),
            ),
        ];

        if self.dont_show_again {
            nav_spans.push(Span::styled(
                "   [s] on",
                Style::default().fg(theme::GREEN()),
            ));
        } else {
            nav_spans.push(Span::styled(
                "   [s] off",
                Style::default().fg(theme::AMBER()),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(nav_spans)), chunks[1]);

        let footer = if self.is_complete() {
            Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled(
                    "[Enter] start",
                    Style::default()
                        .fg(theme::GREEN())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   [Esc] skip", Style::default().fg(theme::fg_dim())),
            ])
        } else {
            Line::from(vec![
                Span::styled("      ", Style::default()),
                Span::styled(
                    "[n] next",
                    Style::default()
                        .fg(theme::BLUE())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("   [Esc] skip", Style::default().fg(theme::fg_dim())),
            ])
        };
        frame.render_widget(Paragraph::new(footer), chunks[2]);
    }

    fn render_welcome(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Welcome to Niki",
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Niki is a hermetic multi-agent coding system.",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  It orchestrates specialized AI agents to implement",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  your task in isolated sandboxes.",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Select a text style:",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  [1] Dark  [2] Light  [3] Colorblind",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press [→] or [n] to continue...",
                Style::default().fg(theme::fg_dim()),
            )),
        ]
    }

    fn render_auth_security(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Authentication & Security",
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  1. Sign in with your provider (API key or OAuth).",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  2. Niki will use your keys for LLM calls.",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Security Note:",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  AI agents can make mistakes and there are prompt",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  injection risks. Only use with code you trust.",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press [→] or [n] to continue...",
                Style::default().fg(theme::fg_dim()),
            )),
        ]
    }

    fn render_terminal_setup(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Terminal Setup",
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Recommended settings:",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  - Option+Enter for newlines",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  - Visual bell notification",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press [→] or [n] to continue...",
                Style::default().fg(theme::fg_dim()),
            )),
        ]
    }

    fn render_telemetry(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Telemetry",
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Niki collects anonymous usage data to improve",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(Span::styled(
                "  the product. No code or prompts are ever sent.",
                Style::default().fg(theme::fg_color()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Telemetry is OFF by default.",
                Style::default().fg(theme::GREEN()),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press [→] or [n] to continue...",
                Style::default().fg(theme::fg_dim()),
            )),
        ]
    }

    fn render_help_page(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Keyboard Shortcuts",
                Style::default()
                    .fg(theme::BLUE())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("    [q] quit", Style::default().fg(theme::fg_color())),
                Span::styled(
                    "      [Esc] close/back",
                    Style::default().fg(theme::fg_color()),
                ),
            ]),
            Line::from(vec![
                Span::styled("    [p] pipeline", Style::default().fg(theme::fg_color())),
                Span::styled(
                    "    [a] agents     [d] diff",
                    Style::default().fg(theme::fg_color()),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Press [Enter] to start, or [Esc] to skip.",
                Style::default().fg(theme::fg_dim()),
            )),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingAction {
    None,
    Skip,
    Finish,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct PersistedState {
    #[serde(default)]
    onboarded: bool,
}

pub fn load_state(project_path: &Path) -> bool {
    let state_path = project_path.join(STATE_FILE);
    let content = match fs::read_to_string(&state_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let state: PersistedState = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return false,
    };
    state.onboarded
}

pub fn persist_state(project_path: &Path) {
    let state_path = project_path.join(STATE_FILE);
    let _ = fs::create_dir_all(state_path.parent().unwrap_or(Path::new(".")));

    let mut state: PersistedState = if state_path.exists() {
        fs::read_to_string(&state_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        PersistedState::default()
    };

    state.onboarded = true;

    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(&state_path, json);
    }
}

pub fn should_show_onboarding(project_path: &Path) -> bool {
    if load_state(project_path) {
        return false;
    }

    if !is_terminal() {
        return false;
    }

    if std::env::var("CI").is_ok() || std::env::var("NIKI_CI").is_ok() {
        return false;
    }

    true
}

fn is_terminal() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::isatty(libc::STDIN_FILENO) != 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_navigation() {
        let welcome = OnboardingPage::Welcome;
        assert_eq!(welcome.next(), Some(OnboardingPage::AuthSecurity));
        assert_eq!(welcome.prev(), None);

        let help = OnboardingPage::Help;
        assert_eq!(help.next(), None);
        assert_eq!(help.prev(), Some(OnboardingPage::Telemetry));
    }

    #[test]
    fn page_index() {
        assert_eq!(OnboardingPage::Welcome.index(), 0);
        assert_eq!(OnboardingPage::Help.index(), 4);
    }

    #[test]
    fn modal_creation() {
        let modal = OnboardingModal::new();
        assert_eq!(modal.page, OnboardingPage::Welcome);
        assert!(modal.dont_show_again);
        assert!(!modal.is_complete());
    }

    #[test]
    fn modal_next_navigation() {
        let mut modal = OnboardingModal::new();
        assert_eq!(modal.page, OnboardingPage::Welcome);

        modal.handle_key(KeyEvent::new(
            KeyCode::Right,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(modal.page, OnboardingPage::AuthSecurity);

        modal.handle_key(KeyEvent::new(
            KeyCode::Char('n'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(modal.page, OnboardingPage::TerminalSetup);

        modal.handle_key(KeyEvent::new(
            KeyCode::Tab,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(modal.page, OnboardingPage::Telemetry);

        modal.handle_key(KeyEvent::new(
            KeyCode::Right,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(modal.page, OnboardingPage::Help);
        assert!(modal.is_complete());
    }

    #[test]
    fn modal_prev_navigation() {
        let mut modal = OnboardingModal::new();
        modal.page = OnboardingPage::TerminalSetup;

        modal.handle_key(KeyEvent::new(
            KeyCode::Left,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(modal.page, OnboardingPage::AuthSecurity);

        modal.handle_key(KeyEvent::new(
            KeyCode::Char('p'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(modal.page, OnboardingPage::Welcome);
    }

    #[test]
    fn modal_skip_action() {
        let mut modal = OnboardingModal::new();
        let action = modal.handle_key(KeyEvent::new(
            KeyCode::Esc,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(action, OnboardingAction::Skip);
    }

    #[test]
    fn modal_finish_action() {
        let mut modal = OnboardingModal::new();
        modal.page = OnboardingPage::Help;
        let action = modal.handle_key(KeyEvent::new(
            KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(action, OnboardingAction::Finish);
    }

    #[test]
    fn modal_enter_advances_page() {
        let mut modal = OnboardingModal::new();
        assert_eq!(modal.page, OnboardingPage::Welcome);

        let action = modal.handle_key(KeyEvent::new(
            KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(action, OnboardingAction::None);
        assert_eq!(modal.page, OnboardingPage::AuthSecurity);
    }

    #[test]
    fn modal_toggle_dont_show_again() {
        let mut modal = OnboardingModal::new();
        assert!(modal.dont_show_again);

        modal.handle_key(KeyEvent::new(
            KeyCode::Char('s'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert!(!modal.dont_show_again);

        modal.handle_key(KeyEvent::new(
            KeyCode::Char('s'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert!(modal.dont_show_again);
    }

    #[test]
    fn persist_and_load_state() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path();

        assert!(!load_state(project_path));

        persist_state(project_path);
        assert!(load_state(project_path));
    }

    #[test]
    fn should_show_respects_persisted_state() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path();

        // Should show when not onboarded and stdin is a terminal (but we can't
        // control the terminal check in tests, so we test the persisted state path)
        persist_state(project_path);
        assert!(!should_show_onboarding(project_path));
    }
}
