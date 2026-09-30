//! The TUI must fail the way a program fails, and act on the terminal it is
//! actually given.
//!
//! Four defects here all had the same shape: the surface looked alive while
//! doing nothing, or did something other than what it displayed.
//!
//! * `run_chat` swallowed every draw error with `.ok()`, then set
//!   `needs_render = false` and kept polling. Once drawing failed the app kept
//!   consuming keystrokes against a frozen frame — no error, no exit, no
//!   notice. `run_tui` already broke on the same error; the chat surface, which
//!   is where every user lands, had the weaker loop.
//! * `let _ = handle.join()` discarded a panic in the TUI thread and `handle()`
//!   ended `Ok(())`. `RestoreGuard` correctly gave the user their terminal
//!   back, and a script recorded success for a run that produced nothing.
//! * `state.chat_width` was only written inside `ChatPage::render`, which
//!   `render()` intercepts before the router, so it stayed at 80 while the
//!   screen drew at the real width. Every mouse hit-test addressed the wrong
//!   line: in a wide terminal, clicking a message copied a different one and
//!   said "copied selection".
//! * `active_focus` knew about three overlays and four are painted. The
//!   keyboard ladder checked all of them; the mouse did not, so a click landed
//!   on the page behind a modal — and a click on the status bar during
//!   onboarding cycled the permission mode toward BYPASS.

/// The draw loop must not discard the error, and the chat must not exit 0.
///
/// Source-scanning, for the same reason the keyring test is: both properties
/// are about a call site rather than about a value, and this repo already
/// uses this technique for wiring it cannot otherwise observe
/// (`tests/ci_contracts.rs`).
#[test]
fn a_draw_failure_ends_the_chat_rather_than_freezing_it() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs is readable");

    let chat_loop = src
        .split("pub fn run_chat(")
        .nth(1)
        .expect("run_chat exists");
    assert!(
        !chat_loop.contains(
            ".draw(|f| render(f, &state, &router, &command_palette))\n                .ok()"
        ),
        "run_chat must not discard a draw error: a frozen frame with no exit and no \
         notice is indistinguishable from a working one. run_tui already breaks here."
    );
    assert!(
        chat_loop.contains("if let Err(e) = drew"),
        "run_chat must branch on the draw result and leave the loop on failure"
    );
}

#[test]
fn a_panic_in_the_chat_thread_is_not_a_successful_exit() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli/chat.rs"),
    )
    .expect("chat.rs is readable");
    // Line-anchored: this file's own comment quotes the old line, and a
    // substring check would read the explanation as the defect.
    assert!(
        !src.lines().any(|l| l.trim() == "let _ = handle.join();"),
        "discarding the JoinError means a panicked render loop exits 0. A script \
         wrapping `niki` records success for a run that produced nothing."
    );
    assert!(
        src.contains("handle.join().is_err()"),
        "the join result must be checked and turned into a non-zero exit"
    );
}

/// The recorded width must come from the renderer that actually runs.
#[test]
fn the_recorded_chat_width_comes_from_the_renderer_that_runs() {
    let layout = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/layout/mod.rs"),
    )
    .expect("layout/mod.rs is readable");
    assert!(
        layout.contains("state.chat_width.set(msg_area.width"),
        "layout::render_chat is what draws the chat; it must record the width it drew \
         at, or every mouse hit-test addresses an 80-column map while the screen is \
         wider."
    );
}

// The behavioural assertions below — focus, and the help overlay — live in
// `src/display/tui.rs`'s own test module, next to the existing
// `active_focus_priority` test. They need private functions, and putting them
// where the code is keeps the reason next to the thing.
