//! `j`/`k` must reach the page that advertises them.
//!
//! Ten pages print `[j/k] navigate` or `[j/k] scroll` in their footer. All ten
//! implement it in their own `handle_key`, against a cursor the page owns
//! privately — `DiffPage::scroll_offset`, `HelpPage::selected_section` and so
//! on.
//!
//! The navigator also claims `j`/`k`, and writes `state.page_selection`, which
//! **no renderer reads** (`grep` finds the declaration, the writes, and the
//! tests — nothing else). In `niki chat` the nav block sits above the router,
//! so the advertised key did nothing on the surface where the help overlay says
//! it works, and an invisible index moved instead.
//!
//! These assert the **rendered buffer**, not a private field, because that is
//! the claim being made: a footer that says `[j/k] scroll` is a promise about
//! what appears on screen. Reading a cursor would pass against a page that
//! scrolled the right number and drew the same thing.

use niki::config::types::NikiConfig;
use niki::display::pages::diff::DiffPage;
use niki::display::pages::help::HelpPage;
use niki::display::pages::{AppState, Page};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Modifier;

fn state() -> AppState {
    AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
}

/// Render a page to text, so two renders can be compared.
fn render(page: &mut dyn Page, state: &AppState) -> String {
    // 40 rows, not 24: at 24 the Diff page's content fills the pane and its own
    // footer is off-screen, so a test asserting the footer advertises `[j/k]`
    // was asserting on something the user could not see either.
    let mut term = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
    term.draw(|f| page.render(f, f.area(), state))
        .expect("draw");
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| {
                    let c = &buf[(x, y)];
                    // Style matters. The Help page marks the selected section
                    // with `BOLD` and a different toggle colour and changes no
                    // characters at all; capturing `symbol()` only reported a
                    // working page as broken.
                    let mark = if c.modifier.contains(Modifier::BOLD) {
                        "\u{1}"
                    } else {
                        ""
                    };
                    format!("{}{}", c.symbol(), mark)
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same render with the style markers removed, for substring assertions.
///
/// `render` interleaves `\u{1}` after every bold character, so a literal
/// search for `[j/k]` cannot match a footer that is entirely bold — the test
/// was reporting a missing footer that was on screen the whole time.
fn plain(rendered: &str) -> String {
    rendered.replace('\u{1}', "")
}

/// A diff **longer than the pane**, so scrolling has somewhere to go.
///
/// The first version used a 20-line diff, which fits entirely inside a 40-row
/// content area. The page then computes `max_scroll = lines - view_height = 0`,
/// clamps every `j` to zero, and renders identically — so the two scroll
/// assertions passed for the wrong reason and would have passed against a
/// completely broken page. The same trap as the pty case, in a unit test.
fn long_diff(lines: usize) -> String {
    let mut d = String::from(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,20 +1,21 @@\n",
    );
    for i in 1..=lines {
        d.push_str(&format!("+line {i:04} added\n"));
    }
    d
}

#[test]
fn j_scrolls_a_page_whose_footer_says_it_does() {
    let mut state = state();
    state.diff_content = Some(long_diff(200));

    let mut page = DiffPage::new();
    let top = render(&mut page, &state);

    for _ in 0..4 {
        page.handle_key(key('j'), &mut state);
    }
    let scrolled = render(&mut page, &state);

    assert_ne!(
        top, scrolled,
        "`j` did not scroll the Diff page, whose footer says `[j/k] scroll`"
    );
    let top_text = plain(&top);
    // No brackets here: this footer reads "j/k scroll", where Pipeline, Agents,
    // Artifacts, History, TestLog and Verdict read "[j/k] navigate". The
    // inconsistency is cosmetic and is left for ROADMAP; the assertion matches
    // what the page actually draws, because asserting `[j/k]` reported a
    // missing footer that was on screen the whole time.
    assert!(
        top_text.contains("j/k"),
        "the footer under test must actually advertise the key: {top_text}"
    );
}

#[test]
fn k_scrolls_back() {
    let mut state = state();
    state.diff_content = Some(long_diff(200));

    let mut page = DiffPage::new();
    let top = render(&mut page, &state);
    for _ in 0..4 {
        page.handle_key(key('j'), &mut state);
    }
    for _ in 0..4 {
        page.handle_key(key('k'), &mut state);
    }
    let back = render(&mut page, &state);

    assert_eq!(top, back, "`k` did not scroll back to the top");
}

#[test]
fn k_at_the_top_does_not_wrap_to_the_bottom() {
    let mut state = state();
    state.diff_content = Some(long_diff(200));

    let mut page = DiffPage::new();
    page.handle_key(key('k'), &mut state);
    page.handle_key(key('k'), &mut state);
    let after = render(&mut page, &state);

    let mut fresh = DiffPage::new();
    assert_eq!(
        render(&mut fresh, &state),
        after,
        "`k` at the top moved the view instead of staying put"
    );
}

/// The Help page's cursor is static content, so it moves with no run at all.
#[test]
fn j_moves_the_help_sections() {
    let mut state = state();
    let mut page = HelpPage::new();
    let first = render(&mut page, &state);
    page.handle_key(key('j'), &mut state);
    let second = render(&mut page, &state);

    assert_ne!(
        first, second,
        "`[j/k] navigate sections` on the Help page did not navigate"
    );
    let first_text = plain(&first);
    assert!(
        first_text.contains("j/k"),
        "the footer under test must advertise the key: {first_text}"
    );
}

/// The rule the two event loops share, pinned from outside so they cannot
/// drift apart again.
#[test]
fn the_navigator_defers_to_a_subpage() {
    use niki::display::nav::{NavIntent, intent_from_key, text_focus_active};
    let s = state();
    let focus = text_focus_active(&s);

    // A key the navigator must not interpret on a sub-page.
    let claimed = |c: char| {
        !focus
            && intent_from_key(&key(c), focus)
                .is_some_and(|i| matches!(i, NavIntent::Select(_) | NavIntent::Quit))
    };
    assert!(claimed('j'), "the navigator still claims `j` on a sub-page");
    assert!(claimed('k'), "the navigator still claims `k` on a sub-page");
    assert!(claimed('q'), "the navigator still claims `q` on a sub-page");

    // Digits and page letters stay the navigator's: they are global
    // navigation, and no page has an opinion about them.
    let global = |c: char| {
        !focus
            && intent_from_key(&key(c), focus)
                .is_some_and(|i| !matches!(i, NavIntent::Select(_) | NavIntent::Quit))
    };
    assert!(global('4'), "digit jumps must remain global navigation");
    // Page letters are a different mechanism entirely — `global_page_jump`, not
    // the nav layer — so they are asserted where they actually live. The first
    // version of this test asserted them against `intent_from_key`, which does
    // not handle letters at all, and so "failed" a path that works.
    assert_eq!(
        niki::display::state::PageId::from_key('d'),
        Some(niki::display::state::PageId::Diff),
        "page letters must remain global navigation"
    );
}

/// **Both event loops must consult the rule before the navigator.** This is the
/// assertion that can actually fail.
///
/// The five tests above all pass with the routing defect fully reinstated —
/// verified, by re-arming the nav block in both loops and running them. They
/// drive `DiffPage::handle_key` and `HelpPage::handle_key` directly, which is
/// exactly the code that was *always* correct: the page handlers were never
/// broken, they were simply never called. And `the_navigator_defers_to_a_subpage`
/// asks `intent_from_key`, which `nav.rs` answers the same way in every version
/// — it describes the navigator, not the decision to consult it.
///
/// So the property under test is the *ordering inside `tui.rs`*, and the only
/// way to see it from a test is to read the source. That is a weak kind of
/// check and it is here anyway, with the weakness written down, because the
/// alternative is shipping a green test that proves nothing.
#[test]
fn both_event_loops_gate_the_nav_block() {
    let s = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");

    assert!(
        s.contains("fn sub_page_owns("),
        "the rule the two loops share is gone"
    );
    // Two guards, because there are two event loops.
    let guards = s.matches("!sub_page_owns(key, &state)").count();
    assert_eq!(
        guards, 2,
        "expected both event loops to gate the nav block on `sub_page_owns`; \
         found {guards}. One of them is taking the key before the page can, \
         which is the whole defect: the page handlers were always correct and \
         were simply never called."
    );
    assert!(
        !s.contains("NavIntent::Quit => break,"),
        "`NavIntent::Quit` breaking the loop is what made 11 page handlers dead"
    );
}
