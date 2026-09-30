//! Every open claim in `ROADMAP.md` is pinned by something that can fail.
//!
//! Four of the six slices in batch 6 were record corrections, and all four
//! were the same failure: a commit closed a defect in the code and left the
//! bullet that named it open. Nothing failed. The next reader — me, six
//! slices later — picked up a bullet describing a fix that had shipped two
//! batches earlier and started re-measuring code that was already right.
//!
//! So the record gets teeth. Each remaining open claim that can be checked
//! mechanically names the test that checks it, and this file asserts the
//! pairing holds in **both** directions:
//!
//!   - the pinning test exists and is listed, so a claim cannot be left
//!     unpinned;
//!   - the pinning test still exists, so a registry cannot point at a test
//!     that was renamed into oblivion and leave a claim looking pinned.
//!
//! A pin that fires *because the work landed* is the mechanism working. The
//! `the_two_sandbox_files_still_have_no_unit_tests` pin is the live example:
//! it is green now, and the slices that add in-file unit tests to
//! `worktree.rs` and `docker.rs` will turn it red, which is the signal to
//! strike that §6 bullet in the same commit.
//!
//! **What this cannot do.** It only covers claims that reduce to a fact this
//! repository can check. A claim about a product decision, a paid account or
//! an unverifiable external service is not here, and cannot be, because a test
//! that cannot fail is not a pin. Those live in `BLOCKERS.md` or in §7/§8
//! with the reason stated, which is the honest place for them.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// Open claims that reduce to something checkable, and the test that checks
/// them. Adding a checkable claim to the roadmap means adding a row here.
const PINS: &[(&str, &str, &str)] = &[
    (
        "§9.1 · `niki resume` restores state, re-runs nothing, and says so",
        "resume_does_not_claim_it_continued",
        "tests/resume_tells_the_truth.rs",
    ),
    (
        "§9.2 · MCP has no agent→server call path",
        "the_missing_call_path_is_still_recorded_as_missing",
        "tests/mcp_does_not_leak_or_lie.rs",
    ),
    (
        "§6 · `sandbox/worktree.rs` and `sandbox/docker.rs` have no in-file unit tests",
        "the_two_sandbox_files_still_have_no_unit_tests",
        "tests/the_record_had_moved.rs",
    ),
    (
        "§2 · the pipeline's tool cards render; the chat sends no tools",
        "the_chat_still_sends_no_tools",
        "tests/tool_cards_are_live.rs",
    ),
    (
        "§2 · a TUI run can reach the user, for approval and for questions",
        "an_attached_interface_is_asked_and_sees_the_command",
        "src/runtime/tools.rs",
    ),
];

#[test]
fn every_pinned_claim_names_a_test_that_exists() {
    let mut missing = Vec::new();
    for (claim, test, file) in PINS {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
        let Ok(source) = std::fs::read_to_string(&path) else {
            missing.push(format!("{claim}: {file} does not exist"));
            continue;
        };
        let needle = format!("fn {test}(");
        if !source.contains(&needle) {
            missing.push(format!("{claim}: `{test}` is not defined in {file}"));
        }
    }
    assert!(
        missing.is_empty(),
        "a pin that points at nothing leaves the claim looking checked:\n  {}",
        missing.join("\n  ")
    );
}

/// Every pin must be a test G3 can prove can fail.
///
/// A pin that is not in the canary map is a comment with an `assert!` on it.
/// The map is what `scripts/verify.sh --only G3` walks, and what every slice in
/// this programme has had to add to; a claim checked only by a test nothing
/// exercises is a claim that will quietly stop being true.
///
/// **Why there is no "is the bullet still open?" check here.** There was one,
/// and it was wrong: it flagged §9.2, whose numbered item *is* struck (batch 5
/// closed the two defects under it) while the bullet's own closing sentence
/// keeps a claim open — the agent→server call path. A struck bullet can carry
/// a live sub-claim, so "struck" does not mean "this claim is closed", and a
/// test that says otherwise would push the next person to delete a pin that
/// was doing its job. The condition that *is* mechanical is the one above.
#[test]
fn every_pinned_test_is_in_the_canary_map() {
    let canary = read("scripts/canary-map.txt");
    let mut unpinned = Vec::new();
    for (claim, test, file) in PINS {
        // The map's format is `slug | test_name | what breaking it looks like`.
        let listed = canary
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .any(|l| l.split('|').nth(1).map(str::trim) == Some(*test));
        if !listed {
            unpinned.push(format!(
                "{claim}: `{test}` ({file}) is not in scripts/canary-map.txt, so \
                 G3 cannot prove it can fail"
            ));
        }
    }
    assert!(
        unpinned.is_empty(),
        "a pin outside the canary map is not yet a pin:\n  {}",
        unpinned.join("\n  ")
    );
}

/// The registry and the record must not have drifted apart in the other
/// direction: a bullet that names a checkable fact, and has no row here.
///
/// This is the part that catches the rot this programme keeps meeting. It
/// cannot read prose, so it looks for the shape the roadmap actually uses —
/// `§N.M` headings and `file:line` references — and asks that each be
/// accounted for. A new numbered item with a source reference and no row is
/// the signal.
#[test]
fn a_new_numbered_item_with_a_source_reference_is_accounted_for() {
    let roadmap = read("ROADMAP.md");
    let registered: Vec<&str> = PINS.iter().map(|(c, _, _)| *c).collect();

    // Numbered items, e.g. `9.1`, `4.5` — the ones a slice can close by
    // number. Prose bullets have no number to pin against.
    let mut unaccounted = Vec::new();
    for line in roadmap.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed
            .strip_prefix("~~")
            .or_else(|| trimmed.strip_prefix('|'))
        else {
            continue;
        };
        // `9.1` at the start of a struck bullet or a table cell.
        //
        // `trim_start` before the digits, and it matters: the first version
        // read the cell opener `| 9.9` as a line with no leading number, so a
        // brand-new unpinned row sailed straight past the check this file
        // exists for.
        let number: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if number.is_empty() || !number.contains('.') {
            continue;
        }
        let id = format!("§{number}");
        // Struck items are closed; §9.5, §9.6 and §9.7 are batch-5 closures
        // and §4.2b was superseded. A struck item needs no pin.
        if trimmed.starts_with("~~") || line.trim_start().starts_with("| ~~") {
            continue;
        }
        if roadmap.contains(&format!("~~{number}~~")) {
            continue;
        }
        if registered.iter().any(|c| c.starts_with(&id)) {
            continue;
        }
        // Only a row that names a source is making a checkable claim; the
        // rest are decisions and belong in §7/§8 or BLOCKERS.md.
        if line.contains(".rs:") || line.contains(".toml:") {
            unaccounted.push(format!("{id} — {}", &trimmed[..trimmed.len().min(100)]));
        }
    }
    assert!(
        unaccounted.is_empty(),
        "these numbered items name a source and so make a claim this \
         repository can check, but nothing pins them. Either add a row to \
         PINS in tests/record_claims_are_pinned.rs, or move the item to a \
         section that says why it cannot be checked:\n  {}",
        unaccounted.join("\n  ")
    );
}
