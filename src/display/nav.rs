//! Navigation: the keys a terminal UI is actually expected to respond to.
//!
//! niki's global binding table was `?`, `Ctrl+E/C/P/T`, `Tab`, `g` and `s`.
//! There was no arrow-key navigation anywhere and no `hjkl`, so a user
//! arriving from any other terminal tool had to learn a bespoke set of
//! chords with no muscle memory behind them. Codex, Claude Code and
//! OpenCode all bind arrows; that is the floor, not the ceiling.
//!
//! Three rules shaped this module:
//!
//! * **Arrows are unambiguous.** A terminal can be in a text field, where
//!   `Left`/`Right` move a caret and `Up`/`Down` walk history. So arrows only
//!   become navigation when *no* text field has focus. Getting this backwards
//!   would make the composer unusable, so the gate is explicit and tested.
//! * **`hjkl` is modal on text focus**, matching every code editor: inside a
//!   composer they are letters, outside they navigate.
//! * **Everything is a pure function** over `(key, focus, state)`, so the
//!   mapping is testable without a terminal — which is the only reason it can
//!   be trusted to be right.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::display::state::PageId;

/// Direction of travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Next,
    Prev,
}

/// What a key press means, independent of what it means doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavIntent {
    /// Move to the next/previous page.
    Page(Dir),
    /// Move the selection within the current page.
    Select(Dir),
    /// Jump straight to the Nth page (1-based).
    GotoPage(usize),
    /// Quit.
    Quit,
}

/// Whether a text field currently owns the keyboard.
///
/// Deliberately conservative: anything that can receive typed characters
/// counts, so a new focusable surface is opted *out* of navigation by default
/// rather than silently stealing arrow keys from a composer.
pub fn text_focus_active(state: &crate::display::state::AppState) -> bool {
    state.show_command_menu
        || state.show_help
        || state.show_permission_modal
        || state.reverse_search
        || state.input_state.autocomplete.is_some()
        || !state.input_state.buffer.is_empty()
}

/// Interpret a key press.
///
/// `text_focus` is [`text_focus_active`] for the current state. When it is
/// true this returns `None` for every navigational key, leaving the key for
/// the focused widget to interpret.
pub fn intent_from_key(key: &KeyEvent, text_focus: bool) -> Option<NavIntent> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    // Ctrl/Alt chords belong to the command layer, never to navigation.
    if ctrl || alt {
        return None;
    }

    // Arrows and hjkl: navigation only when nothing is being typed into.
    if !text_focus {
        match key.code {
            KeyCode::Right => return Some(NavIntent::Page(Dir::Next)),
            KeyCode::Left => return Some(NavIntent::Page(Dir::Prev)),
            KeyCode::Down => return Some(NavIntent::Select(Dir::Next)),
            KeyCode::Up => return Some(NavIntent::Select(Dir::Prev)),
            // `[` and `]` are page navigation. `h` and `l` **are not** — they
            // are page letters, and both `PageId::from_key` and the command
            // palette say so: `h` is History (`pages/help.rs`, PAGES section)
            // and `l` is TestLog (`command_palette.rs:87`). Claiming them here
            // meant neither page could ever be reached by its own key from any
            // sub-page, because the nav block runs before the router.
            //
            // Prev/next page is not lost: the arrows and `[`/`]` do it, and a
            // key that is *also* a page shortcut cannot be both.
            KeyCode::Char(']') => {
                return Some(NavIntent::Page(Dir::Next));
            }
            KeyCode::Char('[') => {
                return Some(NavIntent::Page(Dir::Prev));
            }
            KeyCode::Char('j') => return Some(NavIntent::Select(Dir::Next)),
            KeyCode::Char('k') => return Some(NavIntent::Select(Dir::Prev)),
            KeyCode::Char('q') => return Some(NavIntent::Quit),
            // Digit jumps, the way every tabbed interface works. Page 0 is
            // deliberately not bound: there is no "page zero" to land on.
            KeyCode::Char(c @ '1'..='9') => {
                return Some(NavIntent::GotoPage(c as usize - '1' as usize));
            }
            // `0` is the tenth page, the way every tabbed interface numbers.
            //
            // It was unbound, so digits covered 9 of the 14 pages and the five
            // beyond — Help, TestLog, Fleet, Session, Chat — had a letter or
            // `Tab` and nothing else. `Chat` is deliberately still not on a
            // digit: `Tab` is how you get to and from it, and a digit that
            // meant something different on one page than another is the defect
            // this whole cluster is about.
            KeyCode::Char('0') => {
                return Some(NavIntent::GotoPage(9));
            }
            _ => {}
        }
    }
    None
}

/// The next page in the canonical order, wrapping at the end.
pub fn next_page(current: PageId) -> PageId {
    step_page(current, Dir::Next)
}

/// The previous page, wrapping at the start.
pub fn prev_page(current: PageId) -> PageId {
    step_page(current, Dir::Prev)
}

fn step_page(current: PageId, dir: Dir) -> PageId {
    let all = PageId::all();
    if all.is_empty() {
        return current;
    }
    let i = current.index();
    let n = all.len();
    let next = match dir {
        Dir::Next => (i + 1) % n,
        Dir::Prev => (i + n - 1) % n,
    };
    all[next]
}

/// Move a selection index, clamped to the list rather than wrapping.
///
/// Wrapping a selection is disorienting: pressing Down at the bottom of a
/// file list and landing at the top reads as a bug, not a shortcut. Page
/// navigation wraps because there is no "outside" to land on.
pub fn step_index(len: usize, current: usize, dir: Dir) -> usize {
    if len == 0 {
        return 0;
    }
    let current = current.min(len.saturating_sub(1));
    match dir {
        Dir::Next => (current + 1).min(len - 1),
        Dir::Prev => current.saturating_sub(1),
    }
}

/// Resolve a 1-based digit to a page, if that page exists.
pub fn goto_page(n: usize) -> Option<PageId> {
    PageId::all().get(n).copied()
}

/// How many selectable rows the current page has.
///
/// Pages without a list report 0, which makes `step_index` a no-op rather
/// than an error — a page that grows a list later needs no change here.
pub fn page_item_count(state: &crate::display::state::AppState) -> usize {
    use crate::display::state::PageId;
    match state.current_page {
        PageId::Pipeline => state.stages.len(),
        PageId::Agents => state.tool_cards.len(),
        PageId::History | PageId::Session => state.chat_log.len(),
        PageId::Config => state.keybinding_overrides.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: ratatui::crossterm::event::KeyEventState::NONE,
        }
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(code)
        }
    }

    // ── Arrows navigate when nothing is being typed ────────────────
    #[test]
    fn arrows_navigate_when_no_text_field_has_focus() {
        assert_eq!(
            intent_from_key(&key(KeyCode::Right), false),
            Some(NavIntent::Page(Dir::Next))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Left), false),
            Some(NavIntent::Page(Dir::Prev))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Down), false),
            Some(NavIntent::Select(Dir::Next))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Up), false),
            Some(NavIntent::Select(Dir::Prev))
        );
    }

    /// The inverse matters more. A composer that loses the arrow keys to page
    /// navigation is unusable, and it is exactly what a naive implementation
    /// does.
    #[test]
    fn arrows_are_never_stolen_from_a_text_field() {
        for code in [
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Char('h'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
            KeyCode::Char('q'),
            KeyCode::Char('1'),
        ] {
            assert_eq!(
                intent_from_key(&key(code), true),
                None,
                "{code:?} must reach the focused text field, not the navigator"
            );
        }
    }

    #[test]
    fn hjkl_navigate_outside_a_text_field() {
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('j')), false),
            Some(NavIntent::Select(Dir::Next))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('k')), false),
            Some(NavIntent::Select(Dir::Prev))
        );
        // `h` and `l` are page letters — History and TestLog — and must not
        // be claimed here. See the arms in `intent_from_key` for why, and
        // `tests/tui_page_letters_win.rs` for what a user loses if they are.
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('l')), false),
            None,
            "`l` is the shortcut for TestLog in both `PageId::from_key` and the \
             command palette; claiming it for page navigation makes TestLog \
             unreachable by its own key"
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('h')), false),
            None,
            "`h` is the shortcut for History; claiming it makes History \
             unreachable by its own key"
        );
        // `[` and `]` keep page navigation, so nothing is lost with them.
        assert_eq!(
            intent_from_key(&key(KeyCode::Char(']')), false),
            Some(NavIntent::Page(Dir::Next))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('[')), false),
            Some(NavIntent::Page(Dir::Prev))
        );
    }

    #[test]
    fn bracket_keys_mirror_h_and_l() {
        assert_eq!(
            intent_from_key(&key(KeyCode::Char(']')), false),
            Some(NavIntent::Page(Dir::Next))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('[')), false),
            Some(NavIntent::Page(Dir::Prev))
        );
    }

    #[test]
    fn q_quits_outside_a_text_field() {
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('q')), false),
            Some(NavIntent::Quit)
        );
    }

    #[test]
    fn digit_keys_jump_to_a_page() {
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('1')), false),
            Some(NavIntent::GotoPage(0))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('4')), false),
            Some(NavIntent::GotoPage(3))
        );
        assert_eq!(
            intent_from_key(&key(KeyCode::Char('9')), false),
            Some(NavIntent::GotoPage(8))
        );
    }

    #[test]
    fn ctrl_chords_never_navigate() {
        // Ctrl+C and Ctrl+P are the command layer's; a navigation model that
        // claimed them would break cancel and the palette.
        for c in [
            KeyCode::Char('c'),
            KeyCode::Char('p'),
            KeyCode::Char('j'),
            KeyCode::Char('l'),
        ] {
            assert_eq!(
                intent_from_key(&ctrl(c), false),
                None,
                "ctrl+{c:?} must not navigate"
            );
        }
    }

    #[test]
    fn unmodified_letters_that_are_not_navigation_are_ignored() {
        for c in ['z', 'x', 'Q', '!', '.'] {
            assert_eq!(
                intent_from_key(&key(KeyCode::Char(c)), false),
                None,
                "{c:?}"
            );
        }
    }

    // ── Page stepping ──────────────────────────────────────────────
    #[test]
    fn page_navigation_wraps_at_both_ends() {
        let all = PageId::all();
        assert!(!all.is_empty());
        let first = all[0];
        let last = all[all.len() - 1];

        assert_eq!(
            next_page(last),
            first,
            "past the last page wraps to the first"
        );
        assert_eq!(
            prev_page(first),
            last,
            "before the first page wraps to the last"
        );
    }

    #[test]
    fn page_navigation_is_a_bijection() {
        // Forward from every page then backward must return to the start. A
        // page that appears twice in the ordering would break this.
        for p in PageId::all() {
            let round = prev_page(next_page(*p));
            assert_eq!(round, *p, "next-then-prev did not round-trip from {p:?}");
        }
    }

    #[test]
    fn every_page_is_reachable_by_stepping() {
        let mut seen = vec![PageId::all()[0]];
        let mut cur = PageId::all()[0];
        for _ in 0..PageId::all().len() {
            cur = next_page(cur);
            if seen.contains(&cur) {
                break;
            }
            seen.push(cur);
        }
        assert_eq!(
            seen.len(),
            PageId::all().len(),
            "some page is unreachable by arrow keys"
        );
    }

    // ── Selection stepping ─────────────────────────────────────────
    #[test]
    fn selection_clamps_rather_than_wrapping() {
        assert_eq!(
            step_index(5, 0, Dir::Prev),
            0,
            "cannot go above the first item"
        );
        assert_eq!(
            step_index(5, 4, Dir::Next),
            4,
            "cannot go past the last item"
        );
    }

    #[test]
    fn selection_steps_through_the_middle() {
        assert_eq!(step_index(5, 0, Dir::Next), 1);
        assert_eq!(step_index(5, 3, Dir::Prev), 2);
    }

    #[test]
    fn selection_on_an_empty_list_is_safe() {
        assert_eq!(step_index(0, 0, Dir::Next), 0);
        assert_eq!(step_index(0, 7, Dir::Prev), 0);
    }

    /// A list that shrinks under a stale index must not leave the selection
    /// pointing past the end — a crash here is a panic on a real keystroke.
    #[test]
    fn a_stale_index_is_pulled_back_into_range() {
        // A list that shrank under the selection (a filter, a refresh) must
        // not leave it pointing past the end — that is a panic on a real
        // keystroke, not a cosmetic bug. The index is clamped to the last
        // item, and a subsequent step moves from there.
        assert_eq!(step_index(3, 99, Dir::Next), 2, "clamped to the last item");
        assert_eq!(
            step_index(3, 99, Dir::Prev),
            1,
            "clamped, then stepped back"
        );
    }

    // ── Digit jumps ────────────────────────────────────────────────
    #[test]
    fn digit_jumps_resolve_within_the_page_list() {
        assert_eq!(goto_page(0), Some(PageId::all()[0]));
        assert_eq!(
            goto_page(PageId::all().len() - 1),
            Some(*PageId::all().last().unwrap())
        );
    }

    #[test]
    fn a_digit_beyond_the_page_list_resolves_to_nothing() {
        assert_eq!(
            goto_page(99),
            None,
            "a nonexistent page must not panic or wrap"
        );
    }

    fn test_state() -> crate::display::state::AppState {
        crate::display::state::AppState::new(
            "add a health endpoint".into(),
            crate::config::NikiConfig::default(),
            std::path::PathBuf::from("/tmp/nav-test"),
        )
    }

    #[test]
    fn a_page_with_no_list_reports_zero_and_is_a_no_op() {
        let mut st = test_state();
        st.current_page = PageId::Run;
        let n = page_item_count(&st);
        assert_eq!(n, 0);
        // A zero-length list must not move the selection or panic.
        assert_eq!(step_index(n, st.page_selection, Dir::Next), 0);
        assert_eq!(step_index(n, st.page_selection, Dir::Prev), 0);
    }

    /// The contract is "the count is whatever the page holds", not a
    /// hardcoded number — so the count is asserted against the list the page
    /// actually reads, and Up/Down is shown to traverse it.
    #[test]
    fn a_page_with_a_list_reports_its_own_length() {
        let mut st = test_state();
        st.current_page = PageId::Pipeline;
        let n = page_item_count(&st);
        assert_eq!(
            n,
            st.stages.len(),
            "the count must be the page's own row count"
        );
        if n > 1 {
            st.page_selection = 0;
            let one_down = step_index(n, st.page_selection, Dir::Next);
            assert_eq!(one_down, 1, "Down moves into the second row");
            let back = step_index(n, one_down, Dir::Prev);
            assert_eq!(back, 0, "Up returns to the first row");
        }
    }
}
