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

/// The README's test-count banner must not drift by releases.
///
/// It said "754 unit tests · 448 integration tests across 37 binaries" for five
/// releases while the actual numbers were 946 / ~540 / 53. It is the first line
/// of the README, it is the most checkable claim in the repository, and in a
/// project whose entire pitch is "proof, not promises" it was the one number
/// that was simply typed in and never re-derived.
///
/// The check is a *tolerance*, not an equality. `cargo test` reports 946 cases
/// where the source has 939 `#[test]` attributes, because some are
/// parameterised; pinning an exact figure would mean re-deriving one that
/// depends on the runner. A 20% band catches five releases of drift — which is
/// what actually happened — and does not fail on a handful of new tests.
#[test]
fn the_readme_test_counts_are_not_five_releases_stale() {
    const TOLERANCE: f64 = 0.20;

    let mut src_tests = 0usize;
    fn count_attr(dir: &Path, n: &mut usize) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                count_attr(&p, n);
            } else if p.extension().is_some_and(|x| x == "rs")
                && let Ok(t) = std::fs::read_to_string(&p)
            {
                *n += t.matches("#[test]").count() + t.matches("#[tokio::test]").count();
            }
        }
    }
    count_attr(&repo_root().join("src"), &mut src_tests);

    let mut integration_tests = 0usize;
    let Ok(rd) = std::fs::read_dir(repo_root().join("tests")) else {
        panic!("tests/ is not readable, so this assertion cannot be trusted");
    };
    let mut binaries = 0usize;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() || p.extension().is_some_and(|x| x != "rs") {
            continue;
        }
        binaries += 1;
        if let Ok(t) = std::fs::read_to_string(&p) {
            integration_tests += t.matches("#[test]").count() + t.matches("#[tokio::test]").count();
        }
    }

    let readme = read("README.md");
    let banner = readme
        .lines()
        .find(|l| l.contains("unit tests") && l.contains("integration tests"))
        .unwrap_or_else(|| {
            panic!(
                "README.md no longer states test counts; if the claim is gone, \
                    delete this test rather than let it check nothing"
            )
        });

    let number_before = |needle: &str| -> Option<usize> {
        let idx = banner.find(needle)?;
        let head = &banner[..idx];
        let digits: String = head
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            return None;
        }
        digits.chars().rev().collect::<String>().parse().ok()
    };

    let mut wrong: Vec<String> = Vec::new();

    if let (Some(stated), _) = (number_before("unit tests"), ()) {
        let actual = src_tests as f64;
        if (stated as f64 - actual).abs() / actual > TOLERANCE {
            wrong.push(format!(
                "  README says {stated} unit tests, src/ has {src_tests} test attributes"
            ));
        }
    }

    if let Some(stated) = number_before("integration tests") {
        let actual = integration_tests as f64;
        if (stated as f64 - actual).abs() / actual > TOLERANCE {
            wrong.push(format!(
                "  README says {stated} integration tests, tests/ has {integration_tests} \
                 test attributes"
            ));
        }
    }

    if let Some(stated) = number_before("binaries") {
        let actual = binaries as f64;
        if (stated as f64 - actual).abs() / actual > TOLERANCE {
            wrong.push(format!(
                "  README says {stated} test binaries, tests/ holds {binaries}"
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "the README's test counts have drifted past the {TOLERANCE:.0}% band:\n{}\n\n\
         These are the most checkable numbers in the repository and they were wrong \
         for five releases. `cargo test` reports slightly more cases than there are \
         attributes, which is why this is a band and not an equality.",
        wrong.join("\n")
    );
}

/// The roadmap must not ship a stale version heading.
///
/// It read "### v0.7.0 (shipped)" as the newest shipped release while the
/// crate was five versions further on, so the newest thing in the project
/// looked like it was four releases ago. The heading is now `### Shipped` —
/// a version number there is a maintenance obligation attached to a heading
/// that has to be edited on every release, and it was being forgotten.
#[test]
fn the_roadmap_does_not_pin_a_stale_version_as_its_newest_entry() {
    let readme = read("README.md");
    let version = crate_version();

    let roadmap = readme
        .split("## Roadmap")
        .nth(1)
        .expect("README has a Roadmap section");

    for line in roadmap.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("### v").or_else(|| t.strip_prefix("### V")) else {
            continue;
        };
        let stated: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let stated = stated.trim_end_matches('.').to_string();
        if stated.is_empty() {
            continue;
        }
        // A version older than the crate is legitimate under "Later"; one
        // presented as the newest shipped entry is not.
        if stated == version {
            continue;
        }
        let is_newest_shipped = roadmap
            .lines()
            .take_while(|l| !l.trim().starts_with("## "))
            .any(|l| l.trim() == t);
        if is_newest_shipped && t.to_lowercase().contains("shipped") {
            panic!(
                "README.md's roadmap presents v{stated} as the newest shipped entry, \
                 but the crate is {version}. A roadmap heading pinned to a version \
                 number is a maintenance obligation that gets forgotten — use \
                 `### Shipped` and let the CHANGELOG carry the version."
            );
        }
    }
}

/// The README must not tell a reader that stages run in containers when the
/// lead paragraph says the opposite.
///
/// It did, three paragraphs apart: the hero says "**No API key and no container
/// runtime required**", and the demo section said NIKI "runs a four-stage agent
/// pipeline in an isolated container". A reader who only skims takes the second
/// one as the description of the product. The honest sentence names both
/// backends, because both are supported and which one you get depends on your
/// machine.
#[test]
fn the_readme_names_both_backends_where_it_describes_a_run() {
    let readme = read("README.md");
    let lower = readme.to_lowercase();

    if !lower.contains("no api key and no container runtime") {
        // The lead was reworded; nothing to cross-check against.
        return;
    }

    let run_sentence = readme
        .lines()
        .find(|l| l.contains("four-stage agent pipeline") && l.contains("branch to review"))
        .expect("the demo paragraph that describes a run");

    let lower_sentence = run_sentence.to_lowercase();
    assert!(
        lower_sentence.contains("worktree") || lower_sentence.contains("container"),
        "the paragraph describing a run must say which sandbox it uses, so it \
         cannot contradict the lead three paragraphs above:\n  {run_sentence}"
    );
    assert!(
        !lower_sentence.contains("in an isolated container") || lower_sentence.contains("worktree"),
        "the run paragraph claims container isolation unconditionally, while the \
         lead says no container runtime is required. Both are true of different \
         configurations:\n  {run_sentence}"
    );
}
