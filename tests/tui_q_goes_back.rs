//! The key that goes *back* must not quit the app.
//!
//! Every sub-page answers `q` with "back to Run" — `pages/diff.rs:189`,
//! `pages/history.rs:281`, and nine more. Not one of them could run: the nav
//! layer sat *above* the page router, read `q` as `NavIntent::Quit`, and broke
//! the event loop. So the key that goes back everywhere else in the app
//! destroyed the interface on every page that had a handler for it.
//!
//! It also made the confirm-quit modal written below the nav layer
//! unreachable dead code — the loop had already exited before reaching it —
//! which is why a `grep` for the modal found it and a user could never see it.
//!
//! The behavioural proof is `tests/tui_smoke/cases/13_subpage_q_goes_back.sh`,
//! which drives a real pty and fails with "the session is gone" when `q` quits.
//! A source check cannot be the only guard: the page handlers here were
//! present and correct the whole time, and simply never ran — so what needs
//! pinning is the *ordering*, and that is what these check.

use std::path::Path;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// Both event loops must let the page have `q` before the nav layer sees it.
///
/// **In both loops**, and the assertion used to be two different literals.
///
/// `run_chat` was pinned to `key.code != KeyCode::Char('q')` and `run_tui` to a
/// call-ordering between two branches. Both loops now gate their nav block the
/// same way — on `sub_page_owns(key, &state)`, the *page's* answer to "is this
/// key mine" — so both assertions went red while the code got better. That is
/// the same failure as `state_layout`'s patch-temporary test and `mcp_call_path`
/// before B11-01: **a source-text assertion pinned to a literal from an earlier
/// design.** Putting the old literals back would forbid the improvement.
///
/// The property, once, for both loops: **the nav block is entered only when the
/// page declines the key.** A page that answers `q` with "go back" keeps it; a
/// page that declines it falls through to the confirm modal, which is the
/// behaviour the old letter-gate could not express.
#[test]
fn a_subpage_q_reaches_the_page_not_the_nav_layer() {
    let s = src("src/display/tui.rs");

    let run_tui_start = s.find("fn run_tui(").expect("run_tui exists");
    let run_chat_start = s.find("pub fn run_chat(").expect("run_chat exists");
    assert!(
        run_tui_start < run_chat_start,
        "the two loops must stay separable, or this test compares a line in one \
         against a line in the other and reports an ordering that does not exist"
    );
    let loops = [
        ("run_tui", &s[run_tui_start..run_chat_start]),
        ("run_chat", &s[run_chat_start..]),
    ];

    for (name, body) in loops {
        // **Every** nav block, not the first.
        //
        // There are two gates in each loop — one for a global keybinding, one
        // for the page nav — and the first version of this checked only the
        // first `intent_from_key`. Removing the gate from the *second* left the
        // test green, which is the whole reason this now iterates.
        let mut checked = 0;
        let mut at = 0;
        while let Some(offset) = body[at..].find("crate::display::nav::intent_from_key(") {
            let nav = at + offset;
            let guard = &body[nav.saturating_sub(260)..nav];
            assert!(
                guard.contains("!sub_page_owns(key, &state)"),
                "{name}'s nav layer at offset {nav} must be gated on whether the \
                 *page* claims the key. Read `q` as a quit before the router and \
                 every sub-page's own handler is unreachable. The guard reads:\n{guard}"
            );
            assert!(
                !guard.contains("key.code != KeyCode::Char('q')"),
                "{name} has the letter-based gate back alongside the ownership \
                 one: a page that answers `q` its own way would still be \
                 overridden. The guard reads:\n{guard}"
            );
            checked += 1;
            at = nav + 1;
        }
        assert!(
            checked >= 1,
            "{name} has no nav block, so this test is asserting nothing about it"
        );
    }

    // …and the same refusal sits in front of the **global keybinding** path,
    // which is a separate statement from the nav block and was therefore not
    // covered by the loop above. Removing that gate left the test green, which
    // is how this line came to exist: the page must get first refusal at every
    // place that can bypass it, and there is more than one such place.
    for (name, body) in loops {
        let gates = body.matches("!sub_page_owns(key, &state)").count();
        assert!(
            gates >= 2,
            "{name} has {gates} page-refusal gate(s); it needs at least the nav \
             block's and the global keybinding's. A page that answers `q` with \
             its own handler must keep it on both paths"
        );
    }

    // And the page router still runs in `run_tui`, for the sub-pages whose own
    // handlers decline everything.
    let first_loop = &s[run_tui_start..run_chat_start];
    assert!(
        first_loop.contains("} else if router.handle_key(key, &mut state) {"),
        "run_tui must route the remaining sub-pages through the page router"
    );
}

/// And the quit modal must be reachable, not just present.
#[test]
fn the_confirm_quit_modal_is_reachable_from_a_subpage() {
    let s = src("src/display/tui.rs");
    assert!(
        !s.contains("NavIntent::Quit => break,"),
        "`NavIntent::Quit` breaking the loop is the defect: it is what made \
         every sub-page's `q` handler dead and the modal below it unreachable"
    );
    // Fleet and Session answer `q` for themselves and `continue` before the
    // router, so without their own arm the key is swallowed there.
    let fleet_arm = s
        .find("} else if key.code == KeyCode::Char('q') {")
        .expect("a sub-page must fall back to the confirm modal for `q`");
    assert!(
        fleet_arm > 0,
        "a page that declines `q` must ask, not silently do nothing"
    );
}

/// The pty case is the behavioural proof and must exist to be run by G4.
#[test]
fn the_pty_case_exists_and_waits_for_the_session_to_die() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/tui_smoke/cases/13_subpage_q_goes_back.sh");
    let body =
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("the pty case must exist: {e}"));
    // The liveness check is the load-bearing part. Asserting only that "Run"
    // renders would pass against a session that was about to exit.
    assert!(
        body.contains("has-session"),
        "the case must prove the app is still running, not just that a frame \
         was drawn"
    );
    assert!(
        body.contains("Exit NIKI"),
        "and that `q` did not quietly open the quit modal instead"
    );
}
