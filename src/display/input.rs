//! Full input handling with cursor management and key bindings.
//!
//! Matches Claude Code / Kimi Code key bindings:
//! - Enter: Submit input
//! - Escape: Close menu/modal
//! - Ctrl+C: Cancel current operation
//! - Ctrl+L: Clear screen
//! - Ctrl+P: Command palette
//! - Ctrl+T: Cycle theme
//! - Tab: Autocomplete / switch mode
//! - Up/Down: History navigation / menu navigation
//! - Ctrl+A/E: Beginning/end of line
//! - Ctrl+W: Delete word backward
//! - Ctrl+U: Delete to beginning
//! - Ctrl+K: Delete to end
//! - @: Trigger file autocomplete
//! - /: Trigger command menu (when input empty)
//! - !: Enter shell mode (when input empty)

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::state::{InputAction, InputMode, InputState};

/// Input handler that processes key events.
pub struct InputHandler;

impl InputHandler {
    pub fn new() -> Self {
        Self
    }

    /// Handle a key event in Insert mode.
    pub fn handle_insert(&self, state: &mut InputState, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Enter => {
                let content = state.buffer.clone();
                if content.trim().is_empty() {
                    return InputAction::None;
                }
                state.clear();
                InputAction::Submit(content)
            }
            KeyCode::Esc => InputAction::None,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.clear();
                InputAction::None
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Clear screen — handled at app level
                InputAction::None
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                InputAction::ToggleCommandPalette
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                InputAction::ToggleTheme
            }
            KeyCode::Tab => {
                // Trigger autocomplete if buffer contains @
                let _ = should_trigger_autocomplete(&state.buffer, state.cursor_pos);
                InputAction::None // Will be handled at app level to populate candidates
            }
            KeyCode::BackTab => InputAction::None,
            KeyCode::Up => {
                state.history_prev();
                InputAction::None
            }
            KeyCode::Down => {
                state.history_next();
                InputAction::None
            }
            KeyCode::Left => {
                state.move_left();
                InputAction::None
            }
            KeyCode::Right => {
                state.move_right();
                InputAction::None
            }
            KeyCode::Home | KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.move_to_start();
                InputAction::None
            }
            KeyCode::End | KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.move_to_end();
                InputAction::None
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                delete_word_backward(state);
                InputAction::None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                delete_to_start(state);
                InputAction::None
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                delete_to_end(state);
                InputAction::None
            }
            KeyCode::Backspace => {
                state.delete_back();
                InputAction::None
            }
            KeyCode::Delete => {
                state.delete_forward();
                InputAction::None
            }
            KeyCode::Char(c) => {
                // Check for mode-switching prefixes
                if state.buffer.is_empty() && c == '/' {
                    state.mode = InputMode::Command;
                    state.buffer.push('/');
                    state.cursor_pos = 1;
                    InputAction::None
                } else if state.buffer.is_empty() && c == '!' {
                    state.mode = InputMode::Shell;
                    state.buffer.push('!');
                    state.cursor_pos = 1;
                    InputAction::None
                } else {
                    state.insert_char(c);
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    /// Handle a key event in Command mode (slash commands).
    pub fn handle_command(&self, state: &mut InputState, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Enter => {
                let cmd = state.buffer.clone();
                state.clear();
                state.mode = InputMode::Insert;
                InputAction::Submit(cmd)
            }
            KeyCode::Esc => {
                state.clear();
                state.mode = InputMode::Insert;
                InputAction::None
            }
            KeyCode::Backspace => {
                if state.buffer.len() <= 1 {
                    state.mode = InputMode::Insert;
                }
                state.delete_back();
                InputAction::None
            }
            KeyCode::Char(c) => {
                state.insert_char(c);
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    /// Handle a key event in Shell mode (! prefix).
    pub fn handle_shell(&self, state: &mut InputState, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Enter => {
                let cmd = state.buffer.clone();
                state.clear();
                state.mode = InputMode::Insert;
                InputAction::Submit(cmd)
            }
            KeyCode::Esc => {
                state.clear();
                state.mode = InputMode::Insert;
                InputAction::None
            }
            KeyCode::Backspace => {
                if state.buffer.len() <= 1 {
                    state.mode = InputMode::Insert;
                }
                state.delete_back();
                InputAction::None
            }
            KeyCode::Char(c) => {
                state.insert_char(c);
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    /// Main dispatch: handle a key based on current input mode.
    pub fn handle_key(&self, state: &mut InputState, key: KeyEvent) -> InputAction {
        match state.mode {
            InputMode::Insert => self.handle_insert(state, key),
            InputMode::Command => self.handle_command(state, key),
            InputMode::Shell => self.handle_shell(state, key),
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if @ autocomplete should be triggered.
fn should_trigger_autocomplete(buffer: &str, cursor_pos: usize) -> bool {
    let before = &buffer[..cursor_pos];
    // Check if there's an @ that's not yet completed
    if let Some(at_pos) = before.rfind('@') {
        let after_at = &before[at_pos + 1..];
        // Trigger if no space since @ and not already a full path
        !after_at.contains(' ') && !after_at.contains('/')
    } else {
        false
    }
}

/// Delete a word backward (Ctrl+W behavior).
fn delete_word_backward(state: &mut InputState) {
    if state.cursor_pos == 0 {
        return;
    }
    let buf = &state.buffer;
    let end = state.cursor_pos;
    let before = &buf[..end];

    // Skip trailing whitespace
    let trimmed_end = before.trim_end().len();
    if trimmed_end == 0 {
        // Delete everything to start
        state.buffer.replace_range(..end, "");
        state.cursor_pos = 0;
        return;
    }

    // Find start of word
    let word_start = before[..trimmed_end].rfind(' ').map(|p| p + 1).unwrap_or(0);

    state.buffer.replace_range(word_start..end, "");
    state.cursor_pos = word_start;
}

/// Delete from cursor to start of line (Ctrl+U behavior).
fn delete_to_start(state: &mut InputState) {
    if state.cursor_pos == 0 {
        return;
    }
    state.buffer.replace_range(..state.cursor_pos, "");
    state.cursor_pos = 0;
}

/// Delete from cursor to end of line (Ctrl+K behavior).
fn delete_to_end(state: &mut InputState) {
    if state.cursor_pos >= state.buffer.len() {
        return;
    }
    state.buffer.replace_range(state.cursor_pos.., "");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn input_handler_insert_chars() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        handler.handle_key(&mut state, key(KeyCode::Char('h')));
        handler.handle_key(&mut state, key(KeyCode::Char('i')));
        assert_eq!(state.buffer, "hi");
        assert_eq!(state.cursor_pos, 2);
    }

    #[test]
    fn input_handler_submit() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        state.buffer = "hello world".to_string();
        state.cursor_pos = 11;
        let action = handler.handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(action, InputAction::Submit("hello world".to_string()));
        assert_eq!(state.buffer, "");
    }

    #[test]
    fn input_handler_command_mode() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        let action = handler.handle_key(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.mode, InputMode::Command);
        assert_eq!(state.buffer, "/");
        assert_eq!(action, InputAction::None);
    }

    #[test]
    fn input_handler_shell_mode() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        let action = handler.handle_key(&mut state, key(KeyCode::Char('!')));
        assert_eq!(state.mode, InputMode::Shell);
        assert_eq!(state.buffer, "!");
        assert_eq!(action, InputAction::None);
    }

    #[test]
    fn input_handler_backspace() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        handler.handle_key(&mut state, key(KeyCode::Char('a')));
        handler.handle_key(&mut state, key(KeyCode::Char('b')));
        handler.handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.buffer, "a");
    }

    #[test]
    fn input_handler_ctrl_a_e() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        handler.handle_key(&mut state, key(KeyCode::Char('a')));
        handler.handle_key(&mut state, key(KeyCode::Char('b')));
        handler.handle_key(&mut state, key(KeyCode::Char('c')));
        handler.handle_key(&mut state, ctrl(KeyCode::Char('a')));
        assert_eq!(state.cursor_pos, 0);
        handler.handle_key(&mut state, ctrl(KeyCode::Char('e')));
        assert_eq!(state.cursor_pos, 3);
    }

    #[test]
    fn input_handler_ctrl_u() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        handler.handle_key(&mut state, key(KeyCode::Char('a')));
        handler.handle_key(&mut state, key(KeyCode::Char('b')));
        handler.handle_key(&mut state, key(KeyCode::Char('c')));
        handler.handle_key(&mut state, key(KeyCode::Left));
        handler.handle_key(&mut state, key(KeyCode::Left));
        assert_eq!(state.cursor_pos, 1);
        handler.handle_key(&mut state, ctrl(KeyCode::Char('u')));
        assert_eq!(state.buffer, "bc");
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn input_handler_ctrl_k() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        handler.handle_key(&mut state, key(KeyCode::Char('a')));
        handler.handle_key(&mut state, key(KeyCode::Char('b')));
        handler.handle_key(&mut state, key(KeyCode::Char('c')));
        handler.handle_key(&mut state, key(KeyCode::Left));
        handler.handle_key(&mut state, key(KeyCode::Left));
        assert_eq!(state.cursor_pos, 1);
        handler.handle_key(&mut state, ctrl(KeyCode::Char('k')));
        assert_eq!(state.buffer, "a");
    }

    #[test]
    fn input_handler_ctrl_w() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        handler.handle_key(&mut state, key(KeyCode::Char('h')));
        handler.handle_key(&mut state, key(KeyCode::Char('e')));
        handler.handle_key(&mut state, key(KeyCode::Char('l')));
        handler.handle_key(&mut state, key(KeyCode::Char('l')));
        handler.handle_key(&mut state, key(KeyCode::Char('o')));
        handler.handle_key(&mut state, key(KeyCode::Char(' ')));
        handler.handle_key(&mut state, key(KeyCode::Char('w')));
        handler.handle_key(&mut state, key(KeyCode::Char('o')));
        handler.handle_key(&mut state, key(KeyCode::Char('r')));
        handler.handle_key(&mut state, key(KeyCode::Char('l')));
        handler.handle_key(&mut state, key(KeyCode::Char('d')));
        handler.handle_key(&mut state, ctrl(KeyCode::Char('w')));
        assert_eq!(state.buffer, "hello ");
    }

    #[test]
    fn input_handler_history_navigation() {
        let handler = InputHandler::new();
        let mut state = InputState::new();
        state.buffer = "first".to_string();
        state.cursor_pos = 5;
        state.clear();
        state.buffer = "second".to_string();
        state.cursor_pos = 6;
        state.clear();

        handler.handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.buffer, "second");
        handler.handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.buffer, "first");
        handler.handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.buffer, "second");
        handler.handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.buffer, "");
    }

    #[test]
    fn should_trigger_autocomplete_test() {
        assert!(should_trigger_autocomplete("@src", 4));
        assert!(!should_trigger_autocomplete("hello world", 5));
        // After a space (cursor at end), autocomplete should not trigger
        assert!(!should_trigger_autocomplete("@file.rs ", 9));
        // While typing after @, should trigger
        assert!(should_trigger_autocomplete("@file.rs", 8));
    }
}
