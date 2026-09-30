//! Every advertised slash command must either do what it says or say it is not
//! wired. None of them may reach the model.
//!
//! Twelve commands printed a success message for an action they did not take:
//! "Session forked (new branch created from current state)",
//! "Thinking effort set to high", "Security audit queued (Reviewer →
//! SecurityAuditor stage)", "Added working directory", "Switched to branch",
//! "Compacted 8 previous turns into memory checkpoint". A command that reports
//! work it did not do is worse than one missing, because the user stops
//! checking — and `/compact` was destructive as well as misreported.
//!
//! The worst of them needed no command at all. Everything unmatched fell
//! through to `state.chat_log.push(("user", trimmed))` and was sent to the
//! LLM as text. `/doctor` and `/review` are both listed in the slash menu and
//! in `/help`, so a user's first slash command was likely one of them — and
//! the result was a model reply to the string "/doctor", with nothing on
//! screen saying a command had been typed.

mod common;

use std::path::PathBuf;

use niki::config::NikiConfig;
use niki::display::pages::Page as _;
use niki::display::pages::chat::ChatPage;
use niki::display::pages::chat::NOT_WIRED;
use niki::display::state::AppState;
use niki::display::tui::{ChatSubmit, DisplayEvent};

fn state() -> AppState {
    let s = AppState::new(
        "chat session".to_string(),
        NikiConfig::default(),
        PathBuf::from("."),
    );
    s.chat_width.set(80);
    s
}

/// Type `text` into the composer and press Enter, as a user would.
fn submit(s: &mut AppState, text: &str) {
    for ch in text.chars() {
        let key = ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(ch),
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        let mut page = ChatPage::new();
        page.handle_key(key, s);
    }
    let enter = ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Enter,
        ratatui::crossterm::event::KeyModifiers::NONE,
    );
    let mut page = ChatPage::new();
    page.handle_key(enter, s);
}

fn reply(s: &AppState) -> String {
    s.chat_log
        .iter()
        .map(|(r, t)| format!("{r}: {t}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// No advertised command may report success for work it did not do.
#[test]
fn no_command_reports_a_success_it_did_not_achieve() {
    // Each of these used to print a confident success line.
    for cmd in [
        "/fork",
        "/rename my session",
        "/branch feature/x",
        "/add-dir /tmp",
        "/effort high",
        "/security-review",
        "/btw",
        "/compact",
        "/model gpt-4o",
    ] {
        let mut s = state();
        s.chat_log
            .push(("user".to_string(), "hi there, a first message".to_string()));
        s.chat_log
            .push(("assistant".to_string(), "a first reply".to_string()));
        submit(&mut s, cmd);
        let out = reply(&s);
        for lie in [
            "Session forked",
            "renamed to",
            "Switched to branch",
            "Added working directory",
            "Thinking effort set",
            "Security audit queued",
            "Side question mode: type",
            "into memory checkpoint",
            "Switched model to",
        ] {
            assert!(
                !out.contains(lie),
                "`{cmd}` reported \"{lie}\" for an action it did not take:\n{out}"
            );
        }
    }
}

/// The worst class: an unknown command must never become a prompt.
#[test]
fn an_unrecognised_command_never_reaches_the_model() {
    for cmd in ["/doctor", "/review", "/totally-made-up"] {
        let mut s = state();
        submit(&mut s, cmd);
        let out = reply(&s);
        assert!(
            !s.chat_log.iter().any(|(r, t)| r == "user" && t == cmd),
            "`{cmd}` was pushed into the transcript as a user message and would have been \\
             sent to the provider:\n{out}"
        );
        assert!(!out.is_empty(), "`{cmd}` produced no explanation at all");
        // A recognised command says what it did or that it is not wired; an
        // unrecognised one says so by name.
        assert!(
            out.contains("Unknown command")
                || out.contains("Verdict")
                || out.contains("niki doctor")
                || out.contains(NOT_WIRED)
                || out.contains("shell"),
            "`{cmd}` was swallowed with no explanation:\n{out}"
        );
    }
}

/// `/compact` was destructive as well as misreported: it deleted turns from the
/// transcript and told the user it had written a memory checkpoint.
#[test]
fn compact_does_not_delete_the_transcript() {
    let mut s = state();
    for i in 0..8 {
        s.chat_log.push((
            if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
            format!("turn {i}"),
        ));
    }
    let before = s.chat_log.len();
    submit(&mut s, "/compact");
    assert_eq!(
        s.chat_log.len(),
        before + 1,
        "/compact must not remove anything from the transcript"
    );
    assert!(
        s.chat_log.iter().any(|(_, t)| t == "turn 0"),
        "the oldest turn must still be there — /compact must not delete"
    );
}

/// Anything not wired says so, in the same words, so the UI reads as one
/// surface rather than twelve independent stubs.
#[test]
fn the_not_wired_wording_is_shared() {
    let mut s = state();
    s.chat_log.push(("user".into(), "a".into()));
    submit(&mut s, "/fork");
    assert!(
        reply(&s).contains(NOT_WIRED),
        "an unimplemented command must say so"
    );
}

/// The advertised menu and the advertised help must not drift apart.
///
/// `HELP_TEXT` and `default_commands()` are two hand-maintained lists that
/// already disagreed: `/code-review` and `/security-review` were in neither,
/// while `/doctor` and `/review` were in both and did nothing.
#[test]
fn every_advertised_command_has_a_handler() {
    let s = state();
    for c in &s.commands {
        // The menu's second column is the syntax, so a command that declares
        // an argument is invoked with one. `/steer` with no argument is
        // correctly unknown — the point is that nothing the menu advertises is
        // a dead end, not that every prefix matches on its own.
        let name = if c.name.contains('<') {
            c.name.replace("<agent>", "coder")
        } else {
            c.name.clone()
        };
        if name.trim() == "/" {
            continue;
        }
        let mut s2 = state();
        submit(&mut s2, name.trim());
        let out = reply(&s2);
        assert!(
            !out.contains("Unknown command"),
            "the slash menu advertises `{name}`, which has no handler:\n{out}"
        );
    }
}

/// A Notice is a distinct channel from a turn: it must not look like something
/// the model said.
#[test]
fn a_notice_is_not_rendered_as_something_the_model_said() {
    let mut s = state();
    s.apply_display_event(DisplayEvent::Notice {
        text: "Branch blocked: test suite failed (exit 1)".to_string(),
        warning: false,
    });
    assert_eq!(s.chat_log[0].0, "notice");
    assert_ne!(s.chat_log[0].0, "assistant");
}

/// The submit payload carries the conversation, so the processor can answer in
/// context and route `/run` to the pipeline.
#[test]
fn a_submit_carries_the_conversation_before_it() {
    let submit = ChatSubmit {
        text: "and the auth module?".to_string(),
        history: vec![
            niki::llm::provider::ChatTurn::user("what modules exist?"),
            niki::llm::provider::ChatTurn::assistant("Billing, Auth, Storage."),
        ],
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        permission_mode: None,
    };
    assert_eq!(submit.history.len(), 2);
    assert_eq!(submit.history[1].role, "assistant");
}
