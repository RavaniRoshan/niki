//! The help a user reaches must describe the keys they actually press, and
//! every advertised shortcut must be one that gets you there.
//!
//! Three claims, three sources of drift, one generator.
//!
//! **The Help page was a hand-written copy.** Its GLOBAL section advertised
//! `[t] theme` when the binding is `ctrl+t` — a bare `t` was fixed once (it is
//! now `GlobalAction::CycleTheme` on `ctrl+t`) and the help was never updated
//! with it. It also said `[q] quit`, true on Chat and false on every sub-page,
//! where `q` goes *back*.
//!
//! **The which-key overlay disagreed with it.** The overlay builds its rows
//! from `BINDING_TABLE`, so it always matched behaviour; the page did not.
//! Two help surfaces, two answers. The page's GLOBAL rows now come from the
//! same table, so the drift cannot recur.
//!
//! **The command palette advertised a shortcut that could not work.**
//! `PageId::from_key('?')` resolves to `Help` and every other palette entry is
//! a bare letter read by `global_page_jump` — but `?` never reaches
//! `global_page_jump`, because the `ToggleHelp` binding consumes it first and
//! opens the overlay. So the entry said `?` would take you to the Help page.
//! It now names the route that works.

use niki::config::types::NikiConfig;
use niki::display::keybindings::{GlobalAction, KeyBindings};
use niki::display::pages::help::HelpPage;
use niki::display::pages::{AppState, Page, PageId};

fn state() -> AppState {
    AppState::new(
        "test task".into(),
        NikiConfig::default(),
        "/tmp/test".into(),
    )
}

fn key(c: char) -> ratatui::crossterm::event::KeyEvent {
    ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Char(c),
        ratatui::crossterm::event::KeyModifiers::empty(),
    )
}

fn help_text() -> String {
    HelpPage::new().plain_text()
}

/// The stale claim, stated on its own so a rewrite of the generator has to
/// confront it rather than inherit it.
#[test]
fn the_help_page_does_not_claim_a_bare_t_for_theme() {
    let text = help_text();
    assert!(
        !text.contains("[t] theme"),
        "the help advertises a bare `t` for theme, which is not the binding"
    );
    assert!(
        text.to_lowercase().contains("ctrl+t"),
        "the help must name the key that is actually bound: {text}"
    );
}

/// `q` is not "quit" everywhere, and a help page that says it is sends a user
/// looking for the wrong thing on 11 of 14 pages.
#[test]
fn the_help_page_does_not_say_q_is_always_quit() {
    let text = help_text();
    let q_row = text
        .lines()
        .find(|l| l.trim_start().starts_with("[q]"))
        .unwrap_or_else(|| panic!("the help must document `q`: {text}"));
    assert!(
        q_row.to_lowercase().contains("back") || q_row.to_lowercase().contains("quit"),
        "the `q` row does not say what `q` does: {q_row}"
    );
    assert!(
        !q_row.trim().ends_with("Quit NIKI"),
        "`q` is described as a plain quit, which is false on every sub-page: {q_row}"
    );
}

/// Every global the table binds must appear, so shrinking the page cannot
/// quietly shrink the help. This is the assertion that makes the page
/// *generated* rather than merely *correct today*.
#[test]
fn every_bound_global_appears_in_the_help() {
    let text = help_text();
    let (bindings, _) = KeyBindings::with_overrides(&std::collections::HashMap::new());
    for (combo, description, _) in bindings.help_rows(&[]) {
        if combo.is_empty() {
            continue;
        }
        assert!(
            text.contains(&format!("[{combo}]")),
            "the table binds `{combo}` ({description}) and the help page does \
             not list it — the page is supposed to be generated from the table"
        );
    }
}

/// A user's overrides must show up in the help, because the whole point of a
/// generated help page is that it describes *this* installation's keys.
#[test]
fn the_help_page_reflects_a_rebound_key() {
    // Keyed by binding **id**, not by the key: `with_overrides` takes
    // `id → [specs]`, and passing the key silently left the defaults in
    // place — the first version of this test asserted against a rebound `?`
    // that had never been rebound.
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("toggle_help".to_string(), vec!["ctrl+y".to_string()]);
    let (bindings, _) = KeyBindings::with_overrides(&overrides);
    let rows: Vec<String> = bindings
        .help_rows(&["toggle_help".to_string()])
        .into_iter()
        .map(|(c, _, _)| c)
        .collect();
    assert!(
        rows.iter().any(|r| r.contains("Ctrl+Y")),
        "a rebound `?` must appear in the generated rows: {rows:?}"
    );
}

/// Every other palette entry is a bare letter read by `global_page_jump`, so
/// every other entry must name a letter `PageId::from_key` actually maps.
#[test]
fn every_other_palette_shortcut_is_a_real_page_letter() {
    let palette = niki::display::command_palette::CommandPalette::new();
    for item in &palette.items {
        if item.page.is_none() {
            continue; // an action row, not a page row
        }
        if item.shortcut == "ctrl+p" {
            continue; // the palette's own key; see the test below
        }
        let first = item.shortcut.chars().next().unwrap_or(' ');
        assert!(
            PageId::from_key(first).is_some(),
            "palette entry {:?} advertises {:?}, which `global_page_jump` does \
             not map to any page — the shortcut shown cannot reach the page \
             shown",
            item.label,
            item.shortcut
        );
    }
}

/// Specifically: `?` resolves to `Help` in `from_key`, so an entry that used
/// `?` looked correct — and was unreachable, because `ToggleHelp` consumes the
/// key first. Pinned so putting `?` back is a deliberate act.
/// The theme action must name the chord, not the bare letter it used to be.
///
/// A second copy of the same drift: `tui.rs` moved the theme key from a bare
/// `t` to `ctrl+t`, and the command palette kept advertising the letter. Found
/// by `every_other_palette_shortcut_is_a_real_page_letter`, which flagged the
/// entry for a reason it was not written to catch.
#[test]
fn the_palette_theme_entry_names_the_bound_key() {
    let palette = niki::display::command_palette::CommandPalette::new();
    let theme = palette
        .items
        .iter()
        .find(|i| i.label == "theme: cycle")
        .expect("the palette lists the theme action");
    assert_eq!(
        theme.shortcut, "ctrl+t",
        "a bare `t` does nothing — the theme binding is `ctrl+t`. The palette \
         and the Help page both carried the old letter, which is why this is \
         asserted in both places."
    );
}

#[test]
fn the_palette_does_not_claim_question_mark_reaches_help() {
    let palette = niki::display::command_palette::CommandPalette::new();
    let help_entry = palette
        .items
        .iter()
        .find(|i| i.label == "help")
        .expect("the palette lists help");
    assert_ne!(
        help_entry.shortcut,
        "`?` is consumed by the ToggleHelp binding and never reaches \
         `global_page_jump`, so it cannot navigate to the Help page — which is \
         exactly why the entry was able to claim it did"
    );
    assert_eq!(
        help_entry.shortcut, "ctrl+p",
        "the entry must name the route that works"
    );
}

/// And the Help page must be a page the user can leave.
#[test]
fn the_help_page_can_be_left() {
    for closer in ['q', '?'] {
        let mut s = state();
        s.current_page = PageId::Help;
        let mut page = HelpPage::new();
        assert!(
            page.handle_key(key(closer), &mut s),
            "`{closer}` must be handled on the Help page"
        );
        assert_eq!(
            s.current_page,
            PageId::Run,
            "`{closer}` must leave the Help page; a help page you cannot leave \
             is worse than no help page"
        );
    }
}

/// `?` still opens the which-key overlay, which is what the status bar
/// advertises and the binding is named for. Asserted so the two surfaces stay
/// distinct rather than one quietly absorbing the other.
#[test]
fn question_mark_still_opens_the_which_key_overlay() {
    let (bindings, _) = KeyBindings::with_overrides(&std::collections::HashMap::new());
    assert_eq!(
        bindings.resolve(&key('?')),
        Some(GlobalAction::ToggleHelp),
        "`?` must remain the global help binding"
    );
}
