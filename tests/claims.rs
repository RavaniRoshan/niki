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
///
/// This used to be a hand-written list of three files plus five prompts, and
/// that list is the reason a whole class of drift went unnoticed for five
/// releases. The product ships a 41-page documentation site, a 13 KB
/// `niki.example.toml` that users copy verbatim, 23 subcommands of `--help`
/// text, and a README full of numbers — and none of them were in scope. So
/// `docs/launch-audit.md` could sit at the top of the repository still claiming
/// version 0.4.0, "~30,600 lines" and "Hooks: Not implemented", with the CI
/// badge green, because no assertion had ever read it.
///
/// A list is a promise to remember to update it, and nobody remembers. So this
/// walks declared roots instead: a new page under `docs/content/` is covered the
/// moment it is added, with no edit here. The declarations are the deliberate
/// part; the enumeration is not.
fn claim_surfaces() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();

    // Single files a user is certain to read.
    for rel in [
        "README.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "niki.example.toml",
        "src/display/tips.rs",
    ] {
        let p = repo_root().join(rel);
        if p.exists() {
            out.push((rel.to_string(), read(rel)));
        }
    }

    // Directory roots: everything a person can be told.
    collect_ext(&repo_root().join("docs"), &["md", "mdx"], &mut out);
    collect_ext(&repo_root().join("prompts"), &["md"], &mut out);
    // The handover starter is the first thing a newcomer reads, and it is the
    // document most likely to send them to a command that does not exist. It
    // is in scope for the same reason the docs site is.
    collect_ext(&repo_root().join("niki-starter"), &["md", "mdx"], &mut out);

    // Nothing found is itself the failure mode this change exists to prevent, so
    // the guard below is not optional decoration.
    out
}

fn collect_ext(dir: &Path, exts: &[&str], out: &mut Vec<(String, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            // `node_modules` under docs/ is a build artefact, not a surface.
            if p.file_name()
                .is_some_and(|n| n == "node_modules" || n == "package-lock.json")
            {
                continue;
            }
            collect_ext(&p, exts, out);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| exts.contains(&x))
            && let Ok(t) = std::fs::read_to_string(&p)
        {
            let rel = p
                .strip_prefix(repo_root())
                .unwrap_or(&p)
                .display()
                .to_string();
            out.push((rel, t));
        }
    }
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

/// Commands that take a sub-subcommand, and the enum each one dispatches to.
///
/// Only for parents where *every* second word is a subcommand. Where a command
/// mixes subcommands and flags (`niki session undo`, `niki smoke --backend`)
/// the second word cannot be resolved from the text alone, and guessing
/// produced a test that failed on perfectly good copy.
const SUBCOMMAND_PARENTS: &[(&str, &str)] = &[
    ("config", "src/cli/config.rs"),
    ("auth", "src/cli/auth.rs"),
    ("architecture", "src/cli/architecture.rs"),
    ("index", "src/cli/index.rs"),
    ("skills", "src/cli/skills.rs"),
    ("commands", "src/cli/commands.rs"),
    ("session", "src/cli/session.rs"),
    ("memory", "src/cli/memory.rs"),
];

/// The sub-subcommands each parent accepts, derived from its clap enum.
fn subcommands_for(parent: &str) -> Option<Vec<String>> {
    let path = SUBCOMMAND_PARENTS.iter().find(|(p, _)| *p == parent)?.1;
    let src = std::fs::read_to_string(repo_root().join(path)).ok()?;
    let start = src
        .lines()
        .position(|l| l.starts_with("pub enum ") && l.contains("Commands"))
        .map(|idx| {
            src.lines()
                .take(idx)
                .map(|l| l.len() + 1)
                .sum::<usize>()
        })
        .or_else(|| src.find("pub enum "))?;
    let body = &src[start..];
    let end = body.find("\n}\n").unwrap_or(body.len());
    let body = &body[..end];
    let variants: Vec<String> = re_derive_variants(body)
        .into_iter()
        .map(|v| to_snake(&v))
        .collect();
    if variants.is_empty() {
        None
    } else {
        Some(variants)
    }
}

/// Every `niki <cmd> [<sub>]` reference in the documented surfaces must resolve.
///
/// The first version of this check ran over `src/**/*.rs` only, and only ever
/// looked at the *first* word after `niki`. That is honest about what it can
/// assert — the second word might be a flag, a value, or the start of a quoted
/// task — but it left the documentation site entirely unchecked, which is where
/// a user actually looks.
///
/// So: the surfaces are the walk, and the second word is checked wherever the
/// parent command has a subcommand enum, because there the second word is
/// unambiguously a sub-subcommand. `niki config chek` fails, as it should.
#[test]
fn every_niki_command_in_every_documented_surface_exists() {
    let mut known: Vec<String> = Vec::new();
    let main_rs = std::fs::read_to_string(repo_root().join("src/main.rs")).expect("main.rs");
    let body = &main_rs[main_rs.find("enum Commands {").expect("Commands")..];
    let body = &body[..body.find("\n#[tokio::main]").unwrap_or(body.len())];
    for variant in re_derive_variants(body) {
        known.push(to_snake(&variant));
    }
    assert!(
        known.len() > 20,
        "only {} commands were derived from main.rs; the derivation has broken and \
         every assertion below would pass vacuously",
        known.len()
    );

    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for (rel, body) in claim_surfaces() {
        for reference in backticked_niki_commands(&body) {
            let mut words = reference.split_whitespace();
            if words.next() != Some("niki") {
                continue;
            }
            let Some(sub) = words.next() else { continue };
            // A token beginning with `-` is a flag, not a command; one wrapped
            // in `<>` or `[]` is a placeholder, not a command. The first version
            // of this check treated both as commands and reported `niki version`
            // for `niki --version` and `niki command` for `niki <command> --help`
            // — which is how a gate earns a reputation for being wrong.
            if is_flag_or_placeholder(sub) {
                continue;
            }
            if !known.iter().any(|k| k == sub) {
                violations.push(format!(
                    "  {rel}: `niki {sub}` is not a command (known: {})",
                    known.join(", ")
                ));
                continue;
            }
            checked += 1;

            // Second word, only where it can only be a sub-subcommand.
            let Some(second) = words.next() else { continue };
            if is_flag_or_placeholder(second) {
                continue;
            }
            let Some(subs) = subcommands_for(sub) else {
                continue;
            };
            // `niki index build|query` and `niki session list/show` are shorthand
            // for alternatives, and a user reads them that way. Every
            // alternative is checked; a single bad one still fails.
            let mut bad = Vec::new();
            // In a markdown table cell a literal pipe is written `\|`, so the
            // backslash has to go before the alternatives are split — otherwise
            // `niki index build\|query` reads as a subcommand called `build\`,
            // which is a defect in the checker rather than in the document.
            let second = second.replace('\\', "");
            for alt in second.split(['|', '/']) {
                let alt = alt.trim();
                if alt.is_empty() || is_flag_or_placeholder(alt) {
                    continue;
                }
                if !subs.iter().any(|s| s == alt) {
                    bad.push(alt.to_string());
                }
            }
            if !bad.is_empty() {
                violations.push(format!(
                    "  {rel}: `niki {sub} {}` is not a subcommand of `niki {sub}` \
                     (known: {})",
                    bad.join(", "),
                    subs.join(", ")
                ));
            }
        }
    }

    assert!(
        !violations.is_empty() || checked > 50,
        "the scan checked {checked} command references; a walk that finds almost \
         nothing is not scanning the surfaces it claims to"
    );
    assert!(
        violations.is_empty(),
        "these documented surfaces tell the user to run commands that do not exist:\n{}\n\n\
         A user copies one of these, gets a usage error, and concludes the tool does \
         not know what it is talking about.",
        violations.join("\n")
    );
}

/// Every relative link in the documentation must resolve.
///
/// Cheap, and it catches the class of rot this repository is full of: a file
/// that has existed since before the docs were restructured, pointing at a path
/// that has not existed for months. `CONTRIBUTING.md` links its own licence as
/// `../LICENSE` from the repository root, which resolves to the parent of the
/// repository — a link that works in a web viewer's imagination and nowhere on
/// disk.
#[test]
fn every_relative_link_in_the_documentation_resolves() {
    let mut broken: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for (rel, body) in claim_surfaces() {
        if !rel.ends_with(".md") && !rel.ends_with(".mdx") {
            continue;
        }
        let base = repo_root().join(&rel).parent().unwrap().to_path_buf();
        for target in markdown_links(&body) {
            // External, absolute, in-page anchors and mailto are not ours.
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('#')
                || target.starts_with('/')
            {
                continue;
            }
            // Drop any `#anchor` suffix.
            let path = target.split('#').next().unwrap_or(&target);
            if path.is_empty() {
                continue;
            }
            checked += 1;
            if !base.join(path).exists() {
                broken.push(format!("  {rel}: [{target}]"));
            }
        }
    }

    assert!(
        checked > 0,
        "no relative links were found; the link extractor has broken"
    );
    assert!(
        broken.is_empty(),
        "these documentation links do not resolve to anything on disk:\n{}\n\n\
         A dead link in a file a newcomer reads is the cheapest possible way to \
         teach them that the documentation is not maintained.",
        broken.join("\n")
    );
}

/// Is this token a flag, a placeholder, or a version rather than a command?
fn is_flag_or_placeholder(token: &str) -> bool {
    let t = token.trim();
    if t.is_empty() {
        return true;
    }
    // `--version`, `-i`, `-p/1234`
    if t.starts_with('-') {
        return true;
    }
    // `<command>`, `[subcommand]`, `{stage}`
    if (t.starts_with('<') && t.ends_with('>'))
        || (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('{') && t.ends_with('}'))
    {
        return true;
    }
    // A version: `0.9.0`, `v0.8.0`, `1.88`. Output, not a command.
    let core = t.strip_prefix('v').unwrap_or(t);
    if !core.is_empty() && core.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// Markdown link targets, in order of appearance.
fn markdown_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == '[' {
            // Find the matching `](`.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != ']' {
                j += 1;
            }
            if j + 1 < bytes.len() && bytes[j] == ']' && bytes[j + 1] == '(' {
                let mut k = j + 2;
                let mut target = String::new();
                while k < bytes.len() && bytes[k] != ')' {
                    target.push(bytes[k]);
                    k += 1;
                }
                // Drop a title: `[a](path "title")`.
                let target = target
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches('"')
                    .to_string();
                if !target.is_empty() {
                    out.push(target);
                }
                i = k;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The walk has to actually cover the documentation, or every assertion built
/// on it is decoration.
///
/// This is the guard that would have caught the original design error. When
/// `claim_surfaces()` was three hardcoded filenames, everything here passed
/// while the documentation site said the product needed a container runtime and
/// an API key — the two things the README opens by saying you do not need.
#[test]
fn the_claim_walk_covers_the_documentation_site() {
    let surfaces = claim_surfaces();
    let names: Vec<&str> = surfaces.iter().map(|(n, _)| n.as_str()).collect();

    for required in [
        "README.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "niki.example.toml",
    ] {
        assert!(
            names.contains(&required),
            "{required} is not in scope, so nothing checks it"
        );
    }

    // The starter is the handover artefact: a student is handed this and
    // nothing else. Its links and its commands are checked like any other
    // surface, and a broken one lands on the first person to try it.
    for required in [
        "niki-starter/README.md",
        "niki-starter/HONESTY.md",
        "niki-starter/TROUBLESHOOTING.md",
        "niki-starter/REPORT-GUIDE.md",
    ] {
        assert!(
            names.contains(&required),
            "{required} is missing or not in scope. The handover starter is the \
             first thing a newcomer reads; a claim gate that skips it is a gate \
             that skips the reader."
        );
    }

    let mdx = names.iter().filter(|n| n.ends_with(".mdx")).count();
    assert!(
        mdx >= 30,
        "only {mdx} documentation pages are in scope; the site has 41 and they are \
         the surface a user reads first"
    );

    let docs_md = names
        .iter()
        .filter(|n| n.starts_with("docs/") && n.ends_with(".md"))
        .count();
    assert!(
        docs_md >= 5,
        "only {docs_md} of the docs/*.md files are in scope"
    );

    // And the prompts, which is where the original false claim lived.
    assert!(
        names.contains(&"prompts/base.md"),
        "prompts/base.md must stay in scope"
    );
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

/// The README's MCP row must agree with what the code does — in **both**
/// directions.
///
/// Batch 6-05 wrote this the other way: the row advertised a half-built
/// feature, so the test demanded a hedge. Batch 7 wired the feature, so the
/// same row became a lie in reverse — a user reads "cannot call their tools
/// yet", configures a server, watches the agent ignore it, and concludes the
/// product is broken.
///
/// A one-directional honesty test is a test that has to be rewritten every time
/// the feature moves. So this one asks the question directly: is the call path
/// registered, and does the row say so?
#[test]
fn the_readme_mcp_row_matches_what_the_code_does() {
    let readme = read("README.md");
    let mcp_rows: Vec<&str> = readme
        .lines()
        .filter(|l| l.to_lowercase().contains("mcp") && l.contains('|'))
        .collect();
    assert!(
        !mcp_rows.is_empty(),
        "MCP should still be in the README's feature table"
    );

    // Is the feature actually there? Counted, not grepped for a phrase: the
    // question is whether a discovered tool becomes one the loop can dispatch.
    let pipeline = read("src/orchestrator/pipeline.rs");
    let wired = pipeline.contains("build_registry(&mut registry");
    let row = mcp_rows[0].to_lowercase();

    if wired {
        assert!(
            !row.contains("not yet") && !row.contains("cannot call"),
            "the loop registers MCP tools now, so a row saying they are not \
             callable sends a user looking for a fault that is not there: {:?}",
            mcp_rows[0]
        );
    } else {
        assert!(
            row.contains("not yet") || row.contains("cannot call"),
            "the loop does NOT register MCP tools, so a row advertising them \
             as usable is the original defect: {:?}",
            mcp_rows[0]
        );
    }
}

/// And the example config must not tell a user their MCP tools reach the
/// model. They do not: `McpManager::tools_summary` says "NOT YET CALLABLE"
/// and routes to a display notice, never to a prompt.
#[test]
fn the_example_config_does_not_claim_mcp_tools_reach_the_agent() {
    let example = read("niki.example.toml");
    let mcp_block: String = example
        .lines()
        .skip_while(|l| !l.contains("MCP (Model Context Protocol)"))
        .take_while(|l| !l.trim_start().starts_with('#') || l.contains("MCP"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        mcp_block.contains("MCP"),
        "the [mcp] section must still be documented — it is real config"
    );
    for lie in [
        // Batch 6-05. Still true in batch 7: the tools reach the *tool loop*,
        // not an agent's system prompt, and nothing is concatenated into a
        // prompt anywhere.
        "injected into agent prompts",
        "extending their capabilities",
        // A command that does not exist. A user who copies it gets a usage
        // error and concludes the tool does not know what it is talking about
        // — and `every_niki_command_in_every_documented_surface_exists` is the
        // gate that caught this one being written.
        "niki mcp list",
    ] {
        assert!(
            !mcp_block.to_lowercase().contains(lie),
            "the example config claims {lie:?}, which the code does not do: \
             no MCP tool is ever called by an agent, so nothing is injected \
             into any prompt. The block reads:\n{mcp_block}"
        );
    }
}
