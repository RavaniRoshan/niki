//! User-facing claims must match what the code actually does.
//!
//! `src/display/tips.rs` and `prompts/base.md` shipped two claims that were
//! provably false:
//!
//!   * "your working tree is never modified" — `src/cli/run.rs:850` calls
//!     `apply_diff_to_working_tree` for every non-Docker backend.
//!   * "the pipeline auto-rolls back on failure, leaving your repo clean" — no
//!     rollback path exists, and `src/output/git.rs:194-262` moves the user's
//!     HEAD onto the `niki/<id>` branch.
//!
//! The `base.md` copy was the more damaging one: it told the *agents* their own
//! sandbox was safer than it is, which suppresses exactly the caution the
//! prompt is trying to instil.
//!
//! These assertions are deliberately narrow. Re-introducing either claim is
//! legitimate once `INV-HEAD-UNTOUCHED` and a real rollback path exist; the
//! fix then belongs in this file, not by deleting the test.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every user-facing text surface in the repo, paired with its contents.
fn claim_surfaces() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![
        ("src/display/tips.rs".into(), read("src/display/tips.rs")),
        ("prompts/base.md".into(), read("prompts/base.md")),
        ("README.md".into(), read("README.md")),
    ];
    for name in [
        "coder.md",
        "reviewer.md",
        "tester.md",
        "planner.md",
        "critic.md",
    ] {
        let rel = format!("prompts/{name}");
        if repo_root().join(&rel).exists() {
            let body = read(&rel);
            out.push((rel, body));
        }
    }
    out
}

/// Phrases that assert a guarantee the product does not currently make.
/// Lowercased before matching.
const FALSE_CLAIMS: &[(&str, &str)] = &[
    (
        "working tree is never modified",
        "src/cli/run.rs:850 applies the sandbox diff to the host working tree for every \
         non-Docker backend",
    ),
    (
        "never modifies your working tree",
        "same as above — the diff is applied to the host tree before the branch is created",
    ),
    ("never be modified", "same as above"),
    (
        "auto-rolls back",
        "no rollback path exists; a partially-applied tree is left as-is",
    ),
    (
        "auto rolls back",
        "no rollback path exists; a partially-applied tree is left as-is",
    ),
    (
        "automatically rolls back",
        "no rollback path exists; a partially-applied tree is left as-is",
    ),
    (
        "leaving your repo clean",
        "no rollback path exists; a partially-applied tree is left as-is",
    ),
    (
        "leaving your repository clean",
        "no rollback path exists; a partially-applied tree is left as-is",
    ),
];

#[test]
fn no_user_facing_surface_claims_a_guarantee_the_code_does_not_make() {
    let mut violations: Vec<String> = Vec::new();

    for (rel, body) in claim_surfaces() {
        let lower = body.to_lowercase();
        for (phrase, why) in FALSE_CLAIMS {
            if lower.contains(phrase) {
                violations.push(format!("  {rel}: \"{phrase}\" — {why}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "these surfaces assert guarantees the product does not make:\n{}\n\n\
         Either fix the code, or state what actually happens. Do not delete this test — \
         it is the regression guard for the claim that is most often quoted back at the \
         project.",
        violations.join("\n")
    );
}

#[test]
fn base_md_does_not_claim_the_tree_is_untouched() {
    let base = read("prompts/base.md").to_lowercase();
    assert!(
        !base.contains("never applied to the user's tree"),
        "prompts/base.md told agents their work was 'never applied to the user's tree \
         directly'. That is false, and telling an agent its blast radius is zero is how \
         it stops being careful."
    );
}

#[test]
fn the_replacement_tips_describe_real_behaviour() {
    let tips = read("src/display/tips.rs");
    // The corrected tip must tell the user the diff lands on their tree and that
    // a blocked run can still leave edits — silence would be as unhelpful as the
    // original lie.
    assert!(
        tips.contains("niki/<id> branch"),
        "the replacement safety tip must name what the run actually produces"
    );
    assert!(
        tips.contains("git status"),
        "the replacement safety tip must warn that a blocked run can still touch the tree"
    );
}
