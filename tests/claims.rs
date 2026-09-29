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
        let snake = to_snake(&variant);
        known.push(snake.clone());
        // Subcommands: `niki config init`, `niki providers models`, ...
        for sub in subcommands_of(&root, &snake) {
            known.push(format!("{snake} {sub}"));
        }
    }

    let mut checked = 0usize;
    for (path, text) in rust_sources(&root) {
        for reference in backticked_niki_commands(&text) {
            let word = reference.split_whitespace().next().unwrap_or("");
            if word.ends_with(".toml") || word == "niki" {
                continue;
            }
            let invocation: Vec<&str> = reference
                .split_whitespace()
                .take(2)
                .map(|s| s.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.'))
                .collect();
            let invocation = invocation.join(" ");
            if !known.iter().any(|k| k == &invocation) {
                panic!(
                    "{path} tells the user to run `{invocation}`, which is not a \\
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

/// The `Subcommand` enum variants under `src/cli/<name>.rs`.
fn subcommands_of(root: &Path, snake: &str) -> Vec<String> {
    let path = root.join(format!("src/cli/{snake}.rs"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Some(start) = text.find("enum ") else {
        return Vec::new();
    };
    let body = &text[start..];
    let body = &body[..body.find("\n}\n").unwrap_or(body.len())];
    re_derive_variants(body)
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

/// `` `niki foo bar` `` — a backticked span that starts with the binary name.
fn backticked_niki_commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('`') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('`') else { break };
        let span = &after[..j];
        if span.starts_with("niki ") {
            out.push(span.to_string());
        }
        rest = &after[j + 1..];
    }
    out
}
