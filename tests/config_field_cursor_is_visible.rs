//! The Config form's cursor must be visible, and its cycle must have no gaps.
//!
//! Two defects, one cause: nobody could see `selected_field`, and the number
//! that cycled it was typed rather than derived.
//!
//! **It was never rendered.** `ConfigPage::render` read `selected_section` (for
//! the section's focus ring) and ignored `selected_field` entirely — written at
//! `config.rs:328` and `:333`, read nowhere. So `[Tab] next field`, which
//! batch 3 made reachable, moved a cursor nobody could see.
//!
//! **The modulus was wrong by five.** The cycle was `% 15`, hard-typed. The
//! form is built from 7 section headers, 6 spacers and 10 fields, and *one of
//! those fields is pushed inside a loop* over the agents — so the count could
//! not have been typed correctly in the first place. With 10 fields and a
//! `% 15` cycle, `Tab` walked through indices 10, 11, 12, 13 and 14 selecting
//! nothing before wrapping: **five presses with no visible change, then back
//! where you started**.
//!
//! The count is now derived from the same list that is drawn, so adding an
//! agent or a setting needs no second edit.

use niki::config::types::NikiConfig;
use niki::display::pages::config::ConfigPage;
use niki::display::pages::{AppState, Page, PageId};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn state() -> AppState {
    AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn render(page: &mut dyn Page, state: &AppState) -> String {
    let mut term = Terminal::new(TestBackend::new(110, 44)).expect("terminal");
    term.draw(|f| page.render(f, f.area(), state))
        .expect("draw");
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| {
                    let c = &buf[(x, y)];
                    let mark = if c.modifier.contains(ratatui::style::Modifier::BOLD) {
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

/// How many rows carry the **field** selection marker.
///
/// Not a bare count of `▸`: the block title also uses one, for the section
/// focus ring (`┌ ▸ niki.toml`). The field marker is inside the pane body, and
/// the first version of this helper counted both and reported 2.
fn markers(pane: &str) -> usize {
    // Stripped first: `mark_selected` bolds the marker span, so `render` emits
    // `▸` U+0001 ` ` U+0001 and the literal "│ ▸" never appears. Two earlier
    // versions of this helper matched the raw text — one counting every `▸`
    // (which also caught the block title) and one matching "│ ▸" — and both
    // found zero markers on a form that was marking one.
    pane.lines()
        .filter(|l| l.replace('\u{1}', "").contains("│ ▸"))
        .count()
}

/// The selection must be on screen. This is the assertion the defect would
/// have failed, and the reason the footer is allowed to say `[Tab] next field`.
#[test]
fn the_selected_field_is_visible() {
    let mut st = state();
    st.current_page = PageId::Config;
    let mut page = ConfigPage::new();

    let first = render(&mut page, &st);
    assert_eq!(
        markers(&first),
        1,
        "the form must mark exactly one field as selected; the cursor was \
         written and never rendered"
    );
    assert!(
        first.contains("Tab"),
        "the footer under test must advertise the key: {first}"
    );

    page.handle_key(key(KeyCode::Tab), &mut st);
    let second = render(&mut page, &st);
    assert_ne!(
        first, second,
        "`[Tab] next field` moved a cursor the user cannot see"
    );
    assert_eq!(
        markers(&second),
        1,
        "the marker must follow the selection, not accumulate"
    );
}

/// **The whole point of the slice: no dead stops.** Every press of `Tab` must
/// change what is on screen. The first version of the fix derived the count
/// and the test still could not see the wrap, so this walks the entire cycle
/// twice — once forward, once back — and requires every single press to move.
#[test]
fn the_tab_cycle_has_no_dead_stops() {
    let st = state();
    let n = ConfigPage::field_count(&st);
    assert!(
        n >= 5,
        "a form with {n} fields is too small to be the one under test"
    );

    // n presses to return home means n-1 that must each land somewhere new;
    // the nth press is the wrap. Asserting all n differ from the start is
    // asserting the cycle never wraps, which is the opposite of the truth —
    // and was the second bug in the first version of this test.
    let mut seen = Vec::new();
    let mut page = ConfigPage::new();
    let mut s = state();
    let start = render(&mut page, &s);
    for i in 0..(n - 1) {
        page.handle_key(key(KeyCode::Tab), &mut s);
        let pane = render(&mut page, &s);
        assert_ne!(
            pane,
            start,
            "press {} of {} changed nothing — a dead stop in the cycle",
            i + 1,
            n
        );
        assert!(
            !seen.contains(&pane),
            "press {} of {} repeated an earlier screen, so the cycle is \
             shorter than the field count implies",
            i + 1,
            n
        );
        seen.push(pane);
    }
    assert_eq!(
        seen.len(),
        n - 1,
        "the cycle must visit every field other than the first exactly once"
    );
    page.handle_key(key(KeyCode::Tab), &mut s);
    assert_eq!(
        render(&mut page, &s),
        start,
        "after {} presses the selection must be back where it started",
        n
    );

    // And backwards, because `Shift+Tab` wrapped to a hard-typed `14` — a row
    // that is not a field at all.
    let mut back = ConfigPage::new();
    let mut s2 = state();
    back.handle_key(key(KeyCode::BackTab), &mut s2);
    let from_top = render(&mut back, &s2);
    assert_ne!(
        from_top, start,
        "`Shift+Tab` from the first field must move somewhere real, not to a \
         hard-typed row that is not a field"
    );
    assert_eq!(
        markers(&from_top),
        1,
        "the wrapped-to field must still be marked"
    );
    for _ in 0..(n - 1) {
        back.handle_key(key(KeyCode::BackTab), &mut s2);
    }

    assert_eq!(
        render(&mut back, &s2),
        start,
        "`Shift+Tab` must return to the first field after {n} presses"
    );
}

/// The count must come from the form, not from a constant. If a future edit
/// types a number back in, this is what notices.
#[test]
fn the_field_count_is_derived_not_typed() {
    let s = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/pages/config.rs"),
    )
    .expect("config.rs must be readable");

    assert!(
        !s.contains("% 15"),
        "the hand-typed modulus is back; the form has a different number of \
         fields and one of them is pushed inside a loop"
    );
    let body = s
        .split("fn handle_key(")
        .nth(1)
        .expect("the key handler exists");
    assert!(
        body.contains("Self::field_count(state)"),
        "`Tab` and `Shift+Tab` must cycle by the derived count"
    );
    assert!(
        s.contains("fn build_form("),
        "the form builder must exist for the count to be derived from"
    );
}
