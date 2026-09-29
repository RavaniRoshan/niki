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

use std::path::{Path, PathBuf};

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

/// Every `niki <word>` in a user-facing string must be a command that exists.
///
/// This file exists because two prompts shipped claims that were provably
/// false, and the general shape of both was the same: the program telling the
/// user to do something it cannot do. The first two here were narrower
/// instances of it, caught by reading the code.
///
/// * `src/mcp/mod.rs:342` told a user to run `niki mcp trust <server>`. There
///   is no `mcp` subcommand. The user copies the command, gets a usage error,
///   and learns that the tool does not know what it is talking about.
/// * The `niki chat` config error points at `niki config check`, which had to
///   be added in the same change — a remedy added to a message before the
///   command exists is a remedy that does not work.
///
/// Backticks are the signal. A user-facing string in this codebase writes a
/// command in backticks precisely so it can be copied, which is exactly the
/// set of strings where a wrong name does the most damage.
#[test]
fn every_backticked_niki_command_in_a_user_facing_string_exists() {
    let root = repo_root();
    let mut known: Vec<String> = Vec::new();
    let main_rs = std::fs::read_to_string(root.join("src/main.rs")).expect("main.rs");
    let body = &main_rs[main_rs.find("enum Commands {").expect("Commands")..];
    let body = &body[..body.find("\n#[tokio::main]").unwrap_or(body.len())];
    for variant in re_derive_variants(body) {
        known.push(to_snake(&variant));
    }
    let _ = &root;

    let mut checked = 0usize;
    for (path, text) in rust_sources(&root) {
        for reference in backticked_niki_commands(&text) {
            // Only the *first* word after `niki` is checked, and that is the
            // honest scope of what this can assert.
            //
            // The second word is a flag (`--category`, `--tui`), the value of
            // one (`json`, `true`), the start of a quoted task
            // (`niki run "Actualizar la documentacion"`), or a real
            // sub-subcommand (`niki config check`). Those cannot be told
            // apart from the text alone, and guessing produced a test that
            // failed on fifteen of them — which would have taught a reader
            // nothing.
            //
            // What it *can* assert is the thing that actually went wrong: the
            // command a user is told to run does not exist. A `mcp trust`
            // subcommand is the case in point, and the only one this
            // repository has had.
            let Some(sub) = reference.split_whitespace().nth(1) else {
                continue;
            };
            let sub = sub.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            if sub.is_empty() {
                continue;
            }
            if !known.iter().any(|k| k == sub) {
                panic!(
                    "{path} tells the user to run `niki {sub}`, which is not a \
                     command. Known: {}",
                    known.join(", ")
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "the scan found nothing, so it is not scanning the strings it claims to"
    );
}

/// `Variant,` / `Variant {` / `Variant(` at the start of a line in the enum.
fn re_derive_variants(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        let Some(first) = t.chars().next() else {
            continue;
        };
        if !first.is_uppercase() {
            continue;
        }
        let name: String = t
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if name.len() > 1 {
            out.push(name);
        }
    }
    out
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

fn rust_sources(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root.join("src")) else {
        return out;
    };
    for e in rd.flatten() {
        collect_rs(&e.path(), &mut out);
    }
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(t) = std::fs::read_to_string(&p)
        {
            out.push((p.display().to_string(), t));
        }
    }
}

/// Every `` `niki <word> `` reference in the text.
///
/// Bounded by the **end of the line**, not the next backtick in the file.
/// Pairing backticks across the whole document does not work: the sources are
/// full of them in doc comments, so a span opened in a string literal closes
/// somewhere in an unrelated comment and the scan silently finds nothing. That
/// is not hypothetical — the first version of this function did exactly that,
/// and the test's own "found nothing" guard is the only reason it was caught
/// rather than passing vacuously.
///
/// A reference is always on one line, so the line is the correct span.
fn backticked_niki_commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut from = 0usize;
        while let Some(i) = line[from..].find("`niki ") {
            let start = from + i;
            let after = &line[start + 1..];
            let end = after.find('`').unwrap_or(after.len());
            let span = &after[..end];
            if span.starts_with("niki ") && span.len() > "niki ".len() {
                out.push(span.to_string());
            }
            from = start + 1;
        }
    }
    out
}
