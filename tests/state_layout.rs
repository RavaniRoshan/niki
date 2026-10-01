//! `STATE_LAYOUT.md` is a file contract: it claims to name every persistent
//! store, its writer, and its readers. It drifted — six stores were written by
//! the product and absent from it — and nothing noticed, because a document
//! cannot fail a test on its own.
//!
//! These assertions are deliberately about the *document*, not the product.
//! The product is correct for these paths; the contract was not describing it.

use std::path::Path;

/// Every store the contract names must still exist in the source.
///
/// Deliberately one-directional. A filename scan of `src/` cannot tell a
/// *store* from a per-task artifact or a test fixture — `planner.json`,
/// `mock-script.json` and `package.json` all appear in a `.join("…")` — so
/// asserting the other direction would demand the contract document every
/// artifact and make it useless. What can be checked mechanically, and what
/// actually drifts, is the documented set: a row whose writer is renamed or
/// deleted leaves the contract describing a file nothing writes.
#[test]
fn every_documented_store_is_written_somewhere() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = std::fs::read_to_string(root.join("STATE_LAYOUT.md")).expect("STATE_LAYOUT.md");

    // Filenames claimed by the contract's **Path** column.
    //
    // Two things this got wrong first. Scanning every cell reads the Writer
    // column too, which is a function name and tells us nothing — a row with
    // a wrong path still passed because another cell on the same line named a
    // real file. And splitting on `.` and taking the tail after it yields the
    // *extension* (`json`), not the filename, so nothing ever matched.
    let mut claimed: Vec<String> = Vec::new();
    for line in doc
        .lines()
        .filter(|l| l.starts_with('|') && !l.starts_with("|---"))
    {
        let path_cell: String = line
            .split('|')
            .nth(2)
            .unwrap_or_default()
            .chars()
            .map(|c| {
                if c == '`' || c == '{' || c == '}' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        for token in path_cell.split(['/', ',', ' ', '(', ')', '*']) {
            let name = token.trim();
            let is_store = [".json", ".jsonl", ".md", ".patch"]
                .iter()
                .any(|ext| name.ends_with(ext));
            if is_store && !claimed.iter().any(|c| c == name) {
                claimed.push(name.to_string());
            }
        }
    }

    let src: Vec<String> = walk(&root.join("src"))
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();

    let missing: Vec<&String> = claimed
        .iter()
        .filter(|n| !src.iter().any(|t| t.contains(n.as_str())))
        .collect();

    assert!(
        missing.is_empty(),
        "STATE_LAYOUT.md claims these stores but nothing in src/ writes them: \
         {missing:?}. Either the row is stale or the store moved."
    );
}

// A temp patch in a user repository **is** git-ignored — and the test that
// proves it lives in `tests/patch_temp_path_is_unique.rs`, which does it
// behaviourally: it calls the product's own `ensure_patch_files_ignored`,
// writes a leftover file, runs a real `git add -A`, and asserts the leftover was
// not staged — and that the user's own file still was.
//
// The copy that used to sit here asserted that `.gitignore` contained the
// literal string `.niki-tmp.patch`. The writer stopped producing that name — it
// is `.niki-tmp.<pid>.<unique>.patch` now, and the pattern is the glob
// `.niki-tmp*.patch` — so the assertion was testing a name the product no
// longer writes. It went red, and **no gate ran this binary**, so it stayed red.
//
// Removed rather than corrected: correcting it would mean a second textual
// check of a property two behavioural tests already hold. `state_layout`'s own
// job is that the stores `STATE_LAYOUT.md` documents are the ones the code
// writes, and it is not this.

/// The contract states that JSON state writes go through `kb::write_atomic`.
/// `write_manifest` did not, while being documented as if it did — which is
/// how a convention becomes folklore.
#[test]
fn the_atomic_write_convention_holds_for_manifest() {
    let manifest_src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/orchestrator/provenance.rs"),
    )
    .expect("provenance.rs");

    let body = manifest_src
        .split("pub fn write_manifest")
        .nth(1)
        .expect("write_manifest must exist")
        .split("\n}")
        .next()
        .unwrap_or_default();

    assert!(
        body.contains("write_atomic"),
        "write_manifest writes JSON state and STATE_LAYOUT.md says JSON state \
         writes go through kb::write_atomic. It uses a plain fs::write, which \
         truncates before it writes — a reader arriving mid-write, or an \
         interrupted run, finds a half-written manifest that parses as nothing."
    );
}

/// Walk a directory tree, skipping `target` and other noise.
fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}
