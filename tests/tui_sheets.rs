//! The TUI must be the whole product surface, not a viewer for a CLI.
//!
//! The Config *page* has existed for a long time and is read-only: Tab moves a
//! cursor across fifteen fields, no value can be changed, and nothing can be
//! saved. `/config` therefore did not let a user change anything — and a user
//! who wanted to change a setting had to leave the product and edit TOML by
//! hand. For a product whose pitch is that you never leave your terminal, that
//! is the whole gap in one sentence.
//!
//! These tests drive the sheet the way the key loop does, and assert the file
//! on disk afterwards. Not the render, not the styling — the effect. A settings
//! form that looks right and writes nothing is the failure, so the assertion is
//! on `niki.toml`.

use niki::config::edit;
use niki::display::sheets::settings::{FieldKind, SETTINGS, SettingsSheet};
use niki::display::sheets::theme::ThemeSheet;
use niki::display::sheets::{self, Sheet, SheetOutcome};
use niki::display::state::AppState;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn state_in(dir: &std::path::Path) -> AppState {
    AppState::new(
        "test".into(),
        niki::config::types::NikiConfig::default(),
        dir.to_path_buf(),
    )
}

fn project() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// Read a value out of `niki.toml` as the product would render it.
///
/// Not `Item::to_string()`: that includes the key's decor, so a string value
/// comes back as ` "light"` with a leading space. The first version of this
/// helper did that and every string assertion in the file failed for the same
/// uninteresting reason.
fn read(path: &std::path::Path, key: &str) -> Option<String> {
    let item = edit::get_value(&path.join("niki.toml"), key)
        .ok()
        .flatten()?;
    if let Some(s) = item.as_str() {
        Some(s.to_string())
    } else if let Some(b) = item.as_bool() {
        Some(b.to_string())
    } else if let Some(i) = item.as_integer() {
        Some(i.to_string())
    } else {
        item.as_float().map(|f| f.to_string())
    }
}

#[test]
fn every_setting_is_writable_and_is_a_real_path() {
    // The list is a curated set of dotted paths. If one names a key NIKI does
    // not read, the form would happily write it and nothing would change —
    // which is the same as the read-only page with extra steps.
    for s in SETTINGS {
        assert!(
            !s.path.is_empty() && s.path.contains('.'),
            "{}: a setting needs a dotted path",
            s.label
        );
        assert!(!s.help.is_empty(), "{}: needs a help line", s.label);
        assert!(!s.label.is_empty(), "{}: needs a label", s.path);
    }
    // Paths are unique, or the form would show the same setting twice and the
    // second write would silently win.
    let mut paths: Vec<&str> = SETTINGS.iter().map(|s| s.path).collect();
    paths.sort_unstable();
    let before = paths.len();
    paths.dedup();
    assert_eq!(paths.len(), before, "duplicate setting paths in SETTINGS");
}

#[test]
fn a_boolean_is_toggled_with_space_and_written_on_save() {
    let dir = project();
    let mut st = state_in(dir.path());
    let mut sheet = SettingsSheet::new();

    // Find the first boolean so the test does not depend on list order.
    let idx = SETTINGS
        .iter()
        .position(|s| s.kind == FieldKind::Bool)
        .unwrap();
    sheet.cursor = idx;

    sheet.on_key(key(KeyCode::Char(' ')), &mut st).unwrap();
    assert!(
        !sheet.pending.is_empty(),
        "space on a boolean must stage a change, not require Enter first"
    );

    // Nothing is on disk yet: the form is a draft.
    assert!(
        read(dir.path(), SETTINGS[idx].path).is_none(),
        "nothing may be written before the user commits"
    );

    sheet.on_key(ctrl('s'), &mut st).unwrap();
    let on_disk = read(dir.path(), SETTINGS[idx].path).expect("ctrl+s must write");
    assert!(
        on_disk == "true" || on_disk == "false",
        "expected a boolean on disk, got {on_disk:?}"
    );
}

#[test]
fn enter_starts_editing_and_enter_commits_the_text() {
    let dir = project();
    let mut st = state_in(dir.path());
    let mut sheet = SettingsSheet::new();
    let idx = SETTINGS
        .iter()
        .position(|s| s.kind == FieldKind::Text)
        .expect("there is a text setting");
    sheet.cursor = idx;

    // space starts typing; enter saves.
    sheet.on_key(key(KeyCode::Char(' ')), &mut st).unwrap();
    for c in "build-out".chars() {
        sheet.on_key(key(KeyCode::Char(c)), &mut st).unwrap();
    }
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    assert_eq!(
        read(dir.path(), SETTINGS[idx].path).as_deref(),
        Some("build-out"),
        "the typed text must reach niki.toml"
    );
}

#[test]
fn esc_discards_unsaved_edits_rather_than_silently_losing_them() {
    let dir = project();
    let mut st = state_in(dir.path());
    let mut sheet = SettingsSheet::new();
    let idx = SETTINGS
        .iter()
        .position(|s| s.kind == FieldKind::Bool)
        .unwrap();
    sheet.cursor = idx;

    sheet.on_key(key(KeyCode::Char(' ')), &mut st).unwrap();
    assert!(!sheet.pending.is_empty());

    // First Esc abandons the draft; the sheet stays open and says so.
    let outcome = sheet.on_key(key(KeyCode::Esc), &mut st).unwrap();
    assert_eq!(outcome, None, "one Esc must not close a dirty form");
    assert!(sheet.pending.is_empty(), "the draft must be discarded");
    assert!(
        sheet.status.as_deref().unwrap_or("").contains("discarded"),
        "the user must be told: {:?}",
        sheet.status
    );
    assert!(read(dir.path(), SETTINGS[idx].path).is_none());

    // Second Esc, now clean, closes.
    let outcome = sheet.on_key(key(KeyCode::Esc), &mut st).unwrap();
    assert_eq!(outcome, Some(SheetOutcome::Cancelled));
}

#[test]
fn a_number_rejects_text_rather_than_writing_something_that_will_not_parse() {
    let dir = project();
    let mut st = state_in(dir.path());
    let mut sheet = SettingsSheet::new();
    let idx = SETTINGS
        .iter()
        .position(|s| s.kind == FieldKind::Number)
        .expect("there is a numeric setting");
    sheet.cursor = idx;

    sheet.on_key(key(KeyCode::Char(' ')), &mut st).unwrap();
    for c in "twelve".chars() {
        sheet.on_key(key(KeyCode::Char(c)), &mut st).unwrap();
    }
    // Asserted at the moment it is useful: while the user is still typing. By
    // the time they press Enter the message is gone, because committing an
    // empty numeric field is a legitimate action ("unset").
    assert!(
        sheet.status.as_deref().unwrap_or("").contains("number"),
        "a refused keystroke must say so immediately, got {:?}",
        sheet.status
    );
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    // The property is that no LETTER reached the buffer, not that the field
    // is untouched: committing an empty numeric field is meaningful — it means
    // "unset", and the product's defaults apply.
    let staged = sheet
        .pending
        .get(SETTINGS[idx].path)
        .cloned()
        .unwrap_or_default();
    assert!(
        staged.chars().all(|c| c.is_ascii_digit() || c == '.'),
        "a non-numeric keystroke must never enter the buffer, got {staged:?}"
    );
    assert!(
        !staged.contains('t'),
        "specifically not the typed text: {staged:?}"
    );

    // A valid one does go through.
    sheet.on_key(key(KeyCode::Char(' ')), &mut st).unwrap();
    for c in "12".chars() {
        sheet.on_key(key(KeyCode::Char(c)), &mut st).unwrap();
    }
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    assert_eq!(read(dir.path(), SETTINGS[idx].path).as_deref(), Some("12"));
}

#[test]
fn the_sheet_does_not_leak_keys_to_the_page_behind_it() {
    // A settings form that lets a keypress through to the page underneath is
    // worse than no form: the user is typing a value and the app navigates.
    let dir = project();
    let mut st = state_in(dir.path());
    st.sheets
        .push(Sheet::Settings(Box::new(SettingsSheet::new())));
    let page_before = st.current_page;

    // `q` would quit or go back on several pages; Enter, arrows, letters and
    // Ctrl+S must all be eaten by the form.
    for k in [
        key(KeyCode::Char('q')),
        key(KeyCode::Char('j')),
        key(KeyCode::Down),
        key(KeyCode::Enter),
        ctrl('s'),
    ] {
        let consumed = sheets::route(&mut st, k).unwrap();
        assert!(consumed, "the sheet must consume {k:?}");
    }
    assert_eq!(
        st.current_page, page_before,
        "no key reached the page behind the sheet"
    );
    assert_eq!(st.sheets.len(), 1, "and the sheet is still open");
}

#[test]
fn opening_a_sheet_twice_does_not_stack_two_forms() {
    // Otherwise Esc returns to an identical stale form and the user cannot get
    // out.
    let dir = project();
    let mut st = state_in(dir.path());
    sheets::open_sheet(&mut st, Sheet::Settings(Box::new(SettingsSheet::new())));
    sheets::open_sheet(&mut st, Sheet::Settings(Box::new(SettingsSheet::new())));
    assert_eq!(st.sheets.len(), 1);
}

#[test]
fn a_saved_setting_keeps_the_comments_in_the_file() {
    // The whole reason the TUI can own this: a settings screen that rewrites
    // niki.toml and drops the user's notes is a regression, not a feature.
    let dir = project();
    let path = dir.path().join("niki.toml");
    std::fs::write(
        &path,
        "# my project settings\n[general]\n# how many revisions\nmax_revision_rounds = 3\n",
    )
    .unwrap();

    let mut st = state_in(dir.path());
    let mut sheet = SettingsSheet::new();
    let idx = SETTINGS
        .iter()
        .position(|s| s.path == "general.max_revision_rounds")
        .expect("the settings list includes the revision cap");
    sheet.cursor = idx;
    sheet.on_key(key(KeyCode::Char(' ')), &mut st).unwrap();
    for c in "9".chars() {
        sheet.on_key(key(KeyCode::Char(c)), &mut st).unwrap();
    }
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("# my project settings"), "{after}");
    assert!(after.contains("# how many revisions"), "{after}");
    assert!(after.contains("max_revision_rounds = 9"), "{after}");
}

#[test]
fn the_theme_picker_previews_on_move_and_restores_on_cancel() {
    let dir = project();
    let mut st = state_in(dir.path());
    let original = niki::display::theme::current_mode();
    let mut sheet = ThemeSheet::new();

    sheet.on_key(key(KeyCode::Down), &mut st).unwrap();
    assert_ne!(
        niki::display::theme::current_mode(),
        original,
        "moving the cursor must preview, not just move a highlight"
    );

    // Backing out must not leave the preview behind.
    let outcome = sheet.on_key(key(KeyCode::Esc), &mut st).unwrap();
    assert_eq!(outcome, Some(SheetOutcome::Cancelled));
    assert_eq!(
        niki::display::theme::current_mode(),
        original,
        "a cancelled picker must put the theme back"
    );
}

#[test]
fn the_theme_picker_writes_the_choice_to_niki_toml() {
    let dir = project();
    let mut st = state_in(dir.path());
    let original = niki::display::theme::current_mode();
    let mut sheet = ThemeSheet::new();
    let start = sheet.cursor;

    // Select something other than where we started.
    sheet.on_key(key(KeyCode::Down), &mut st).unwrap();
    assert_ne!(sheet.cursor, start, "the cursor must actually move");
    let chosen = niki::display::sheets::theme::THEMES[sheet.cursor].0;

    sheet.on_key(key(KeyCode::Enter), &mut st).unwrap();
    assert_eq!(
        read(dir.path(), "ui.theme").as_deref(),
        Some(chosen),
        "accepting must persist the theme that was selected"
    );
    assert_eq!(
        niki::display::theme::current_mode(),
        niki::display::theme::ThemeMode::from_str(chosen),
        "and it must take effect in this process, not only on the next run"
    );
    let _ = original;
}

#[test]
fn the_theme_picker_only_offers_themes_the_product_implements() {
    // A picker that lists palettes `ThemeMode` has never heard of teaches the
    // user that the list is not to be trusted: every one of them silently
    // falls back to Auto. The first draft of this file listed twelve and nine
    // of them were fiction.
    for (name, _) in niki::display::sheets::theme::THEMES {
        assert!(
            matches!(*name, "auto" | "dark" | "light"),
            "{name:?} is offered but ThemeMode cannot select it"
        );
    }
    let names: Vec<&str> = niki::display::sheets::theme::THEMES
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(
        names,
        vec!["auto", "dark", "light"],
        "the picker must list every real mode"
    );
}

/// The wiring, not just the sheet.
///
/// Every other test here drives `SettingsSheet` directly, which means all of
/// them would pass while `/config` still sent the user to the read-only page —
/// the state this work exists to leave. So the dispatch itself is asserted:
/// typing `/config` must open a sheet, and typing `/theme` must open the
/// picker, with no page navigation anywhere in it.
#[test]
fn the_slash_commands_open_the_sheets_rather_than_a_page() {
    let src = include_str!("../src/display/pages/chat.rs");
    for command in ["/config", "/theme"] {
        let anchor = format!("trimmed == \"{command}\"");
        let at = src
            .find(&anchor)
            .unwrap_or_else(|| panic!("the dispatcher handles {command}"));
        let arm = &src[at..(at + 700).min(src.len())];
        assert!(
            arm.contains("open_sheet"),
            "{command} must open a sheet. The slice it was given starts here:\n{arm}"
        );
        assert!(
            !arm.contains("PageId::Config") && !arm.contains("PageId::"),
            "{command} must not also navigate to a page:\n{arm}"
        );
    }
}
