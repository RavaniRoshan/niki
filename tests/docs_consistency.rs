//! Documentation cannot state a version the repository does not have.
//!
//! The previous `docs/launch-audit.md` sat at the top of this repository for
//! six weeks and five releases, still headed `Version: 0.4.0`, listing module
//! directories that had been added since, and claiming three features were
//! "Not implemented" that had shipped in 0.5. `README.md` cited it as the
//! methodology behind the project's honesty.
//!
//! It rotted for one reason and one reason only: nothing read it. The claims
//! gate covered three filenames and this was not one of them. That gap is now
//! closed for *commands* and *links* by `tests/claims.rs`. This file closes the
//! other half — the version-shaped facts a document states about itself.
//!
//! These assertions are deliberately mechanical. A hand-maintained "current
//! state" document will drift; the only question is whether the drift is
//! noticed. Here it is noticed by the build.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The crate's own version, from the one place it is defined.
fn crate_version() -> String {
    let toml = read("Cargo.toml");
    for line in toml.lines() {
        if let Some(rest) = line.strip_prefix("version = \"") {
            return rest.trim_end_matches('"').to_string();
        }
    }
    panic!("no version in Cargo.toml");
}

fn crate_field(key: &str) -> String {
    let toml = read("Cargo.toml");
    let needle = format!("{key} = \"");
    for line in toml.lines() {
        if let Some(rest) = line.strip_prefix(&needle) {
            return rest.trim_end_matches('"').to_string();
        }
    }
    panic!("no {key} in Cargo.toml");
}

/// Every document that states a version of the product must state *this* one.
///
/// A document that pins a stale version is worse than one that pins none: it
/// reads as current, it is cited as evidence, and it is confidently wrong.
#[test]
fn no_document_states_a_version_the_crate_does_not_have() {
    let version = crate_version();
    let mut offenders: Vec<String> = Vec::new();

    for rel in ["README.md", "docs/launch-audit.md", "docs/claims-audit.md"] {
        let body = read(rel);
        for (i, line) in body.lines().enumerate() {
            // Only a *stated* version counts. Prose like "0.4.0 was the old
            // behaviour" is history and must stay readable.
            let stated = line
                .strip_prefix("> **Crate version:** ")
                .or_else(|| line.strip_prefix("> **Version:** "))
                // The header continues with other facts on the same line
                // ("0.9.0 · **MSRV:** 1.88 · **Edition:** 2024"), so the version
                // is the first whitespace-delimited token, not the rest.
                .map(|v| {
                    v.split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                        .to_string()
                });
            if let Some(v) = stated
                && v != version
            {
                offenders.push(format!(
                    "  {rel}:{} says version {v}, Cargo.toml says {version}",
                    i + 1
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these documents state a version the crate does not have:\n{}\n\n\
         A document pinned to a stale version is worse than one pinned to none: it \
         reads as current and it is cited as evidence.",
        offenders.join("\n")
    );
}

/// The audit's "current shape" table is a set of measurements. They are only
/// worth printing if they are true, so the ones that can be derived from the
/// tree are derived here rather than trusted.
#[test]
fn the_audits_headline_counts_match_the_tree() {
    let audit = read("docs/launch-audit.md");
    let mut wrong: Vec<String> = Vec::new();

    let count_rust_files = || {
        fn walk(dir: &Path, n: &mut usize) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    *n += 1;
                }
            }
        }
        let mut n = 0;
        walk(&repo_root().join("src"), &mut n);
        n
    };

    let modules = std::fs::read_dir(repo_root().join("src"))
        .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).count())
        .unwrap_or(0);

    let docs_pages = {
        fn walk(dir: &Path, n: &mut usize) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n);
                } else if p.extension().is_some_and(|x| x == "mdx") {
                    *n += 1;
                }
            }
        }
        let mut n = 0;
        walk(&repo_root().join("docs/content"), &mut n);
        n
    };

    // Both of these were re-derived here rather than typed, because the first
    // draft of this table got the tool count wrong by hand — it said 20 where
    // the registry registers 22. A count in a document about accuracy is the
    // last number anyone should type from memory.
    let registered_tools = || {
        let src = read("src/runtime/tools.rs");
        let start = src
            .find("pub fn build_baseline_registry()")
            .expect("build_baseline_registry");
        let body = &src[start..];
        let end = body.find("\n    reg\n}").unwrap_or(body.len());
        body[..end]
            .lines()
            .filter(|l| l.contains("reg.register("))
            .count()
    };

    let subcommand_count = || {
        let src = read("src/main.rs");
        let start = src.find("enum Commands {").expect("Commands");
        let body = &src[start..];
        let end = body.find("\n}\n").unwrap_or(body.len());
        body[..end]
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                let name: String = t
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if name.len() < 2 {
                    return None;
                }
                // A variant, not a doc comment and not a field. The character
                // after the identifier is what tells them apart: `Run(…)` and
                // `Config {` are variants, `/// Run a coding…` is not.
                let after = t[name.len()..].chars().next();
                match after {
                    Some('(') | Some('{') | Some(',') | None => Some(()),
                    _ => None,
                }
            })
            .count()
    };

    for (label, actual, needle) in [
        ("Rust source files", count_rust_files(), "Rust source files"),
        ("Module directories", modules, "Module directories"),
        ("Documentation pages", docs_pages, "Documentation pages"),
        (
            "Baseline tools",
            registered_tools(),
            "Baseline tools registered",
        ),
        ("CLI subcommands", subcommand_count(), "CLI subcommands"),
    ] {
        let claimed = audit
            .lines()
            .find(|l| l.contains(needle))
            .and_then(|l| {
                l.split('|')
                    .map(|c| c.trim())
                    .find(|c| c.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            })
            .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
            .unwrap_or_else(|| panic!("the audit has no '{needle}' row"));
        if claimed != actual.to_string() {
            wrong.push(format!(
                "  {label}: audit says {claimed}, the tree has {actual}"
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "docs/launch-audit.md disagrees with the repository it describes:\n{}\n\n\
         Every count in that table is a measurement. A measurement that is not \
         re-derived is a guess with a citation.",
        wrong.join("\n")
    );
}

/// The MSRV is a promise to every contributor and to every release build. A
/// document that states a different one sends people to pin a toolchain that is
/// older than the code requires, or to fail a build they thought would pass.
#[test]
fn the_stated_msrv_is_the_one_cargo_pins() {
    let msrv = crate_field("rust-version");
    for rel in ["docs/launch-audit.md", "CONTRIBUTING.md"] {
        let body = read(rel);
        for (i, line) in body.lines().enumerate() {
            if !line.contains("MSRV") && !line.contains("Rust 1.") {
                continue;
            }
            // Pull out any `1.NN` and require it to be the pinned one.
            for token in line.split(|c: char| !c.is_ascii_digit() && c != '.') {
                if token.starts_with("1.") && token.len() >= 4 && token[2..3] == *"" {
                    continue;
                }
                if token.starts_with("1.")
                    && token[2..].chars().all(|c| c.is_ascii_digit())
                    && token.len() == 4
                    && token != msrv
                {
                    panic!("{rel}:{i} states MSRV {token}, Cargo.toml pins {msrv}\n  {line}");
                }
            }
        }
    }
}

/// The README's project-structure block describes the crate's own modules. If
/// it names a count, the count is a claim about a tree that changes.
#[test]
fn the_readme_tool_count_matches_the_registry() {
    let readme = read("README.md");
    let tools_src = read("src/runtime/tools.rs");
    let start = tools_src
        .find("pub fn build_baseline_registry()")
        .expect("build_baseline_registry");
    let body = &tools_src[start..];
    let end = body.find("\n    reg\n}").unwrap_or(body.len());
    let registered = body[..end]
        .lines()
        .filter(|l| l.contains("reg.register("))
        .count();

    // Whatever number the README prints, it has to be this one.
    for (i, line) in readme.lines().enumerate() {
        if !line.contains("baseline tools") {
            continue;
        }
        // The count is the last digit run in the text preceding the phrase.
        let head = line.split("baseline tools").next().unwrap_or_default();
        let mut digits_found: Vec<String> = Vec::new();
        for word in head.split_whitespace() {
            let digits: String = word.chars().filter(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                digits_found.push(digits);
            }
        }
        let claimed = digits_found.last().cloned().unwrap_or_default();
        assert!(
            !claimed.is_empty(),
            "README.md:{i} mentions baseline tools without a count, so this check \
             cannot tell whether it is right. Give it a number or remove the phrase.\n  {line}"
        );
        if claimed != registered.to_string() {
            panic!(
                "README.md:{i} claims {claimed} baseline tools, \
                 build_baseline_registry() registers {registered}\n  {line}"
            );
        }
    }
}
