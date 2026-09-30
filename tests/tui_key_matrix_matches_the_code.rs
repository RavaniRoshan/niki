//! `docs/tui/key-matrix.md` must not describe a codebase that no longer exists.
//!
//! Its "Known run_tui vs run_chat divergences" section listed four
//! divergences under the heading **"do not 'fix' without unification"**. All
//! four had already been fixed:
//!
//! 1. *Theme: Ctrl+T vs bare `t`* — there is no bare `t` handler anywhere. The
//!    stale letter had also survived in the command palette's `theme: cycle`
//!    row, which advertised a key that did nothing.
//! 2. *Quit: `q` confirm vs `q` quit* — `q` goes back on a sub-page in both
//!    loops, and a page that declines it falls back to the confirm modal in
//!    both.
//! 3. *Tab: Chat/Run vs Chat/last page* — both resolve `Chat → Run`.
//! 4. *Ctrl+C* — both loops route it through the same handler.
//!
//! So the document was telling a maintainer not to fix work that was done, and
//! listing three divergences that did not exist. A reference that misdirects
//! the next person is worse than no reference, and this one is linked from the
//! TUI's own docs.
//!
//! The section is now **empty**, and these tests are what keeps it that way.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

fn src(rel: &str) -> String {
    read(&format!("src/{rel}"))
}

/// The divergence section, which must be empty.
fn divergences() -> String {
    let doc = read("docs/tui/key-matrix.md");
    let start = doc
        .find("## Known run_tui vs run_chat divergences")
        .expect("the section header must exist");
    let rest = &doc[start..];
    let end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// No unresolved divergence may be listed. A `1.`-style entry that is not
/// struck through is a claim about the code, and the code is the thing this
/// file checks.
#[test]
fn no_unresolved_divergence_is_listed() {
    let section = divergences();
    assert!(
        section.contains("**Empty.**"),
        "the divergence section must say it is empty, so a reader knows the \
         list was checked rather than forgotten: {section}"
    );
    let live: Vec<&str> = section
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with(|c: char| c.is_ascii_digit()) && t.contains(". ") && !t.contains("~~")
        })
        .collect();
    assert!(
        live.is_empty(),
        "these divergences are listed as unresolved: {live:?}. If one is real, \
         it needs the evidence next to it; if it is not, delete the line."
    );
}

/// Divergence 1, restated against the code: no bare `t` reaches the theme.
#[test]
fn there_is_no_bare_t_handler() {
    for rel in [
        "display/tui.rs",
        "display/pages/run.rs",
        "display/pages/chat.rs",
    ] {
        let body = src(rel);
        for (i, line) in body.lines().enumerate() {
            if !line.contains("Char('t')") {
                continue;
            }
            assert!(
                line.contains("CONTROL") || line.contains("ctrl"),
                "{rel}:{} has a bare `t` arm: {line}",
                i + 1
            );
        }
    }
    // And the palette must not advertise the letter either.
    let palette = src("display/command_palette.rs");
    assert!(
        !palette.contains("shortcut: \"t\","),
        "the command palette still advertises a bare `t` for the theme"
    );
}

/// Divergence 2: no loop quits the app straight from a key a page owns.
#[test]
fn no_loop_quits_from_a_key_the_page_owns() {
    let tui = src("display/tui.rs");
    assert!(
        !tui.contains("NavIntent::Quit => break,"),
        "a `NavIntent::Quit` that breaks the event loop is how 11 pages' `q` \
         handlers became dead code"
    );
    // Both loops must defer to the page, or one of them still does.
    let guards = tui.matches("!sub_page_owns(key, &state)").count();
    assert!(
        guards >= 4,
        "expected both loops to gate the nav block and the chat toggle on \
         `sub_page_owns`; found {guards} guards"
    );
}

/// Divergence 3: both loops send `Tab` from Chat to the same page.
#[test]
fn both_loops_send_tab_to_run() {
    let tui = src("display/tui.rs");
    assert!(
        tui.contains("PageId::Chat => PageId::Run"),
        "one of the two loops no longer sends Chat to Run; the divergence is \
         back and the document does not say so"
    );
}

/// Divergence 4: the shared handler the code already has.
#[test]
fn ctrl_c_goes_through_one_handler() {
    let tui = src("display/tui.rs");
    assert!(
        tui.contains("route_ctrl_c"),
        "Ctrl+C is handled in two places again; route it through the shared \\
         handler and note the divergence if it cannot be"
    );
}

/// The other half of the document: the keys it *does* list must still exist.
/// A key matrix that drops a binding without saying so is the same failure in
/// the other direction.
#[test]
fn the_matrix_still_documents_the_chord_keys_it_claims() {
    let doc = read("docs/tui/key-matrix.md");
    let bindings = src("display/keybindings.rs");
    for chord in ["ctrl+e", "ctrl+c", "ctrl+p", "ctrl+t"] {
        assert!(
            doc.to_lowercase().contains(chord),
            "the matrix no longer documents {chord}"
        );
        assert!(
            bindings.contains(chord),
            "{chord} is documented but not in the binding table"
        );
    }
}
