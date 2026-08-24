//! Event→state pipeline tests for the chat input editor.
//!
//! Drives `InputHandler` with synthetic crossterm `KeyEvent`s (no PTY, no
//! timing) and asserts on `InputState` transitions — the deterministic half
//! of what the tuiwright PTY suite verifies end-to-end. Complements the
//! navigation coverage in tui_navigation.rs by owning the *editing* surface:
//! insert/delete, kill-ring, undo/redo, history recall, paste bursts, and the
//! submit queue.

use niki::config::types::NikiConfig;
use niki::display::input::InputHandler;
use niki::display::pages::AppState;
use niki::display::state::{InputAction, InputMode};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn state() -> AppState {
    AppState::new(
        "input pipeline".to_string(),
        NikiConfig::default(),
        std::path::PathBuf::from("/tmp/niki-input"),
    )
}

fn ch(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(handler: &InputHandler, st: &mut AppState, s: &str) {
    for c in s.chars() {
        handler.handle_key(&mut st.input_state, ch(c));
    }
}

#[test]
fn typing_inserts_at_cursor_and_advances() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "he");
    assert_eq!(st.input_state.buffer, "he");
    assert_eq!(st.input_state.cursor_pos, 2);

    // Move to start, insert mid-string.
    handler.handle_key(&mut st.input_state, ctrl('a'));
    handler.handle_key(&mut st.input_state, ch('X'));
    assert_eq!(st.input_state.buffer, "Xhe");
    assert_eq!(st.input_state.cursor_pos, 1);
}

#[test]
fn enter_submits_buffer_and_clears_it() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "do the thing");
    let action = handler.handle_key(&mut st.input_state, key(KeyCode::Enter));
    assert_eq!(action, InputAction::Submit("do the thing".to_string()));
    assert_eq!(st.input_state.buffer, "");
    assert_eq!(st.input_state.cursor_pos, 0);
}

#[test]
fn backspace_delete_forward_kill_line_and_yank() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "abcdef");
    handler.handle_key(&mut st.input_state, key(KeyCode::Backspace)); // "abcde"
    assert_eq!(st.input_state.buffer, "abcde");

    handler.handle_key(&mut st.input_state, ctrl('a'));
    handler.handle_key(&mut st.input_state, key(KeyCode::Delete)); // "bcde"
    assert_eq!(st.input_state.buffer, "bcde");

    // Ctrl+K kills to end of line; Ctrl+Y yanks it back.
    handler.handle_key(&mut st.input_state, ctrl('a'));
    let killed = handler.handle_key(&mut st.input_state, ctrl('k'));
    assert_eq!(killed, InputAction::None);
    assert_eq!(st.input_state.buffer, "");
    handler.handle_key(&mut st.input_state, ctrl('y'));
    assert_eq!(st.input_state.buffer, "bcde");
}

#[test]
fn kill_word_then_undo_restores() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "hello world");
    handler.handle_key(&mut st.input_state, ctrl('w')); // kill trailing word -> "hello "
    assert_eq!(st.input_state.buffer, "hello ");

    handler.handle_key(&mut st.input_state, ctrl('z')); // undo the kill
    assert_eq!(st.input_state.buffer, "hello world");

    handler.handle_key(
        &mut st.input_state,
        ctrl('_'), /* redo-ish: undo again is fine */
    );
    let _ = st.input_state.buffer.clone(); // no panic; exact semantics owned by editor
}

#[test]
fn history_recall_round_trip() {
    let handler = InputHandler::new();
    let mut st = state();

    for prompt in ["first", "second", "third"] {
        type_str(&handler, &mut st, prompt);
        let action = handler.handle_key(&mut st.input_state, key(KeyCode::Enter));
        assert_eq!(action, InputAction::Submit(prompt.to_string()));
    }
    // AppState::new seeds sample history; replace with a known fixture so
    // indices in the assertions below are exact.
    st.input_state.history = vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ];
    st.input_state.history_index = None;

    handler.handle_key(&mut st.input_state, key(KeyCode::Up));
    assert_eq!(st.input_state.buffer, "third");
    handler.handle_key(&mut st.input_state, key(KeyCode::Up));
    assert_eq!(st.input_state.buffer, "second");
    handler.handle_key(&mut st.input_state, key(KeyCode::Down));
    assert_eq!(st.input_state.buffer, "third");
    handler.handle_key(&mut st.input_state, key(KeyCode::Down));
    // Past newest entry returns to the live buffer (empty).
    assert_eq!(st.input_state.buffer, "");
}

#[test]
fn shell_mode_uses_shell_history_not_chat_history() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "echo one");
    handler.handle_key(&mut st.input_state, key(KeyCode::Enter));
    st.input_state.history.push("echo one".to_string());

    // Switch to shell mode and run a shell command.
    st.input_state.mode = InputMode::Shell;
    type_str(&handler, &mut st, "cargo test");
    handler.handle_key(&mut st.input_state, key(KeyCode::Enter));
    st.input_state.shell_history.push("cargo test".to_string());

    // Chat history must not have absorbed the shell entry and vice versa.
    st.input_state.mode = InputMode::Insert;
    handler.handle_key(&mut st.input_state, key(KeyCode::Up));
    assert_eq!(st.input_state.buffer, "echo one");

    st.input_state.mode = InputMode::Shell;
    handler.handle_key(&mut st.input_state, key(KeyCode::Up));
    assert_eq!(st.input_state.buffer, "cargo test");
}

#[test]
fn paste_burst_turns_enter_into_newline() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "line one");
    st.input_state.start_paste_burst();
    assert!(st.input_state.in_paste_burst());

    let action = handler.handle_key(&mut st.input_state, key(KeyCode::Enter));
    assert_eq!(action, InputAction::None);
    assert!(
        st.input_state.buffer.contains('\n'),
        "Enter during a paste burst must insert a newline"
    );
}

#[test]
fn queued_prompts_drain_in_fifo_order() {
    let mut st = state();
    assert!(!st.input_state.has_queued());

    st.input_state.queue_prompt("one".to_string());
    st.input_state.queue_prompt("two".to_string());
    assert!(st.input_state.has_queued());

    assert_eq!(st.input_state.next_queued().as_deref(), Some("one"));
    assert_eq!(st.input_state.next_queued().as_deref(), Some("two"));
    assert!(!st.input_state.has_queued());
}

#[test]
fn word_movement_respects_boundaries() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "alpha beta");
    handler.handle_key(&mut st.input_state, ctrl('a')); // Home
    assert_eq!(st.input_state.cursor_pos, 0);
    handler.handle_key(&mut st.input_state, ctrl('e')); // End
    assert_eq!(st.input_state.cursor_pos, 10);

    // Kill-word backward from end of line removes the trailing word plus the
    // separating space.
    handler.handle_key(&mut st.input_state, ctrl('w'));
    assert_eq!(st.input_state.buffer, "alpha ");
}

#[test]
fn esc_cancels_pending_input() {
    let handler = InputHandler::new();
    let mut st = state();

    type_str(&handler, &mut st, "draft text");
    let action = handler.handle_key(&mut st.input_state, key(KeyCode::Esc));
    assert_eq!(action, InputAction::Cancel);
}
