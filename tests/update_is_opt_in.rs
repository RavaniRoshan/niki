//! W14 — self-update is opt-in, checksum-verified, and never automatic.
//!
//! Three properties, each asserted as a **negative** (this string never appears here), because
//! that is the only way to state "it never happens". A test can assert that a behaviour is
//! present; it cannot assert an absence except by looking for the thing that would cause it.
//!
//! `dist-workspace.toml` sets `install-updater = true`, so cargo-dist ships an `niki-update`
//! binary next to the engine. That is the shape the checklist asks for: a user runs it when they
//! choose to. What must not exist is anything that runs it *for* them.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// Every `.rs` under `src/`, which is the whole engine.
fn engine_sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![repo().join("src")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let rel = path
                    .strip_prefix(repo())
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                out.push((rel, std::fs::read_to_string(&path).unwrap_or_default()));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        out.len() > 50,
        "the engine source walk found almost nothing"
    );
    out
}

/// Nothing in the engine may spawn an updater. A self-updating binary replaces the thing that is
/// running it, which is the one operation where being wrong is unrecoverable.
#[test]
fn the_engine_never_invokes_an_updater() {
    let needles = ["niki-update", "self_update", "selfupdate"];
    let mut hits = Vec::new();
    for (rel, body) in engine_sources() {
        for line in body.lines() {
            let lower = line.to_lowercase();
            for needle in needles {
                if lower.contains(needle) {
                    hits.push(format!("  {rel}: {}", line.trim()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the engine must never update itself; an update has to be something a person runs:\n{}",
        hits.join("\n")
    );
}

/// The same for CI. A workflow step that fetches and runs an updater on every push turns every
/// commit into a supply-chain event.
#[test]
fn no_workflow_runs_an_updater_automatically() {
    let dir = repo().join(".github/workflows");
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("workflows dir").flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            for (i, line) in body.lines().enumerate() {
                let lower = line.to_lowercase();
                // `cargo dist` legitimately *builds* an updater; it must never *run* one on a
                // user's machine. So a build/upload reference is fine and a `niki-update` run is not.
                if lower.contains("run niki-update") || lower.contains("./niki-update") {
                    hits.push(format!("  {}:{}: {}", path.display(), i + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "a workflow must not run the updater automatically:\n{}",
        hits.join("\n")
    );
}

/// The updater exists on purpose, and cargo-dist's checksum verification is what makes running it
/// safe. Both are stated in `dist-workspace.toml`; if either is removed, this fails rather than
/// leaving an install that can silently do neither.
#[test]
fn the_updater_is_declared_and_checksums_are_required() {
    let dist = read("dist-workspace.toml");
    assert!(
        dist.contains("install-updater = true"),
        "the updater is expected to ship; removing it means an install can never be updated \
         without reinstalling by hand"
    );
    assert!(
        !dist.contains("checksums = false") && !dist.contains("checksum = \"false\""),
        "checksums must not be disabled for the release artifacts"
    );
}

/// An update a user did not ask for is indistinguishable from an attack. The uninstall removes
/// the updater along with the binaries, so a removed install leaves nothing behind that can be
/// run later.
#[test]
fn the_uninstall_removes_the_updater_too() {
    let script = read("scripts/uninstall.sh");
    assert!(
        script.contains("niki-update"),
        "uninstall.sh must remove the updater it ships alongside the engine"
    );
    assert!(
        script.contains("--purge") && script.contains("NIKI_INSTALL_DIR"),
        "uninstall.sh must keep its purge flag and honour the install directory"
    );
}

/// The guard above is only a check if it can fail. This is the same shape the repo already uses
/// for the supply-chain policy: point the scanner at a string it must reject, and require the
/// rejection.
#[test]
fn the_engine_scan_would_reject_an_updater_invocation() {
    let needles = ["niki-update", "self_update"];
    let decoy = r#"
fn upgrade_in_background() {
    let _ = std::process::Command::new("niki-update").status();
}
"#;
    let hits: Vec<&str> = decoy
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            needles.iter().any(|n| lower.contains(n))
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "the scan the test above uses must actually match an updater invocation; if it does not, \
         that test passes for the wrong reason. Decoy lines matched: {hits:?}"
    );
    assert!(
        hits[0].contains("niki-update"),
        "the matched line must be the invocation itself, not a comment above it: {hits:?}"
    );
}

/// `scripts/install.sh` must not run the updater either — installing is installing.
#[test]
fn the_installer_does_not_update_anything() {
    let installer = read("scripts/install.sh");
    let offending: Vec<&str> = installer
        .lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("niki-update") || lower.contains("install-updater")
        })
        .collect();
    assert!(
        offending.is_empty(),
        "install.sh must install and nothing else; these lines update something:\n{}",
        offending.join("\n")
    );
}

/// Sanity: the paths this file reads exist. A test that reads a renamed file and finds nothing
/// would pass vacuously.
#[test]
fn the_files_this_suite_reads_are_there() {
    for rel in [
        "dist-workspace.toml",
        "scripts/uninstall.sh",
        "scripts/install.sh",
    ] {
        assert!(Path::new(&repo().join(rel)).exists(), "{rel} is missing");
    }
}
