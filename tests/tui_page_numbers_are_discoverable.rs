//! A digit must mean one page, and the user must be able to find out which.
//!
//! Two problems, and the second is the one that mattered.
//!
//! **Digits covered 9 of 14 pages.** `nav.rs` answered `Char('1'..='9')` and
//! nothing else, so `PageId::all()[9..]` — Help, TestLog, Fleet, Session,
//! Chat — had a letter or `Tab` and no number. `0` is the tenth page now, the
//! way every tabbed interface numbers, which takes it to 10 of 14.
//!
//! **The numbering was undiscoverable.** `GotoPage(n)` resolves through
//! `PageId::all()`, so the *internal* order of a Rust enum was the numbering a
//! user had to guess. The Help page now has a `PAGE NUMBERS` section generated
//! from `all()` and from `PageId::shortcut()`, so it cannot drift from either.
//!
//! `Chat` deliberately stays off a digit. `Tab` is how you reach and leave it,
//! and a digit that meant one thing on nine pages and another on the tenth
//! would be the same defect this whole cluster is about.

use niki::config::types::NikiConfig;
use niki::display::nav::{NavIntent, intent_from_key, text_focus_active};
use niki::display::pages::help::HelpPage;
use niki::display::state::PageId;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn state() -> niki::display::state::AppState {
    niki::display::state::AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}

/// Every digit must land on the page the numbering says it does.
#[test]
fn each_digit_reaches_the_page_the_order_claims() {
    let all = PageId::all();
    assert!(
        all.len() >= 10,
        "the fixture assumes at least ten pages; the tree has {}",
        all.len()
    );
    let s = state();
    let focus = text_focus_active(&s);

    for (i, page) in all.iter().enumerate().take(10) {
        let digit = if i + 1 == 10 {
            '0'
        } else {
            char::from_digit((i + 1) as u32, 10).expect("1-9 is a digit")
        };
        match intent_from_key(&key(digit), focus) {
            Some(NavIntent::GotoPage(n)) => {
                assert_eq!(
                    all.get(n),
                    Some(page),
                    "digit `{digit}` claims {page:?} but resolves to {:?}",
                    all.get(n)
                );
            }
            other => panic!("digit `{digit}` is not a page jump: {other:?}"),
        }
    }
}

/// And no two digits may land on the same page — a numbering that aliases is
/// worse than one that stops early.
#[test]
fn no_two_digits_reach_the_same_page() {
    let s = state();
    let focus = text_focus_active(&s);
    let mut seen: Vec<(char, PageId)> = Vec::new();
    for digit in "1234567890".chars() {
        if let Some(NavIntent::GotoPage(n)) = intent_from_key(&key(digit), focus) {
            let page = PageId::all()[n];
            assert!(
                !seen.iter().any(|(_, p)| *p == page),
                "digit `{digit}` and an earlier digit both reach {page:?}"
            );
            seen.push((digit, page));
        }
    }
    assert_eq!(seen.len(), 10, "all ten digits must be bound");
}

/// The help must say what the digits mean — that was the whole point.
#[test]
fn the_help_documents_every_digit() {
    let text = HelpPage::new().plain_text();
    assert!(
        text.contains("PAGE NUMBERS"),
        "the help has no page-number section, so the numbering is still only \
         knowable from the source: {text}"
    );
    for (i, page) in PageId::all().iter().enumerate().take(10) {
        let digit = if i + 1 == 10 {
            "0".to_string()
        } else {
            (i + 1).to_string()
        };
        let row = text
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{digit}  ")))
            .unwrap_or_else(|| panic!("digit `{digit}` ({page:?}) is not listed: {text}"));
        assert!(
            row.contains(page.title()),
            "digit `{digit}` should name the {} page, and says: {row}",
            page.title()
        );
    }
}

/// And the pages past the digits must be listed with the key that reaches
/// them, rather than the list stopping at `9` and going quiet.
#[test]
fn the_pages_past_the_digits_say_how_to_reach_them() {
    let text = HelpPage::new().plain_text();
    for page in PageId::all().iter().skip(10) {
        let key = page
            .shortcut()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "Tab".to_string());
        assert!(
            text.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with(&format!("{key}  ")) && l.contains(page.title())
            }),
            "{page:?} has no digit, so the help must name `{key}` as the way \
             to it: {text}"
        );
    }
}

/// `PageId::shortcut` is the inverse of `PageId::from_key`, so it cannot name
/// a key that does not work. Asserted over every page, because the inverse is
/// only useful if it is total.
#[test]
fn every_pages_shortcut_actually_reaches_it() {
    for page in PageId::all() {
        if let Some(c) = page.shortcut() {
            assert_eq!(
                PageId::from_key(c),
                Some(*page),
                "{page:?} claims `{c}` reaches it, and `from_key` disagrees"
            );
        }
    }
    // Run and Chat are the two the product reaches another way; if either
    // grows a letter this test stops asserting nothing for them.
    assert_eq!(
        PageId::Run.shortcut(),
        None,
        "Run is reached with Esc, not a letter"
    );
    assert_eq!(
        PageId::Chat.shortcut(),
        None,
        "Chat is reached with Tab, not a letter"
    );
}
