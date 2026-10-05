//! The foundation's documents exist, and they are documents rather than placeholders.
//!
//! Every row of the foundation plan ends in a claim that something was written down, and a claim
//! about a document that nothing reads rots silently. This file is the reader. It does not check
//! whether a document is *correct* — a document can be confidently wrong and pass every assertion
//! here — it checks the weaker and more mechanical property that the file exists, is big enough to
//! contain an argument, and is found where the plan said it would be.
//!
//! A one-line `KEYMAP.md` that says "TODO" satisfies "the file exists" and satisfies nothing else.
//! The size floor is what turns that into a failure.
//!
//! The byte size of each path is printed as the test runs, because the output of this test is
//! meant to be the evidence: a reviewer reads the sizes, not the summary line.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn docs_root() -> PathBuf {
    repo_root().join("docs").join("foundation")
}

/// Each document the foundation claims to produce, and the smallest it may plausibly be.
///
/// The floor is deliberately low. Its job is to reject an empty file, a stub and a placeholder,
/// not to second-guess an author mid-sentence.
const DOCUMENTS: &[(&str, &str, usize)] = &[
    ("DESIGN.md", "the shell and engine design decisions", 2_000),
    ("ARCHITECTURE.md", "module map and event flow", 2_000),
    (
        "EVENT_MAP.md",
        "every protocol message and who sends it",
        500,
    ),
    (
        "GAPS.md",
        "what NIKI cannot show, and what does not exist yet",
        2_000,
    ),
    ("CHECKLIST.md", "the row-by-row proof ledger", 5_000),
    (
        "PROGRESS.md",
        "the build log with recorded run output",
        5_000,
    ),
    (
        "OWNER_VERIFY.md",
        "what the owner has to verify by hand",
        2_000,
    ),
    (
        "PARITY.md",
        "reference capabilities NIKI lacks, with a recommendation each",
        2_000,
    ),
    ("KEYMAP.md", "the generated key and command map", 2_000),
];

/// The frame dumps the owner looks at when judging the interface by eye.
const MIN_REVIEW_DUMPS: usize = 12;

fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()))
        .len()
}

/// Every foundation document is present, and is large enough to be an argument rather than a stub.
#[test]
fn every_foundation_document_exists_and_is_not_a_stub() {
    let root = docs_root();
    assert!(
        root.is_dir(),
        "docs/foundation/ must exist at {}",
        root.display()
    );

    let mut missing: Vec<&str> = Vec::new();
    let mut thin: Vec<String> = Vec::new();

    for (name, what, floor) in DOCUMENTS {
        let path = root.join(name);
        if !path.is_file() {
            missing.push(name);
            println!("MISSING  {:>9} B  {}", 0, path.display());
            continue;
        }
        let bytes = size_of(&path);
        println!("OK       {bytes:>9} B  {}  ({what})", path.display());
        if bytes < *floor as u64 {
            thin.push(format!("{name} is {bytes} B, below the {floor} B floor"));
        }
    }

    assert!(
        missing.is_empty(),
        "foundation documents do not exist: {missing:?}"
    );
    assert!(thin.is_empty(), "foundation documents are stubs: {thin:#?}");
}

/// KEYMAP.md says it is generated and names the command that regenerates it.
///
/// A generated file that does not say so is indistinguishable from a hand-written one, and the
/// next person to change a binding has no reason to run the generator.
#[test]
fn the_keymap_declares_how_it_is_generated() {
    let body = std::fs::read_to_string(docs_root().join("KEYMAP.md")).expect("read KEYMAP.md");
    assert!(
        body.contains("GENERATED"),
        "KEYMAP.md must carry a header saying it is generated"
    );
    assert!(
        body.contains("gen-keymap.ts"),
        "KEYMAP.md must name the command that regenerates it"
    );
}

/// The review frame dumps exist, in the number the plan promised.
///
/// These are the one artefact a human judges rather than a machine, so a missing or thin set is
/// worse than a missing test: it removes the only channel through which the owner can see the
/// interface without building it.
#[test]
fn the_review_frame_dumps_are_there_to_be_looked_at() {
    let dir = docs_root().join("review");
    assert!(
        dir.is_dir(),
        "docs/foundation/review/ must exist at {}",
        dir.display()
    );

    let mut dumps: Vec<(String, u64)> = Vec::new();
    let mut thin: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("entry in {}: {e}", dir.display()));
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let bytes = size_of(&path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // A frame dump that is empty or near-empty is a frame that failed to render, which is the
        // one outcome the dump exists to catch. The floor is low on purpose: `11-narrow-49_*` is a
        // correct capture of a terminal too narrow to render in, so it is two short lines of text
        // by design and would fail any floor set for the rendered frames.
        if bytes < 20 {
            thin.push(format!("{name} is {bytes} B"));
            continue;
        }
        dumps.push((name, bytes));
    }

    dumps.sort();
    for (name, bytes) in &dumps {
        println!("FRAME    {bytes:>9} B  {}", dir.join(name).display());
    }

    assert!(
        thin.is_empty(),
        "review dumps are empty, so the capture failed: {thin:#?}"
    );
    assert!(
        dumps.len() >= MIN_REVIEW_DUMPS,
        "docs/foundation/review/ holds {} frame dumps, and the plan promised at least {MIN_REVIEW_DUMPS}: {}",
        dumps.len(),
        dir.display()
    );
}
