//! The Session page's Conversation tab must show the conversation.
//!
//! It showed nothing, ever. `render_conversation` read
//! `SessionState::messages`, and **nothing in the tree writes that field** —
//! not the chat view, not the pipeline, not the session processor. So the tab
//! rendered "No messages yet — start from Chat (press Tab)" on every mission,
//! permanently, while the conversation the user was looking at sat in
//! `AppState::chat_log`, one `Tab` away and invisible from here.
//!
//! A page whose first tab is permanently empty reads as a broken page, and the
//! other six tabs are placeholders besides — so the whole view is the thing a
//! user reaches with `s` from the Fleet grid.
//!
//! The tab now reads the live `chat_log`. `SessionState::messages` stays as the
//! mission-scoped store for whoever wants it; the *view* shows what the chat
//! view is showing, which is the only thing that can stay true as the
//! conversation grows.

use niki::config::types::NikiConfig;
use niki::display::pages::session::{SessionState, SessionTab, render_conversation};
use niki::display::state::AppState;
use niki::mission::Mission;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn buffer(w: u16, h: u16) -> Buffer {
    Buffer::empty(Rect::new(0, 0, w, h))
}

fn pane(buf: &Buffer) -> String {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn area(w: u16, h: u16) -> Rect {
    Rect::new(0, 0, w, h)
}

#[test]
fn the_conversation_tab_shows_the_live_transcript() {
    let log = vec![
        ("user".to_string(), "add a health endpoint".to_string()),
        ("assistant".to_string(), "Added GET /health".to_string()),
        ("user".to_string(), "now add tests".to_string()),
    ];
    let mut buf = buffer(60, 10);
    render_conversation(&log, area(60, 10), &mut buf);
    let out = pane(&buf);

    assert!(
        out.contains("add a health endpoint"),
        "the user's turn is missing: {out}"
    );
    assert!(
        out.contains("Added GET /health"),
        "the assistant's turn is missing: {out}"
    );
    assert!(
        out.contains("now add tests"),
        "the last turn is missing: {out}"
    );
    assert!(
        !out.contains("No messages yet"),
        "the tab claimed to be empty while a conversation existed: {out}"
    );
    // Roles must be labelled, or the transcript is unreadable.
    assert!(out.contains("User"), "turns must be attributed: {out}");
    assert!(out.contains("NIKI"), "turns must be attributed: {out}");
}

/// The empty case must still say so honestly, and say how to start one.
#[test]
fn an_empty_conversation_says_so_and_says_how() {
    let mut buf = buffer(60, 6);
    render_conversation(&[], area(60, 6), &mut buf);
    let out = pane(&buf);
    assert!(
        out.contains("No messages yet"),
        "an empty transcript must say it is empty: {out}"
    );
    assert!(out.contains("Tab"), "and must say how to start one: {out}");
}

/// The tail, not the head: a long conversation's most recent turns are the ones
/// the user is looking at. A page that shows turn one of ninety is a page that
/// looks broken.
#[test]
fn a_long_conversation_shows_its_most_recent_turns() {
    let log: Vec<(String, String)> = (0..90)
        .map(|i| ("user".to_string(), format!("turn-{i:03}")))
        .collect();
    let mut buf = buffer(40, 8);
    render_conversation(&log, area(40, 8), &mut buf);
    let out = pane(&buf);

    assert!(
        out.contains("turn-089"),
        "the newest turn must be visible: {out}"
    );
    assert!(
        !out.contains("turn-000"),
        "the oldest turn must have scrolled off: {out}"
    );
}

/// The other wiring point. `the_event_loop_passes_the_live_chat_log` checks
/// what the loop *passes*; this checks what the page *uses*. They are separate
/// places and they fail separately — reading `state.messages` again while the
/// loop kept passing `chat_log` compiles, renders an empty tab, and left every
/// other test in this file green on the first run.
#[test]
fn the_tab_uses_the_log_it_is_given() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/pages/session.rs"),
    )
    .expect("session.rs must be readable");
    let arm = page
        .split("SessionTab::Conversation =>")
        .nth(1)
        .map(|r| r.lines().take(12).collect::<String>())
        .expect("the Conversation arm exists");
    assert!(
        arm.contains("chat_log"),
        "the Conversation tab must render the `chat_log` it is handed, not \
         `SessionState::messages`, which nothing writes: {arm}"
    );
    assert!(
        !arm.contains("state.messages") && !arm.contains("sv.messages"),
        "the Conversation arm reads the unwritten field: {arm}"
    );
}

/// And the field the tab used to read is still empty, which is the evidence
/// that the old implementation could never have worked — whatever it was
/// reading, nothing was putting anything there.
#[test]
fn the_field_the_tab_used_to_read_is_still_unwritten() {
    let st = SessionState::new(Mission::new(
        niki::mission::MissionId("m1".into()),
        "demo mission".into(),
        "test".into(),
    ));
    assert!(
        st.messages.is_empty(),
        "if `SessionState::messages` now has a writer, the tab should read it \
         and this test should say which is the source of truth"
    );
    assert_eq!(st.active_tab, SessionTab::Conversation);
}

/// The caller must pass the live log. `render_session` gained the parameter in
/// this change, and the event loop is its only caller — a slice is the one way
/// this could have been wired wrongly and still compiled.
#[test]
fn the_event_loop_passes_the_live_chat_log() {
    let tui = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");
    let arm = tui
        .find("render_session(")
        .map(|i| tui[i..].lines().take(8).collect::<String>())
        .expect("the Session render call exists");
    assert!(
        arm.contains("&state.chat_log"),
        "render_session must receive the live chat log; passing `sv.messages` \
         is the defect this slice removes: {arm}"
    );
    // And the empty-state fallback must stay, because a mission with no
    // conversation is a real state, not a bug to hide.
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/pages/session.rs"),
    )
    .expect("session.rs must be readable");
    assert!(
        page.contains("No messages yet"),
        "the empty state must still be stated honestly"
    );
    let _ = AppState::new("t".into(), NikiConfig::default(), "/tmp".into());
}
