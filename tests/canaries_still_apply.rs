//! Every canary patch must still match exactly one site in the tree.
//!
//! `scripts/canary-gate.sh` applies each canary's `patch` to its `file` and
//! **aborts** if the match count is anything but 1. That refusal is correct —
//! a patch that matches nothing would be scored as "survived", and a mutation
//! gate that stops killing anything is worse than no gate. But it is only
//! discovered when the nightly workflow runs.
//!
//! It had rotted. `ad1d4b9` moved the line `PL-1` targets out of
//! `src/cli/run.rs` into `src/orchestrator/deliver.rs`; the canary kept naming
//! the old file; and the next nightly run — the first since 2026-09-28, after
//! two cancellations — died on exit 5 before killing a single mutant.
//!
//! So the check moves into the normal test lane. A refactor that relocates a
//! canary target now fails `cargo test`, which runs on every push, instead of
//! waiting for a schedule.

use std::collections::BTreeMap;
use std::path::Path;

fn canaries_toml() -> toml::Value {
    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("mutants/canaries.toml"),
    )
    .expect("mutants/canaries.toml is readable");
    toml::from_str(&raw).expect("mutants/canaries.toml parses")
}

/// Every `patch` must match exactly once in its `file`.
///
/// Zero means the target moved and the canary is now inert — the gate will
/// abort rather than kill, which is safe but useless. More than one means the
/// patch is ambiguous and the gate aborts on that too.
#[test]
fn every_canary_patch_still_matches_exactly_one_site() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let list = canaries_toml();
    let canaries = list
        .get("canary")
        .and_then(|c| c.as_array())
        .expect("canaries.toml has a [[canary]] array");
    assert!(!canaries.is_empty(), "the canary list is empty");

    let mut broken: BTreeMap<&str, String> = BTreeMap::new();

    for c in canaries {
        let id = c["id"].as_str().expect("every canary has an id");
        let file = c["file"].as_str().expect("every canary has a file");
        let patch = c["patch"].as_str().expect("every canary has a patch");

        let source = match std::fs::read_to_string(root.join(file)) {
            Ok(s) => s,
            Err(e) => {
                broken.insert(id, format!("{file} is unreadable: {e}"));
                continue;
            }
        };
        let n = source.matches(patch).count();
        if n != 1 {
            broken.insert(
                id,
                format!(
                    "{file}: patch matches {n} site(s), needs exactly 1. \
                     `scripts/canary-gate.sh` aborts on anything else, so this \
                     canary is currently killing nothing."
                ),
            );
        }
    }

    assert!(
        broken.is_empty(),
        "canaries that no longer apply:\n{}",
        broken
            .iter()
            .map(|(id, why)| format!("  {id}: {why}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The guard above is only useful if it can fail. Point one canary at a file
/// that cannot contain it and confirm the logic reports it — otherwise this
/// file proves nothing about the real check.
#[test]
fn a_canary_whose_target_moved_is_detected() {
    let list = canaries_toml();
    let c = list["canary"]
        .as_array()
        .expect("canaries")
        .iter()
        .find(|c| c["id"].as_str() == Some("PL-1-branch-claimed-without-creation"))
        .expect("PL-1 exists");

    let patch = c["patch"].as_str().expect("patch");
    let declared = c["file"].as_str().expect("file");

    // The canary's own text, applied against a file that does not hold it.
    let elsewhere = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/orchestrator/pipeline.rs"),
    )
    .expect("pipeline.rs");
    assert_eq!(
        elsewhere.matches(patch).count(),
        0,
        "this fixture picked a file that happens to contain the patch, so the \
         negative case proves nothing"
    );
    assert_ne!(
        declared, "src/orchestrator/pipeline.rs",
        "the fixture file is now the canary's real target"
    );
}
