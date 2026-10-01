//! The modal that puts an agent's question to the user, and takes the answer.
//!
//! This exists because `ask_user` could not reach anyone. The tool reads
//! stdin, and the interface owns stdin — it holds it in raw mode and runs its
//! own `event::read()` — so a `read_line` in the tool would race the event loop
//! for the next keypress and, in raw mode, return after a single keystroke
//! with no newline. Failing closed was correct. What was wrong was the
//! conclusion: *nobody is there to ask*. This is that somebody.
//!
//! It is deliberately **not** the permission modal. That one is a binary
//! Allow/Deny over a command, and its four options (allow once, allow always,
//! deny, deny always) have no meaning for "which database should this use?".
//! A question needs a cursor in a text field, a choice list it can read, and
//! an Esc that means "I am not answering right now" — not "no".
//!
//! Esc is therefore a **cancel, not an answer**. It sends the empty string and
//! a `cancelled` flag, so the tool can tell "the user said nothing" from "the
//! user said no". Collapsing those is how an agent ends up telling the user
//! they declined something they were never shown.

use crate::display::state::{AppState, AskRequest};
use crate::display::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// The answer, as the modal produces it.
///
/// `cancelled` is separate from an empty `text` so "Esc" and "I typed nothing
/// and pressed Enter" stay distinguishable all the way to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskAnswer {
    pub text: String,
    pub cancelled: bool,
}

/// Where the modal sits, and how tall it needs to be.
///
/// The height is driven by the content — a bare question is two lines of
/// chrome plus one, and a question with four choices is taller. A fixed height
/// would either clip the choices or leave a box of empty space around a
/// one-word question.
///
/// Every subtraction saturates. Raw `u16` subtraction panics in release builds
/// on underflow, and this runs on whatever size terminal the user has.
pub fn modal_rect(area: Rect, request: &AskRequest) -> Rect {
    let width = area.width.saturating_sub(8).max(1).min(area.width);
    let height = (content_height(request) + 2).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// Rows the body needs, excluding the border.
///
/// Derived from the same list the renderer builds, so the two cannot drift.
/// The first version counted a different set and clipped the `default:` line
/// and the footer hint off the bottom of every modal that had a default — the
/// two things a user most needs to see.
pub fn content_height(request: &AskRequest) -> u16 {
    // question, blank, choices, overflow note, blank, the typed answer,
    // the default, the hint.
    let shown = request.options.len().min(4);
    let rows = 1
        + 1
        + shown
        + usize::from(request.options.len() > 4)
        + 1
        + 1
        + usize::from(request.default.is_some())
        + 1;
    rows as u16
}

/// Answer the question, if one is open. Returns `true` when the key was used.
///
/// Every key is consumed while the modal is open. A question the user is
/// halfway through typing must not lose characters to the page behind it — the
/// composer would receive them and the answer would arrive with a prefix
/// nobody typed here.
pub fn handle_key(key: &ratatui::crossterm::event::KeyEvent, state: &mut AppState) -> bool {
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    // Gated on the same flag the permission modal uses, not on whether a
    // request happens to be present.
    //
    // The two can disagree — a payload left behind by a path that cleared the
    // flag, or a flag set before the payload arrived — and then a handler
    // gated on the *payload* takes the key, answers a question the user
    // cannot see, and reports `Consumed` for it. Gating on the flag means the
    // visible state is what decides, which is the only thing the user can
    // reason about.
    if !state.show_ask_modal {
        return false;
    }
    let Some(request) = state.ask_request.take() else {
        // The modal is up with no question behind it: take nothing, answer
        // nothing, and let the key through to whatever is below.
        return false;
    };

    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => submit(
            state,
            request,
            AskAnswer {
                text: String::new(),
                cancelled: true,
            },
        ),
        (KeyCode::Enter, _) => {
            let text = state.ask_input.trim().to_string();
            let text = if text.is_empty() {
                request.default.clone().unwrap_or_default()
            } else {
                text
            };
            submit(
                state,
                request,
                AskAnswer {
                    text,
                    cancelled: false,
                },
            )
        }
        (KeyCode::Backspace, _) => {
            // The character *before the cursor*, not the last one. A question
            // answered in the middle — going back to fix a typo — otherwise
            // deleted the wrong end of the answer.
            if state.ask_cursor > 0 {
                let at = byte_index(&state.ask_input, state.ask_cursor - 1);
                state.ask_input.remove(at);
                state.ask_cursor -= 1;
            }
            state.ask_request = Some(request);
            true
        }
        (KeyCode::Delete, _) => {
            del_char_at(&mut state.ask_input, state.ask_cursor);
            state.ask_request = Some(request);
            true
        }
        (KeyCode::Left, _) => {
            state.ask_cursor = state.ask_cursor.saturating_sub(1);
            state.ask_request = Some(request);
            true
        }
        (KeyCode::Right, _) => {
            state.ask_cursor = (state.ask_cursor + 1).min(state.ask_input.chars().count());
            state.ask_request = Some(request);
            true
        }
        (KeyCode::Home, _) => {
            state.ask_cursor = 0;
            state.ask_request = Some(request);
            true
        }
        (KeyCode::End, _) => {
            state.ask_cursor = state.ask_input.chars().count();
            state.ask_request = Some(request);
            true
        }
        // A digit picks one of the offered choices. Without it a question with
        // a fixed answer set has to be answered by retyping the whole option,
        // and a mistyped answer is a wrong answer the model will act on.
        (KeyCode::Char(c), KeyModifiers::NONE) if c.is_ascii_digit() && c != '0' => {
            let n = c.to_digit(10).unwrap_or(0) as usize;
            match request.options.get(n - 1) {
                Some(choice) => {
                    let choice = choice.clone();
                    submit(
                        state,
                        request,
                        AskAnswer {
                            text: choice,
                            cancelled: false,
                        },
                    )
                }
                // A digit with no matching choice is just a digit. Swallowing
                // it would silently eat part of an answer.
                None => {
                    insert_char(&mut state.ask_input, &mut state.ask_cursor, c);
                    state.ask_request = Some(request);
                    true
                }
            }
        }
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            insert_char(&mut state.ask_input, &mut state.ask_cursor, c);
            state.ask_request = Some(request);
            true
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
            state.ask_input.clear();
            state.ask_cursor = 0;
            state.ask_request = Some(request);
            true
        }
        // Everything else — function keys, Tab, the global bindings — is
        // swallowed, because the page behind must not act on it.
        _ => {
            state.ask_request = Some(request);
            true
        }
    }
}

fn submit(state: &mut AppState, request: AskRequest, answer: AskAnswer) -> bool {
    let _ = request.response_tx.send(answer);
    state.show_ask_modal = false;
    state.ask_input.clear();
    state.ask_cursor = 0;
    true
}

fn insert_char(buffer: &mut String, cursor: &mut usize, c: char) {
    let at = byte_index(buffer, *cursor);
    buffer.insert(at, c);
    *cursor += 1;
}

fn del_char_at(buffer: &mut String, cursor: usize) {
    let len = buffer.chars().count();
    if cursor < len {
        let at = byte_index(buffer, cursor);
        let end = byte_index(buffer, cursor + 1);
        buffer.replace_range(at..end, "");
    }
}

fn byte_index(buffer: &str, char_idx: usize) -> usize {
    buffer
        .char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(buffer.len())
}

/// Paint the question.
pub fn render_ask_user_modal(
    frame: &mut Frame,
    request: &AskRequest,
    area: Rect,
    state: &AppState,
) {
    let modal_area = modal_rect(area, request);
    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" The agent is asking ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::primary()))
        .style(Style::default().bg(theme::bg_elevated()));
    frame.render_widget(block, modal_area);

    // Nothing can be read on a terminal this small, and drawing into it
    // indexes outside the buffer — a panic, in the one moment the user cannot
    // work around. Onboarding already guards the same way.
    if modal_area.width < 8 || modal_area.height < 4 {
        return;
    }

    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            request.question.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, choice) in request.options.iter().take(4).enumerate() {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {}. ", i + 1),
                Style::default().fg(theme::primary()),
            ),
            Span::raw(choice.clone()),
        ]));
    }
    if request.options.len() > 4 {
        lines.push(Line::from(Span::styled(
            format!("  … and {} more", request.options.len() - 4),
            Style::default().fg(theme::text_dim()),
        )));
    }

    // The typed answer, with the cursor made visible. A hidden cursor in a
    // field the user is looking at is a field they cannot tell is there.
    let typed = &state.ask_input;
    let at = byte_index(typed, state.ask_cursor);
    let (before, after) = typed.split_at(at);
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme::primary())),
        Span::raw(before.to_string()),
        Span::styled(
            after.chars().next().unwrap_or(' ').to_string(),
            Style::default().add_modifier(Modifier::SLOW_BLINK | Modifier::REVERSED),
        ),
        Span::raw(after.chars().skip(1).collect::<String>()),
    ]));

    if let Some(default) = &request.default {
        lines.push(Line::from(Span::styled(
            format!("default: {default}"),
            Style::default().fg(theme::text_dim()),
        )));
    }
    lines.push(Line::from(Span::styled(
        "enter to answer · esc to skip · 1-9 to choose",
        Style::default().fg(theme::text_dim()),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn state_with_request(
        options: &[&str],
        default: Option<&str>,
    ) -> (AppState, std::sync::mpsc::Receiver<AskAnswer>) {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("t".to_string(), config, ".".into());
        let (tx, rx) = std::sync::mpsc::channel();
        state.ask_request = Some(AskRequest {
            question: "Which database should this use?".into(),
            options: options.iter().map(|s| s.to_string()).collect(),
            default: default.map(str::to_string),
            response_tx: tx,
        });
        state.show_ask_modal = true;
        (state, rx)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn a_typed_answer_comes_back_exactly_as_typed() {
        let (mut state, rx) = state_with_request(&[], None);
        for c in "postgres".chars() {
            assert!(handle_key(&key(KeyCode::Char(c)), &mut state));
        }
        assert!(handle_key(&key(KeyCode::Enter), &mut state));
        let answer = rx.try_recv().expect("an answer must arrive");
        assert_eq!(answer.text, "postgres");
        assert!(!answer.cancelled);
        assert!(!state.show_ask_modal, "the modal must close on submit");
        assert!(state.ask_input.is_empty(), "and the field must be cleared");
    }

    /// Esc is a cancel, not a "no".
    ///
    /// A question is not a permission prompt: "I have no answer for you right
    /// now" and "your answer is no" are different, and an agent that is told
    /// the second will act on it.
    #[test]
    fn esc_cancels_rather_than_answering_no() {
        let (mut state, rx) = state_with_request(&[], None);
        assert!(handle_key(&key(KeyCode::Char('x')), &mut state));
        assert!(handle_key(&key(KeyCode::Esc), &mut state));
        let answer = rx.try_recv().expect("a cancel must still answer");
        assert!(
            answer.cancelled,
            "Esc must be distinguishable from an answer, or a skipped question \
             reads as a refusal"
        );
        assert!(answer.text.is_empty());
    }

    #[test]
    fn a_digit_picks_the_choice_it_names() {
        let (mut state, rx) = state_with_request(&["sqlite", "postgres", "mysql"], None);
        assert!(handle_key(&key(KeyCode::Char('2')), &mut state));
        assert_eq!(rx.try_recv().unwrap().text, "postgres");
    }

    /// A digit with no choice behind it is just a digit.
    #[test]
    fn a_digit_with_no_choice_becomes_part_of_the_answer() {
        let (mut state, rx) = state_with_request(&["sqlite"], None);
        assert!(handle_key(&key(KeyCode::Char('7')), &mut state));
        assert!(handle_key(&key(KeyCode::Char('b')), &mut state));
        assert!(handle_key(&key(KeyCode::Enter), &mut state));
        assert_eq!(rx.try_recv().unwrap().text, "7b");
    }

    #[test]
    fn the_default_is_used_when_nothing_is_typed() {
        let (mut state, rx) = state_with_request(&[], Some("sqlite"));
        assert!(handle_key(&key(KeyCode::Enter), &mut state));
        assert_eq!(rx.try_recv().unwrap().text, "sqlite");
    }

    /// What the user typed wins over the default — otherwise a question can
    /// never be answered with anything else, which makes offering a default
    /// and a choice list at the same time a trap.
    #[test]
    fn what_the_user_typed_beats_the_default() {
        let (mut state, rx) = state_with_request(&[], Some("sqlite"));
        assert!(handle_key(&key(KeyCode::Char('p')), &mut state));
        assert!(handle_key(&key(KeyCode::Enter), &mut state));
        assert_eq!(rx.try_recv().unwrap().text, "p");
    }

    /// The cursor moves, so a mistake in the middle is fixable without
    /// clearing the whole answer.
    #[test]
    fn the_cursor_can_move_and_the_text_is_inserted_where_it_points() {
        let (mut state, rx) = state_with_request(&[], None);
        for c in "ac".chars() {
            assert!(handle_key(&key(KeyCode::Char(c)), &mut state));
        }
        assert!(handle_key(&key(KeyCode::Left), &mut state));
        assert!(handle_key(&key(KeyCode::Char('b')), &mut state));
        assert!(handle_key(&key(KeyCode::Enter), &mut state));
        assert_eq!(rx.try_recv().unwrap().text, "abc");
    }

    #[test]
    fn backspace_removes_the_character_before_the_cursor() {
        let (mut state, rx) = state_with_request(&[], None);
        for c in "abc".chars() {
            assert!(handle_key(&key(KeyCode::Char(c)), &mut state));
        }
        assert!(handle_key(&key(KeyCode::Left), &mut state));
        assert!(handle_key(&key(KeyCode::Backspace), &mut state));
        assert!(handle_key(&key(KeyCode::Enter), &mut state));
        assert_eq!(rx.try_recv().unwrap().text, "ac");
    }

    /// Nothing must escape to the page behind the modal.
    #[test]
    fn a_key_with_no_meaning_here_is_still_consumed() {
        let (mut state, _rx) = state_with_request(&[], None);
        for k in [
            key(KeyCode::Tab),
            key(KeyCode::F(5)),
            key(KeyCode::PageDown),
        ] {
            assert!(
                handle_key(&k, &mut state),
                "{:?} must be swallowed: the page behind must not act on it",
                k.code
            );
        }
    }

    #[test]
    fn nothing_is_consumed_when_no_question_is_open() {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("t".to_string(), config, ".".into());
        assert!(!handle_key(&key(KeyCode::Char('a')), &mut state));
    }

    /// The two flags can disagree, and the *visible* one has to win.
    ///
    /// A payload with no modal is the dangerous direction: the handler would
    /// take the key, answer a question nobody can see, and report it as
    /// consumed — so the key is swallowed and nothing on screen explains why.
    #[test]
    fn a_question_with_no_visible_modal_does_not_swallow_keys() {
        let (mut state, rx) = state_with_request(&[], None);
        state.show_ask_modal = false;
        assert!(
            !handle_key(&key(KeyCode::Enter), &mut state),
            "an invisible question must not consume Enter and answer itself"
        );
        assert!(
            rx.try_recv().is_err(),
            "and must not have sent an answer to anyone"
        );
        assert!(
            state.ask_request.is_some(),
            "the payload stays put, so a later modal can still show it"
        );
    }

    /// The other direction is not a silent trap: a modal with no question
    /// behind it draws nothing to answer, so it must not claim a key either.
    #[test]
    fn a_visible_modal_with_no_question_does_not_swallow_keys() {
        let config = crate::config::NikiConfig::default();
        let mut state = AppState::new("t".to_string(), config, ".".into());
        state.show_ask_modal = true;
        assert!(
            !handle_key(&key(KeyCode::Char('a')), &mut state),
            "there is nothing to answer, so the key belongs to the page"
        );
    }

    #[test]
    fn the_question_and_its_choices_are_painted() {
        let (state, _rx) = state_with_request(&["sqlite", "postgres"], Some("sqlite"));
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let request = state.ask_request.as_ref().unwrap();
        terminal
            .draw(|f| render_ask_user_modal(f, request, f.area(), &state))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect();
        for want in [
            "The agent is asking",
            "Which database should this use?",
            "1. sqlite",
            "2. postgres",
            "default: sqlite",
            "esc to skip",
        ] {
            assert!(text.contains(want), "the modal must show {want:?}:\n{text}");
        }
    }

    /// A narrow terminal must not panic.
    ///
    /// Raw `u16` subtraction panics in release builds, and this is a modal
    /// whose width is derived from the terminal's.
    #[test]
    fn a_tiny_terminal_still_renders() {
        let (state, _rx) = state_with_request(&["a", "b", "c", "d", "e", "f"], Some("a"));
        for (w, h) in [(1u16, 1u16), (3, 2), (8, 4), (20, 6)] {
            let backend = ratatui::backend::TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            let request = state.ask_request.as_ref().unwrap();
            terminal
                .draw(|f| render_ask_user_modal(f, request, f.area(), &state))
                .unwrap_or_else(|e| panic!("{w}x{h} must render: {e}"));
        }
    }
}

/// The full state machine a live run drives: an event arrives, the user
/// answers, and the interface is back to normal.
///
/// `tests/tui_smoke/cases/16_agent_asks_the_user.sh` found that after an
/// answer the run does not reach a verdict, and the modal appears to stay up.
/// Two tests already cover the halves — the ladder takes the key
/// (`an_open_question_takes_every_key`) and the tool gets the answer
/// (`a_questions_answer_reaches_the_loop_and_it_moves_on`) — so the gap is
/// between them: does the *state* actually return to normal?
///
/// It is here because it is the join. An event sets `show_ask_modal`, a
/// submit clears it, and anything in between that re-asserts the flag leaves a
/// question on screen that nobody is answering — and a footer that keeps
/// offering `enter send` for a modal whose tool has already returned.
#[test]
fn a_question_closes_when_it_is_answered() {
    use crate::display::tui::DisplayEvent;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    let config = crate::config::NikiConfig::default();
    let mut state = AppState::new("t".to_string(), config, ".".into());
    let (answer_tx, answer_rx) = std::sync::mpsc::channel();

    // What a tool does when it asks.
    state.apply_event(DisplayEvent::AskUser {
        question: "Which database?".into(),
        options: vec!["sqlite".into(), "postgres".into()],
        default: None,
        response_tx: answer_tx,
    });
    assert!(state.show_ask_modal, "the event must open the question");
    assert!(state.ask_request.is_some());
    assert!(
        crate::display::nav::text_focus_active(&state),
        "and the keyboard must belong to the question while it is open"
    );

    // What the user does.
    let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
    assert!(handle_key(&key(KeyCode::Char('p')), &mut state));
    assert_eq!(
        state.ask_input, "p",
        "typing must reach the answer, not the page"
    );
    assert!(handle_key(&key(KeyCode::Enter), &mut state));

    assert!(
        !state.show_ask_modal,
        "the modal must close once it is answered — a question still on screen \
         whose tool has already returned is a question nobody is answering"
    );
    assert!(
        state.ask_request.is_none(),
        "and the request must be taken, so nothing can answer it twice"
    );
    assert!(state.ask_input.is_empty(), "and the field must be cleared");
    assert_eq!(
        state.ask_cursor, 0,
        "and the cursor, or the next question starts mid-string"
    );
    assert!(
        !crate::display::nav::text_focus_active(&state),
        "and the keyboard must go back to the page"
    );
    // The tool is waiting on this, and it must not be waiting for ever.
    assert!(
        answer_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the answer must reach the tool that asked")
            .text
            == "p",
        "and it must be the text the user typed, not a default or a blank"
    );
}
