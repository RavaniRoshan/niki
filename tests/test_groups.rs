//! One list of which test binaries may not run concurrently, and the gate that
//! keeps it from rotting.
//!
//! `.config/nextest.toml` shipped `[test-groups.heavy]` and `[test-groups.heap]`
//! with `max-threads = 1`, and a comment explaining that heavy binaries "get
//! max-threads = 1". No test anywhere carried a `#[test-group]` attribute and
//! the file had no `[[overrides]]` block at all, so both groups were empty: every
//! `cargo nextest run` in CI executed the fourteen heaviest binaries fully in
//! parallel, which is the exact configuration that OOMs a 7.5 GiB host. The
//! comment was true about the mechanism and false about the effect, and no test
//! could tell, because an empty group is valid TOML.
//!
//! `scripts/test-layer.sh` carried its own copy of the same fourteen names and
//! said the nextest groups "mirror the categories below". They did not.
//!
//! The fix is a single source of truth — `.config/test-binary-groups` — read by
//! `test-layer.sh` directly and compiled into `.config/nextest.toml` by
//! `scripts/gen-nextest-groups.py`. These tests are the regression gate: an
//! empty group, a stale `nextest.toml`, a listed binary that does not exist, a
//! binary in two groups, and a second hardcoded list in `test-layer.sh` all fail
//! here rather than in production.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const GROUPS_REL: &str = ".config/test-binary-groups";
const NEXTEST_REL: &str = ".config/nextest.toml";
const LAYER_REL: &str = "scripts/test-layer.sh";

/// The groups we know how to run. Order is the order they are rendered in.
const KNOWN_GROUPS: [&str; 2] = ["heavy", "heap"];

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Parse `.config/test-binary-groups` into `group -> [binary, ...]`.
///
/// Deliberately a hand-rolled scan rather than a TOML/INI dependency: this file
/// is three lines of grammar, and the project gates dependencies through
/// `cargo deny`, so a new crate here would have to earn its licence allowance.
fn parse_groups() -> BTreeMap<String, Vec<String>> {
    let text = std::fs::read_to_string(repo(GROUPS_REL))
        .expect(".config/test-binary-groups must exist — it is the single source of truth");

    let mut groups: BTreeMap<String, Vec<String>> =
        KNOWN_GROUPS.iter().map(|g| ((*g).to_string(), Vec::new())).collect();

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (group, binary) = match (parts.next(), parts.next(), parts.next()) {
            (Some(g), Some(b), None) => (g, b),
            _ => panic!("{}:{}: expected `<group> <binary>`, got {raw:?}", GROUPS_REL, lineno + 1),
        };
        let bucket = groups
            .get_mut(group)
            .unwrap_or_else(|| panic!("{}:{}: unknown group {group:?}", GROUPS_REL, lineno + 1));
        assert!(
            !bucket.contains(&binary.to_string()),
            "{}:{}: {binary:?} is listed twice in group {group:?}",
            GROUPS_REL,
            lineno + 1
        );
        bucket.push(binary.to_string());
    }
    groups
}

/// Every binary named in the shared list must be a real integration test.
///
/// A typo'd or renamed entry is the quiet failure: the generator would emit a
/// filter that matches nothing and the group would be empty again, which is the
/// original bug wearing a new hat.
#[test]
fn every_listed_binary_exists() {
    for (group, binaries) in parse_groups() {
        for binary in binaries {
            let path = repo("tests").join(format!("{binary}.rs"));
            assert!(
                path.exists(),
                ".config/test-binary-groups lists {binary:?} as {group:?} but {} does not exist",
                path.strip_prefix(repo("")).unwrap().display()
            );
        }
    }
}

/// No binary may be in two groups — the groups are serialisation constraints, and
/// a binary in both is either a contradiction or a silent duplicate.
#[test]
fn no_binary_is_in_two_groups() {
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for (group, binaries) in parse_groups() {
        for binary in binaries {
            if let Some(prev) = owner.insert(binary.clone(), group.clone()) {
                panic!("{binary:?} is in both {prev:?} and {group:?}");
            }
        }
    }
}

/// The regression itself: both groups must be non-empty.
///
/// `max-threads = 1` on an empty group is valid TOML that does nothing. This is
/// the assertion that would have caught the original bug, and it is one line.
#[test]
fn both_groups_are_non_empty() {
    for group in parse_groups() {
        assert!(
            !group.1.is_empty(),
            "group {:?} in {GROUPS_REL} is empty — an empty test-group serialises nothing, \
             so the heavy binaries run in parallel and the host OOMs",
            group.0
        );
    }
}

/// `.config/nextest.toml` must be the generated form of the shared list.
///
/// This is what stops the two from drifting again: change the list without
/// regenerating, or hand-edit the overrides, and the heavy binaries silently
/// stop being serialised.
#[test]
fn nextest_overrides_match_the_shared_list() {
    let groups = parse_groups();
    let nextest = std::fs::read_to_string(repo(NEXTEST_REL)).expect(".config/nextest.toml");

    assert!(
        nextest.contains("scripts/gen-nextest-groups.py"),
        "{NEXTEST_REL} no longer says how its overrides are produced — the generated block was \
         replaced by hand"
    );

    // Line-based rather than byte-offset-based. A `filter = '…'` is a single
    // TOML line, so scanning lines is both simpler and immune to the off-by-one
    // that a hand-rolled quote search walks into on the long generated filters.
    let mut filter_for: BTreeMap<String, String> = BTreeMap::new();
    let mut pending_filter: Option<String> = None;
    for line in nextest.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("filter = '") {
            pending_filter = rest.strip_suffix('\'').map(|s| s.to_string());
        } else if let Some(rest) = line.strip_prefix("test-group = '") {
            if let Some(group) = rest.strip_suffix('\'') {
                let f = pending_filter
                    .take()
                    .unwrap_or_else(|| panic!("{NEXTEST_REL}: {group:?} has a test-group with no filter"));
                filter_for.insert(group.to_string(), f);
            }
        }
    }

    for (group, binaries) in &groups {
        let filter = filter_for.get(group).unwrap_or_else(|| {
            panic!("{NEXTEST_REL} has no override assigning anything to {group:?}")
        });
        for binary in binaries {
            assert!(
                filter.contains(&format!("binary({binary})")),
                "{NEXTEST_REL} does not serialise {binary:?} via {group:?}.\n\
                 Run: python3 scripts/gen-nextest-groups.py"
            );
        }
    }
}

/// Every `test-group = '…'` target in `nextest.toml` must be a group that is
/// actually declared and actually populated — otherwise the override is inert.
#[test]
fn every_nextest_override_targets_a_populated_group() {
    let groups = parse_groups();
    let nextest = std::fs::read_to_string(repo(NEXTEST_REL)).expect(".config/nextest.toml");

    let declared: BTreeSet<&str> = nextest
        .lines()
        .filter_map(|l| l.trim().strip_prefix("[test-groups.").and_then(|r| r.strip_suffix("]")))
        .collect();

    for line in nextest.lines() {
        let Some(target) = line.trim().strip_prefix("test-group = '").and_then(|r| r.strip_suffix('\''))
        else {
            continue;
        };
        assert!(
            declared.contains(target),
            "{NEXTEST_REL} assigns tests to {target:?}, which is not declared as a [test-groups.{target}]"
        );
        let size = groups.get(target).map(|v| v.len()).unwrap_or(0);
        assert!(size > 0, "override targets {target:?}, whose group is empty");
    }
}

/// `test-layer.sh` must read the shared list rather than carry its own copy.
///
/// This is the half that is easy to reintroduce: someone adds a heavy binary to
/// the shell arrays "just in bash", and the two runners disagree about what is
/// heavy again.
#[test]
fn test_layer_reads_the_shared_list() {
    let script = std::fs::read_to_string(repo(LAYER_REL)).expect("scripts/test-layer.sh");

    assert!(
        script.contains("test-binary-groups"),
        "{LAYER_REL} no longer references .config/test-binary-groups — it is maintaining its own copy"
    );

    // The literal-array form is what the drift looked like. `HEAVY=(` and
    // `HEAP=(` are the exact shapes to forbid.
    for literal in ["HEAVY=(", "HEAP=("] {
        assert!(
            !script.contains(literal),
            "{LAYER_REL} contains a hardcoded `{literal}` array; the categories must come from \
             .config/test-binary-groups"
        );
    }
}
