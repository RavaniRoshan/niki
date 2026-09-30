//! A footer must not advertise a key that does nothing.
//!
//! Two footers did. Fleet's read `P Pause · R Resume · K Kill · V Diff` and
//! Session's read `P Pause · R Resume` — **six advertised controls, none of
//! them handled**. `handle_fleet_nav` answered `↑↓`, `Enter`, `s` and `Esc`;
//! `handle_session_nav` answered `Tab`, `←→` and `Esc`.
//!
//! They split three ways here, and the split is the point:
//!
//! - **`P` was wired.** `state.paused` is a real flag that the Run page's
//!   `Space` and the command palette's `pause / resume` both already flip, so
//!   the concept exists and the key was a hole in it.
//! - **`R` was retracted.** Pause and resume are *one* toggle. Two advertised
//!   keys for one action is how a footer rots into a lie, and aliasing `R` to
//!   the same flag would have preserved the lie in a new shape.
//! - **`K Kill` and `V Diff` were retracted.** There is no implementation of
//!   either anywhere in the tree, and wiring them to something approximate
//!   would be worse than their absence — a key that kills the wrong thing.
//!
//! The test walks the footers and the handlers, so a footer that grows a new
//! claim without a handler fails here rather than in a user's terminal.

/// Every single-letter key advertised in a footer, as (page, letter).
///
/// Parsed out of the footers rather than hand-listed, because a hand-list is
/// exactly what went stale: the footers said `P R K V` and nothing tracked
/// them.
fn advertised() -> Vec<(&'static str, char)> {
    let read = |rel: &str| {
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
    };
    let fleet = read("src/display/pages/fleet.rs");
    let session = read("src/display/pages/session.rs");

    let mut out = Vec::new();
    for (page, src) in [("fleet", &fleet), ("session", &session)] {
        for line in src
            .lines()
            .filter(|l| l.contains("Esc Back") || l.contains("Back to Fleet"))
        {
            // The footer's shape is `↑↓ Navigate · Enter Open · P Pause/Resume`,
            // so each `·`-separated segment is `KEY label…` and the key is its
            // first token. The first version of this parser collected every
            // ASCII letter in the line and reported `r`, `e`, `w` from
            // "Navigate" as unhandled keys.
            for segment in line.split('·') {
                let key = segment.split_whitespace().next().unwrap_or("");
                // Multi-key and word keys (`↑↓`, `Enter`, `Esc`) are checked
                // here only insofar as the single letters are; the rest are
                // covered by the page's own navigation tests.
                if key.chars().count() == 1 && key.chars().next().unwrap().is_ascii_alphabetic() {
                    let c = key.chars().next().unwrap();
                    if c.is_ascii_lowercase() {
                        // A lower-case first token is a label, not a key:
                        // footers capitalise the key and not the description.
                        continue;
                    }
                    out.push((page, c));
                }
            }
        }
    }
    out
}

/// The body of `handle_fleet_nav` / `handle_session_nav`.
///
/// Scoped because a whole-file search is not a collision test. Putting
/// `K Kill` back in the Fleet footer left this test green, because
/// `KeyCode::Char('k')` **is** in the file — it is `state.fleet.select_prev()`,
/// the `j`/`k` navigation. A key that does something other than what its
/// footer says is the same defect as a key that does nothing, and a
/// whole-file search cannot tell the two apart.
fn nav_body(name: &str) -> String {
    let tui = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");
    let start = tui
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("{name} must exist"));
    let rest = &tui[start..];
    let end = rest.find("\n}\n").map(|i| i + 2).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// Every advertised letter must be handled *by that page's own nav*, or by the
/// page's `handle_key` — never merely by something else in the file.
#[test]
fn an_advertised_key_is_either_handled_or_retracted() {
    let unhandled = advertised()
        .into_iter()
        .filter(|(page, c)| {
            let lower = c.to_ascii_lowercase();
            let arm = format!("KeyCode::Char('{lower}')");
            let in_nav = nav_body(&format!("handle_{page}_nav")).contains(&arm);
            let in_page = page_source(page).contains(&arm);
            !(in_nav || in_page)
        })
        .collect::<Vec<_>>();

    assert!(
        unhandled.is_empty(),
        "these footer keys are advertised and nothing handles them: {unhandled:?}. \
         Either wire them or take them out of the footer — a key that does \
         nothing is the same as a bug from where the user is standing."
    );
}

/// **What this file deliberately cannot check.** A footer's *label* has to
/// match the action its key performs — `K Kill` resolving to `select_prev` is a
/// lie even though the key is handled. That needs the label text and the
/// handler body compared, which is a judgement call, not a static check, and a
/// test that guessed at it would be worse than none: it would pass on the
/// cases it happened to model.
///
/// So the guard for that class is `the_retracted_keys_are_gone_from_the_footers`
/// below, which names the specific claims that were wrong. Adding a footer
/// claim means adding it there, and this file's comment is the reminder.
fn page_source(page: &str) -> String {
    let rel = format!("src/display/pages/{page}.rs");
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_default()
}

/// The specific regression, so the general test above has something to fail
/// against and the retraction is recorded rather than merely absent.
#[test]
fn the_retracted_keys_are_gone_from_the_footers() {
    let fleet = page_source("fleet");
    for gone in ["Kill", "R Resume", "V Diff"] {
        assert!(
            !fleet.contains(gone),
            "Fleet's footer still advertises `{gone}`, which has no \
             implementation anywhere in the tree"
        );
    }
    let session = page_source("session");
    assert!(
        !session.contains("R Resume"),
        "Session's footer still advertises `R Resume`; pause and resume are one \
         toggle, and two keys for one action is how a footer rots into a lie"
    );
    // And what replaced them must be true.
    assert!(
        fleet.contains("P Pause/Resume") && session.contains("P Pause/Resume"),
        "both footers should advertise the key that is actually wired, once \
         each — pause and resume are one toggle"
    );
}

/// `P` must really be handled on both pages, and the two handlers are private
/// to the `tui` module, so the assertion is on the file.
///
/// An earlier version of this file *copied* both handlers into the test and
/// called the copies — a test of the copy, which passes the moment the
/// production arm is deleted. The source assertion is weaker than a call and
/// it is the one that can actually fail.
#[test]
fn the_real_handlers_carry_the_pause_arm() {
    let tui = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/tui.rs"),
    )
    .expect("tui.rs must be readable");
    let pauses = tui.matches("KeyCode::Char('p') =>").count();
    assert_eq!(
        pauses, 2,
        "both `handle_fleet_nav` and `handle_session_nav` must handle `p`, and \
         each must flip the one `state.paused` flag the Run page's `Space` and \
         the command palette already flip; found {pauses} arms"
    );
    // The same flag, not a new one per surface: the Run page's `Space` lives
    // in `pages/run.rs`, so the count spans both files.
    let run = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/display/pages/run.rs"),
    )
    .expect("run.rs must be readable");
    let flips = tui.matches("state.paused = !state.paused;").count()
        + run.matches("state.paused = !state.paused;").count();
    assert_eq!(
        flips, 3,
        "the pause toggle must be the one `state.paused` flag everywhere — \
         Fleet `P`, Session `P`, Run `Space` — not a new flag per surface; \
         found {flips} flips"
    );
}
