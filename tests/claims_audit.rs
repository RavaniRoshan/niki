//! `docs/claims-audit.md` is a promise that every public claim is reproducible
//! from the repository. Nothing checked that, so the file accumulated two
//! things it should not have: line-number citations into a 5,000-line file
//! that had moved, and an open item asserting a packaging shape the project
//! had left behind.
//!
//! These are deliberately narrow. A document cannot be verified by a test, but
//! the *specific* claims this release made can be, and the citations can at
//! least be checked for existing.

use std::path::Path;

fn audit() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/claims-audit.md"))
        .expect("docs/claims-audit.md must exist")
}

/// Every `path:line` citation in the audit must point at a file that exists and
/// have at least that many lines.
///
/// Line numbers drift, which is why this checks existence and length rather
/// than content. A citation into a file that was renamed, or past the end of
/// one that shrank, is a claim nobody can follow — and that is the failure
/// that had actually happened here.
#[test]
fn every_line_citation_in_the_audit_resolves() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = audit();

    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();

    // `src/foo.rs:123` — a path, an extension, a colon, a number. Walked over
    // byte offsets into the original text; slicing a moving remainder made the
    // offsets meaningless and the check silently passed on nothing.
    let bytes = text.as_bytes();
    for idx in text.match_indices(".rs:").map(|(i, _)| i) {
        let Some(src_at) = text[..idx].rfind("src/") else {
            continue;
        };
        let delim = text[..src_at]
            .rfind(|c: char| c.is_whitespace() || c == '`' || c == '(' || c == '[')
            .map(|i| i + 1)
            .unwrap_or(0);
        // idx points AT ".rs:", so the slice excludes the extension.
        let path = format!("{}.rs", &text[delim..idx]);
        if !path.contains('/') {
            continue;
        }
        let digits: String = bytes[idx + 4..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .map(|b| *b as char)
            .collect();
        if digits.is_empty() {
            continue;
        }
        checked += 1;

        let full = root.join(&path);
        if !full.is_file() {
            bad.push(format!("{path} — no such file"));
            continue;
        }
        let lines = std::fs::read_to_string(&full)
            .map(|t| t.lines().count())
            .unwrap_or(0);
        let n: usize = digits.parse().unwrap_or(0);
        if n > lines {
            bad.push(format!("{path}:{n} — file has {lines} lines"));
        }
    }

    assert!(
        checked > 0,
        "the audit cites no source lines; the check is vacuous"
    );
    assert!(
        bad.is_empty(),
        "docs/claims-audit.md cites source that no longer resolves: {bad:?}"
    );
}

/// The audit must say which release it was verified against, and must not
/// claim a full re-verification it did not do.
#[test]
fn the_audit_states_its_verification_scope() {
    let text = audit();
    assert!(
        text.contains("Last verified:"),
        "the audit must date its verification, or it reads as current forever"
    );
    assert!(
        text.contains("Not") && text.contains("re-verified"),
        "the audit must say what this pass did NOT cover. A document that \
         implies full coverage is the failure it exists to prevent."
    );
}

/// Every claim row must cite evidence.
///
/// A row in a verification table with an empty or prose-only evidence column is
/// a marketing assertion wearing the table's clothes. The audit's whole value
/// is the mapping, so a row that skips it is worse than no row: it looks
/// checked.
#[test]
fn every_claim_row_cites_evidence() {
    let text = audit();
    let mut unchecked: Vec<String> = Vec::new();

    // The "overstated claims" correction table is a different shape on
    // purpose: its third column is the *corrected copy*, not a source. Skip
    // everything from its heading onward rather than modelling it.
    let body = text
        .split("## Claims that were OVERSTATED")
        .next()
        .unwrap_or(&text);

    for line in body
        .lines()
        .filter(|l| l.starts_with('|') && !l.starts_with("|---"))
    {
        let cells: Vec<&str> = line.split('|').collect();
        // Skip the header row and the summary/correction tables, which have a
        // different shape on purpose.
        if cells.len() < 4 {
            continue;
        }
        let claim = cells[1].trim();
        // A trailing `|` leaves an empty final cell; the evidence is the one before it.
        let evidence = cells
            .get(cells.len().saturating_sub(2))
            .copied()
            .unwrap_or("")
            .trim();
        // Skip headers: the main tables' header, and the header of the
        // "overstated claims" correction table, which is a different shape on
        // purpose.
        if claim.is_empty()
            || claim.eq_ignore_ascii_case("claim")
            || claim.eq_ignore_ascii_case("original claim")
        {
            continue;
        }
        // A row is evidenced if its evidence cell names a source file or a
        // test, or points at a sibling table in this same document.
        let cites = evidence.contains(".rs")
            || evidence.contains(".toml")
            || evidence.contains("tests/")
            || evidence.contains("`src")
            || evidence.contains("repo")
            || evidence.contains("README")
            || evidence.contains("CHANGELOG");
        if !cites {
            unchecked.push(claim.to_string());
        }
    }

    assert!(
        unchecked.is_empty(),
        "these claim rows cite no verifiable evidence: {unchecked:?}"
    );
}
