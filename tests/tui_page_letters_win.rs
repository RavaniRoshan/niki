//! A page's own shortcut must reach that page.
//!
//! `PageId::from_key` maps twelve letters to pages, the command palette lists
//! all twelve, and the Help page's PAGES section lists most of them. Then the
//! nav layer claimed two of them:
//!
//! ```text
//! KeyCode::Char('l') | KeyCode::Char(']') => Page(Dir::Next),
//! KeyCode::Char('h') | KeyCode::Char('[') => Page(Dir::Prev),
//! ```
//!
//! So `h` and `l` — History and TestLog — could not reach their pages from any
//! sub-page. The nav block sits above the router in `niki chat` and above
//! `global_page_jump` in `run_tui`, so the key was consumed as "previous/next
//! page" and the palette's own listed shortcut was a dead end.
//!
//! `[` and `]` keep page navigation. Nothing is lost with them, because a key
//! that is *also* a page shortcut cannot be both, and the arrows still do it.

use niki::config::types::NikiConfig;
use niki::display::nav::{Dir, NavIntent, intent_from_key, text_focus_active};
use niki::display::state::PageId;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}

fn state() -> niki::display::state::AppState {
    niki::display::state::AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

/// `h` reaches History and `l` reaches TestLog, by the one mechanism the app
/// actually uses to get there.
#[test]
fn the_page_letters_are_not_claimed_by_the_navigator() {
    let s = state();
    let focus = text_focus_active(&s);
    for c in ['h', 'l'] {
        assert_eq!(
            intent_from_key(&key(c), focus),
            None,
            "`{c}` is listed in the command palette as a page shortcut and \
             mapped by `PageId::from_key`, so the navigator must not answer for \
             it — otherwise that page is unreachable by its own key"
        );
        assert!(
            PageId::from_key(c).is_some(),
            "`{c}` should be a page letter, or the argument above is wrong"
        );
    }
    assert_eq!(PageId::from_key('h'), Some(PageId::History));
    assert_eq!(PageId::from_key('l'), Some(PageId::TestLog));
}

/// Every page letter must be unclaimed, not just the two that happened to
/// collide. A third collision would otherwise sit here unnoticed.
#[test]
fn no_page_letter_is_claimed_by_the_navigator() {
    let s = state();
    let focus = text_focus_active(&s);
    for (label, page) in [
        ('p', PageId::Pipeline),
        ('a', PageId::Agents),
        ('d', PageId::Diff),
        ('v', PageId::Verdict),
        ('c', PageId::Cost),
        ('f', PageId::Artifacts),
        ('h', PageId::History),
        (',', PageId::Config),
        ('?', PageId::Help),
        ('l', PageId::TestLog),
        ('g', PageId::Fleet),
        ('s', PageId::Session),
    ] {
        assert_eq!(
            PageId::from_key(label),
            Some(page),
            "the fixture is stale: `{label}` no longer maps to {page:?}"
        );
        assert_eq!(
            intent_from_key(&key(label), focus),
            None,
            "`{label}` ({page:?}) is a page shortcut and must not be claimed by \
             the navigator"
        );
    }
}

/// And prev/next page survives, on the keys that are not page shortcuts.
#[test]
fn page_navigation_still_works_on_the_keys_that_are_not_page_letters() {
    let s = state();
    let focus = text_focus_active(&s);
    assert_eq!(
        intent_from_key(&KeyEvent::new(KeyCode::Right, KeyModifiers::empty()), focus),
        Some(NavIntent::Page(Dir::Next))
    );
    assert_eq!(
        intent_from_key(&KeyEvent::new(KeyCode::Left, KeyModifiers::empty()), focus),
        Some(NavIntent::Page(Dir::Prev))
    );
    assert_eq!(
        intent_from_key(&key(']'), focus),
        Some(NavIntent::Page(Dir::Next))
    );
    assert_eq!(
        intent_from_key(&key('['), focus),
        Some(NavIntent::Page(Dir::Prev))
    );
}

/// The composer is still exempt: a letter typed into a text field is text.
#[test]
fn a_page_letter_in_the_composer_is_text_not_navigation() {
    let mut s = state();
    s.input_state.insert_str("hello");
    let focus = text_focus_active(&s);
    assert!(focus, "the composer must hold focus once it has text");
    for c in ['h', 'l', 'd', 'p'] {
        assert_eq!(
            intent_from_key(&key(c), focus),
            None,
            "`{c}` typed into a text field must not navigate"
        );
    }
}
