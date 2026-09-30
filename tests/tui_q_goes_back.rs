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
#[test]
fn a_subpage_q_reaches_the_page_not_the_nav_layer() {
    let s = src("src/display/tui.rs");

    // The second loop (`run_chat`) gated the nav block on `q`, so a sub-page
    // that *declines* `q` still gets the confirm modal, and one that answers it
    // keeps its handler.
    assert!(
        s.contains("key.code != KeyCode::Char('q')"),
        "the `niki chat` event loop is back to reading `q` as NavIntent::Quit \
         before the router, so every sub-page's own `q` handler is unreachable"
    );

    // The first loop (`run_tui`) routed the remaining sub-pages straight into
    // the nav block. `router.handle_key` has to come first there.
    let router_before_nav = s
        .find("} else if router.handle_key(key, &mut state) {")
        .expect("the remaining sub-pages branch calls the router");
    let nav_in_first_loop = s
        .find("} else if let Some(intent) = crate::display::nav::intent_from_key(")
        .expect("the nav block exists");
    assert!(
        router_before_nav < nav_in_first_loop,
        "in `run_tui` the nav layer must not sit above the page router: `q` on \
         a sub-page has to reach the page before the navigator can claim it"
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
