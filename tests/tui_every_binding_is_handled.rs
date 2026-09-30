//! `ROADMAP.md` §1's exit criterion, made into a test.
//!
//! > every key in `BINDING_TABLE` and every footer hint maps to an
//! > implemented handler, asserted by a test that enumerates the table rather
//! > than a hand-copied list
//!
//! Enumerating the table found two actions that `run_chat` never handled:
//! `GotoFleet` (`g`) and `GotoSession` (`s`).
//!
//! - **`g` did nothing at all** in `niki chat`, on any page.
//! - **`s` navigated to a blank screen.** It fell through to
//!   `global_page_jump`, which sets `current_page = Session` without opening a
//!   session; the Session page renders only when `state.session_view` is
//!   `Some`, so the user got an empty page and no way back except `Esc`.
//!
//! Both are advertised: `g` and `s` are in the command palette, in the Help
//! page's sections, and — for `s` — the Help page's own `PAGE NUMBERS` table.
//!
//! `global_page_jump` now also refuses Fleet and Session outright, because a
//! bare `current_page =` is precisely the call that left a blank screen. They
//! are reachable through `apply_nav_action` and nothing else.

use niki::config::types::NikiConfig;
use niki::display::keybindings::{BINDING_TABLE, BindingDef};
use niki::display::state::AppState;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn tui() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable")
}

/// Where a key can be handled: the two event loops, and the helpers **both**
/// of them call.
///
/// The helpers are named rather than inferred. An action handled in a function
/// only one loop calls is a divergence, and a file-wide search cannot tell the
/// two apart — which is how the first version of this test reported five
/// actions as dead in a loop that handles them through the shared overlay
/// ladder, and then, after moving `g`/`s` into a helper that sits *after* both
/// loops, reported them as dead in the other one.
///
/// Naming the shared functions is the honest version: a new shared helper is
/// added here deliberately, with its call sites, or not at all.
struct Regions {
    run_tui: String,
    run_chat: String,
    shared: Vec<(&'static str, String)>,
}

fn fn_body(src: &str, header: &str) -> Option<String> {
    let start = src.find(header)?;
    let rest = &src[start..];
    let end = rest[1..].find("\n}\n").map(|i| i + 3)?;
    Some(rest[..end].to_string())
}

fn regions() -> Regions {
    let s = tui();
    let tui_at = s.find("fn run_tui(").expect("run_tui must exist");
    let chat_at = s.find("pub fn run_chat(").expect("run_chat must exist");
    assert!(
        tui_at < chat_at,
        "the two loops moved relative to each other"
    );
    let run_tui = s[tui_at..chat_at].to_string();
    let run_chat = s[chat_at..].to_string();

    let mut shared = Vec::new();
    for name in ["fn route_overlay_key(", "fn apply_nav_action("] {
        let body = fn_body(&s, name)
            .unwrap_or_else(|| panic!("{name} must exist; add it here if it is new"));
        shared.push((name, body));
    }
    Regions {
        run_tui,
        run_chat,
        shared,
    }
}

/// A helper only counts as shared if both loops actually call it. Otherwise
/// naming it in the list above would exempt an action nothing reaches.
///
/// Called from **both** tests that depend on it. With the check only in
/// `fleet_and_session_go_through_one_helper`, removing `apply_nav_action`'s
/// call site from `run_chat` left
/// `every_bound_action_is_handled_in_both_event_loops` **green** — the very
/// divergence that test exists to catch, reported as fine because a helper
/// nothing calls still "contains" the action.
fn shared_really_is_shared(r: &Regions) {
    for (name, _) in &r.shared {
        let stem = name.trim_start_matches("fn ").trim_end_matches('(');
        // Count **calls**, not mentions. `apply_nav_action` is defined after
        // `run_chat`, so the chat region contains its definition and a bare
        // count of the stem found two "calls" there with the call site deleted —
        // and the divergence went unreported. A call is the name followed by
        // `(` and *not* preceded by `fn `.
        let calls = |body: &str| {
            body.match_indices(&format!("{stem}("))
                .filter(|(i, _)| !body[..*i].trim_end().ends_with("fn"))
                .count()
        };
        let sites = calls(&r.run_tui) + calls(&r.run_chat);
        assert!(
            sites >= 2,
            "{name} is treated as shared but is called {sites} time(s); a \
             helper only one loop calls is a divergence, not a shared handler. \
             The enumeration below would otherwise report every action it \
             holds as handled in both loops, which is the bug this file is \
             for."
        );
    }
}

#[test]
fn the_table_is_not_empty() {
    let bound = BINDING_TABLE
        .iter()
        .filter(|d: &&BindingDef| d.action.is_some())
        .count();
    assert!(
        bound >= 8,
        "the binding table shrank to {bound} bound rows; this file enumerates \
         it, so a smaller table means a smaller product"
    );
}

/// **The exit criterion.** Every bound action must be handled in *both* event
/// loops. This is the assertion that found `GotoFleet` and `GotoSession`
/// missing from `run_chat`.
#[test]
fn every_bound_action_is_handled_in_both_event_loops() {
    let r = regions();
    // Before the enumeration, not after: a helper that is not really shared
    // makes every row below meaningless.
    shared_really_is_shared(&r);
    let mut missing = Vec::new();
    for def in BINDING_TABLE.iter().filter(|d| d.action.is_some()) {
        let action = def.action.expect("filtered");
        let needle = format!("GlobalAction::{action:?}");
        let shared_handles = r.shared.iter().any(|(_, b)| b.contains(&needle));
        let in_tui = r.run_tui.contains(&needle);
        let in_chat = r.run_chat.contains(&needle);
        // Reachable from a loop if the loop handles it or handles it through
        // shared code both loops call.
        if !(in_tui || shared_handles) {
            missing.push(format!("{} ({action:?}) is dead in `niki run`", def.id));
        }
        if !(in_chat || shared_handles) {
            missing.push(format!("{} ({action:?}) is dead in `niki chat`", def.id));
        }
    }
    assert!(
        missing.is_empty(),
        "these bound actions are not reachable from both event loops: \
         {missing:?}. `niki run` and `niki chat` are the same product; a key \
         that works in one and not the other is a key the user cannot rely on."
    );
}

/// And the two navigation actions go through the one shared helper, so a
/// future divergence is a one-line difference rather than a silent split.
#[test]
fn fleet_and_session_go_through_one_helper() {
    let s = tui();
    let r = regions();
    shared_really_is_shared(&r);
    let calls = s.matches("apply_nav_action(").count();
    assert!(
        calls >= 3,
        "expected the definition plus one call per event loop; found {calls}"
    );
    // And no loop may set those pages directly any more.
    for direct in [
        "state.current_page = PageId::Fleet",
        "state.current_page = PageId::Session",
    ] {
        let outside = s
            .lines()
            .filter(|l| l.contains(direct) && !l.contains("fn apply_nav_action"))
            .count();
        assert!(
            outside <= 1,
            "`{direct}` appears {outside} times outside the helper; both loops \
             must go through `apply_nav_action` or they will drift again"
        );
    }
}

/// `g` must not be a dead key in `niki chat`, and `s` must not land on a
/// screen that renders nothing.
#[test]
fn fleet_and_session_are_not_reachable_only_by_bare_assignment() {
    let s = tui();
    let jump = s
        .split("fn global_page_jump(")
        .nth(1)
        .expect("global_page_jump exists");
    // One `matches!` names both, so asserting each name separately looks for a
    // shape the code does not have.
    assert!(
        jump.contains("!matches!(p, PageId::Fleet | PageId::Session)"),
        "`global_page_jump` must refuse Fleet and Session: it sets \
         `current_page` without the state those renderers need, which is the \
         blank screen. {jump}"
    );
}

/// And `s` opens a session when there is none, which is what makes the page
/// have something to draw.
#[test]
fn s_opens_a_session_rather_than_navigating_to_nothing() {
    let s = tui();
    let helper = s
        .split("fn apply_nav_action(")
        .nth(1)
        .expect("the helper exists");
    assert!(
        helper.contains("open_selected_mission()"),
        "`s` must open the selected mission when nothing is open, or the \
         Session page renders nothing: {helper}"
    );
}

/// The palette advertises `g` and `s`, so they had better work.
#[test]
fn the_advertised_navigation_keys_are_bound() {
    let (bindings, _) =
        niki::display::keybindings::KeyBindings::with_overrides(&std::collections::HashMap::new());
    for (key, action) in [('g', "GotoFleet"), ('s', "GotoSession")] {
        let event = KeyEvent::new(KeyCode::Char(key), KeyModifiers::empty());
        let resolved = bindings.resolve(&event);
        assert!(
            resolved.is_some(),
            "`{key}` is advertised by the command palette and resolves to \
             nothing, so it cannot be the {action} key it claims to be"
        );
        let _ = action;
    }
    let _ = AppState::new("t".into(), NikiConfig::default(), "/tmp".into());
}
