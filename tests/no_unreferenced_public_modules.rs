//! A public module nothing references is dead code the compiler will not catch.
//!
//! `src/errors.rs`, `src/control_plane/` and `src/persistence/` were 697 lines
//! of it. All three were `pub`, so `dead_code` was silent on every one — which
//! is the only reason they survived: a private item that is never used is a
//! warning, a public one is invisible, and a library's whole surface is public.
//!
//! Two of the three were not accidents of an abandoned experiment:
//!
//! - `persistence` was a complete, working mission store — superseded by
//!   `mission::MissionStore`, which the Fleet grid and the goal loop actually
//!   call (`state.rs:1403`).
//! - `control_plane` was a documented Convex mirror scaffold whose own header
//!   said "intentionally not wired into the pipeline yet".
//!
//! So they were deleted rather than left to rot, and git keeps them. What this
//! file prevents is the next one: a module that is added, wired to nothing, and
//! never noticed — which is exactly how 697 lines accumulated.
//!
//! The check is a reference count, not a call-graph analysis, and it has a
//! known blind spot written down in `a_module_only_referenced_by_itself_is_dead`.

use std::collections::BTreeMap;
use std::path::Path;

/// Every `pub mod` in `src/lib.rs`.
fn public_modules() -> Vec<String> {
    let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("src/lib.rs must be readable");
    lib.lines()
        .filter_map(|l| l.trim().strip_prefix("pub mod "))
        .filter_map(|rest| rest.split(';').next())
        .map(|name| name.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Where each module is referenced from, ignoring the module's own files.
///
/// One pass over the tree, not one per module. The first version re-read every
/// source file for each of the 32 modules and took **232 seconds** — slow
/// enough to be a defect in its own right on a box this size, and it made
/// every run of G3 noticeably slower for a check that is pure string matching.
fn references(mods: &[String]) -> BTreeMap<String, Vec<String>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let needles: Vec<(String, String)> = mods
        .iter()
        .map(|m| (m.clone(), format!("crate::{m}::")))
        .collect();

    let mut hits: BTreeMap<String, Vec<String>> = needles
        .iter()
        .map(|(m, _)| (m.clone(), Vec::new()))
        .collect();

    let mut files = Vec::new();
    for rel_dir in ["src", "tests"] {
        walk(&root.join(rel_dir), &mut files);
    }
    {
        for f in files {
            let rel = f
                .strip_prefix(root)
                .unwrap_or(&f)
                .to_string_lossy()
                .to_string();
            let Ok(body) = std::fs::read_to_string(&f) else {
                continue;
            };
            for (i, line) in body.lines().enumerate() {
                let code = match line.find("//") {
                    Some(at) => &line[..at],
                    None => line,
                };
                if code.trim().is_empty() {
                    continue;
                }
                for (name, needle) in &needles {
                    if !code.contains(needle.as_str()) {
                        continue;
                    }
                    // A module's own subtree does not count as a use.
                    if rel.starts_with(&format!("src/{name}/")) || rel == format!("src/{name}.rs") {
                        continue;
                    }
                    hits.get_mut(name)
                        .expect("every module has an entry")
                        .push(format!("{rel}:{}", i + 1));
                }
            }
        }
    }
    hits
}

/// **The check.** Every public module must be referenced from somewhere else.
#[test]
fn no_public_module_is_unreferenced() {
    let mods = public_modules();
    assert!(
        mods.len() >= 25,
        "expected the library surface to be at least 25 modules, found {} — \
         the parser is broken, not the tree",
        mods.len()
    );

    let hits = references(&mods);
    let dead: BTreeMap<String, Vec<String>> =
        hits.into_iter().filter(|(_, v)| v.is_empty()).collect();

    assert!(
        dead.is_empty(),
        "these public modules are referenced from nowhere: {dead:?}\n\n\
         A `pub` item that is never used is invisible to `dead_code`, which is \
         how 697 lines of it accumulated here. Either wire it or delete it — \
         git keeps the history either way."
    );
}

/// The three that were here, named so a re-introduction is a deliberate act
/// rather than a resurrection by accident.
#[test]
fn the_three_removed_modules_stay_removed() {
    let mods = public_modules();
    for gone in ["errors", "control_plane", "persistence"] {
        assert!(
            !mods.contains(&gone.to_string()),
            "`{gone}` is declared again. If it is being wired now, that is \
             progress — but it must be wired, not merely declared: the last \
             time these three existed they were referenced by nothing."
        );
        assert!(
            !Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("src/{gone}.rs"))
                .exists()
                && !Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("src/{gone}"))
                    .exists(),
            "src/{gone} is back on disk while lib.rs does not declare it — an \
             orphaned file nothing can reach"
        );
    }
}

/// The blind spot, named so nobody trusts this suite further than it deserves.
///
/// A module referenced only from **its own** subtree — a helper calling a
/// sibling helper, with nothing outside reaching either — counts as live here.
/// That is the same blind spot `dead_code` has, narrowed but not closed. The
/// way to close it is `cargo +nightly udeps` or `cargo-machete`, both of which
/// want a different build than this gate can afford on a 7.5 GiB box.
#[test]
fn a_module_only_referenced_by_itself_is_dead() {
    // Stated as a passing test so the limitation is in the suite rather than
    // in a comment someone skips: if a future tool closes this gap, this test
    // is the one to delete.
    let mods = public_modules();
    assert_eq!(
        mods.len(),
        32,
        "the module count changed. If a new module appeared, check whether it \
         is referenced from outside its own subtree — this suite cannot tell, \
         and that is its known blind spot."
    );
}
