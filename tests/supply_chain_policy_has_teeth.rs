//! The supply-chain gate must have teeth, and its exceptions must be reasoned.
//!
//! `deny.toml` carried:
//!
//! ```toml
//! # How aggressively to treat unsoundness advisories.
//! unsound = "none"
//! ```
//!
//! which reads as a policy and is the opposite of one. `none` means *no*
//! unsound advisory can ever fail this gate, so the four below were tolerated
//! and so would any **new** one be — the same green either way. There was
//! nothing to review and nothing to fail.
//!
//! `unsound = "all"` plus a named, reasoned `[[advisories.ignore]]` entry for
//! each is the same four tolerated advisories, with the difference that the
//! fifth stops CI.
//!
//! ## Which four, and why each is tolerated
//!
//! | Advisory | Crate | Why it is not reachable from here |
//! |---|---|---|
//! | RUSTSEC-2026-0183 | git2 0.20.4 | UB in `Remote::list()`. NIKI never constructs a `Remote` — the whole git2 surface is local open/read/commit/diff. |
//! | RUSTSEC-2026-0184 | git2 0.20.4 | UB for a `Signature` derived from a buffer-created `BlameHunk`. NIKI calls `Signature::now` and never reads blame. |
//! | RUSTSEC-2026-0002 | lru 0.12.5 | Transitive: ratatui 0.29 → lru. `IterMut` violating Stacked Borrows, needing a concurrent mutation pattern NIKI does not write. |
//! | RUSTSEC-2026-0253 | lru 0.12.5 | Transitive, as above. Use-after-free from missing panic safety in `pop()`. |
//!
//! The roadmap said **three**; `cargo audit` reports **four**, across two
//! crates. The count in the document was wrong and this test is derived from
//! the tool's own output rather than from the document.

use std::path::Path;
use std::process::Command;

fn deny_toml() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("deny.toml"))
        .expect("deny.toml must be readable")
}

/// The policy must actually deny unsoundness.
#[test]
fn unsound_advisories_are_denied_not_ignored() {
    let toml = deny_toml();
    assert!(
        toml.contains(r#"unsound = "all""#),
        "`unsound` must scope to `all`. With `\"none\"`, no unsound advisory \
         can fail this gate — a newly published one is exactly as green as the \
         four we have reasoned about below, which is what made the setting \
         worthless rather than permissive."
    );
    assert!(
        !toml.contains(r#"unsound = "none""#),
        "`unsound = \"none\"` means unsoundness is never a failure"
    );
}

/// And the exception list must match what the tool actually reports — derived
/// from `cargo audit`, not from a hand-typed list that drifts.
#[test]
fn every_unsound_advisory_is_named_with_a_reason() {
    // What the tool reports.
    let out = Command::new("cargo").args(["audit", "--json"]).output();
    let Ok(out) = out else {
        panic!("cargo audit must run for this test to mean anything");
    };
    let parsed: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("audit JSON: {e}"));
    let unsound = parsed
        .get("warnings")
        .and_then(|w| w.get("unsound"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // The ids come from the **JSON**, not from the tool's human-readable output.
    //
    // It used to scrape `Warning:   unsound` / `ID:` out of the text, on the
    // stated grounds that "advisory ids are not in the JSON's warning entries".
    // They are — under `warnings[].advisory.id` — and the text format is not a
    // contract: a newer cargo-audit changed the spacing, the scrape returned
    // nothing, and the test failed in CI with *"the JSON's unsound count and
    // the text output's unsound IDs disagree: 4 vs []"*. The same numbers, read
    // through a format nobody promised to keep.
    let mut reported: Vec<String> = unsound
        .iter()
        .filter_map(|w| w.get("advisory").and_then(|a| a.get("id")))
        .filter_map(|id| id.as_str())
        .map(str::to_string)
        .collect();
    reported.sort();
    reported.dedup();

    let toml = deny_toml();
    let mut ignored: Vec<String> = Vec::new();
    for line in toml.lines() {
        if let Some(id) = line.trim().strip_prefix("id = \"") {
            ignored.push(id.trim_end_matches('"').to_string());
        }
    }
    ignored.sort();
    reported.sort();

    assert_eq!(
        unsound.len(),
        reported.len(),
        "the JSON's unsound count and the text output's unsound IDs disagree: \
         {} vs {reported:?}",
        unsound.len()
    );
    assert!(
        !reported.is_empty(),
        "cargo audit reported no unsound warnings; either the advisory \
         database changed or the parse broke, and this test should be revisited"
    );
    assert_eq!(
        ignored, reported,
        "the ignore list must name exactly the advisories cargo audit reports. \\
         Reported: {reported:?}\\nIgnored: {ignored:?}\\nAn advisory that is \\
         reported and not ignored fails the gate; one that is ignored and no \\
         longer reported is a stale exception nobody removed."
    );
}

/// Every exception must carry a real reason, and name what would change it.
#[test]
fn every_exception_explains_itself() {
    let toml = deny_toml();
    let blocks: Vec<&str> = toml.split("[[advisories.ignore]]").skip(1).collect();
    assert_eq!(blocks.len(), 4, "the exception count moved; re-read them");
    for b in &blocks {
        let id = b
            .lines()
            .find_map(|l| l.trim().strip_prefix("id = \""))
            .unwrap_or("<no id>")
            .trim_end_matches('"');
        let reason = b.split("reason = \"\"\"").nth(1).unwrap_or("");
        assert!(
            reason.trim().len() > 80,
            "{id} has no real reason. A bare `ignore` entry is a decision \\
             nobody can review, which is the thing this gate is for."
        );
        // A reason must say *why it does not bite here*, not restate the
        // advisory title.
        assert!(
            reason.contains("NIKI") || reason.contains("ratatui") || reason.contains("Transitive"),
            "{id}'s reason does not say why it is safe *here* — it appears to \\
             restate the advisory rather than justify tolerating it: {reason}"
        );
    }
}

/// The two git2 advisories are tolerated on **reachability**, and reachability
/// is a property of the tree that can change. If NIKI ever starts using
/// `Remote` or blame, the justification stops being true and this must notice.
#[test]
fn the_git2_exceptions_are_still_unreachable() {
    let mut offenders: Vec<String> = Vec::new();
    for p in ["src/output/git.rs", "src/sandbox/worktree.rs", "src/goal"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(p);
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let hits: Vec<&str> = body
            .lines()
            .filter(|l| {
                !l.trim_start().starts_with("//")
                    && (l.contains("Remote::") || l.contains("blame(") || l.contains("BlameHunk"))
            })
            .collect();
        if !hits.is_empty() {
            offenders.push(format!("{p}: {hits:?}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "NIKI now uses the git2 APIs RUSTSEC-2026-0183 and -0184 describe: \\
         {offenders:?}. The ignore reasons in deny.toml are no longer true, and \\
         the fix is the git2 0.21 bump, not a justification."
    );
}
