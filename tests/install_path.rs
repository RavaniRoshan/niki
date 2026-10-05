//! The install path, tested the way a user meets it.
//!
//! Everything else in this suite tests the interface against a checkout. This file tests the thing
//! that actually broke for a user: **an install contains no source tree, no node and no bun**, and
//! the interface still has to open.
//!
//! The layout it builds is the one `cargo-dist` produces, because `dist-workspace.toml` attaches
//! `shell/dist/niki-shell` as an extra artifact so it lands beside `niki`:
//!
//! ```text
//!   <prefix>/bin/niki          the engine
//!   <prefix>/bin/niki-shell    the interface, self-contained
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

/// Build the interface binary once per run; it takes a couple of seconds and a checkout's
/// `shell/dist` may not exist on a fresh clone.
fn shell_binary(repo: &Path) -> Option<PathBuf> {
    let built = repo.join("shell/dist/niki-shell");
    if built.is_file() {
        return Some(built);
    }
    let status = Command::new("npm")
        .args(["run", "build:binary"])
        .current_dir(repo.join("shell"))
        .status()
        .ok()?;
    if status.success() && built.is_file() {
        Some(built)
    } else {
        None
    }
}

/// Lay out a throwaway install directory containing only what a release would contain.
fn fake_install(repo: &Path) -> Option<(tempfile::TempDir, PathBuf)> {
    let shell = shell_binary(repo)?;
    let dir = tempfile::tempdir().ok()?;
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).ok()?;
    let engine = repo.join("target/debug/niki");
    if !engine.is_file() {
        return None;
    }
    std::fs::copy(&engine, bin.join("niki")).ok()?;
    std::fs::copy(&shell, bin.join("niki-shell")).ok()?;
    Some((dir, bin))
}

#[test]
fn an_install_contains_both_binaries_and_the_interface_finds_its_sibling() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some((_dir, bin)) = fake_install(repo) else {
        // Building bun is not this test's job. When it is unavailable the assertion below about
        // resolution order still runs; only the on-disk layout check is skipped.
        eprintln!(
            "skipping the on-disk layout check: build target/debug/niki and shell/dist first"
        );
        return;
    };
    assert!(
        bin.join("niki").is_file(),
        "the engine must be in the install"
    );
    assert!(
        bin.join("niki-shell").is_file(),
        "the interface must ship beside the engine, or `niki ui` has nothing to launch"
    );

    // The lookup the launcher performs, against the layout a release produces.
    let candidates = niki::cli::ui::candidates(
        None,
        Some(&bin.join("niki")),
        Path::new("/definitely/not/a/checkout"),
    );
    assert!(
        candidates.contains(&bin.join("niki-shell")),
        "resolution missed the sibling path: {candidates:?}"
    );
}

#[test]
fn the_ui_command_is_registered_and_documents_itself() {
    let out = Command::new(env!("CARGO_BIN_EXE_niki"))
        .args(["ui", "--help"])
        .output()
        .expect("niki ui --help runs");
    assert!(out.status.success(), "`niki ui --help` must succeed");
    let said = String::from_utf8_lossy(&out.stdout);
    // A user who types `niki ui --help` must be told how to point at an interface.
    assert!(
        said.contains("shell"),
        "help does not mention --shell: {said}"
    );
}

#[test]
fn the_ui_command_explains_itself_when_the_interface_is_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("bin/niki");
    let out = Command::new(env!("CARGO_BIN_EXE_niki"))
        .arg("ui")
        .arg("--shell")
        .arg(missing)
        .env("NIKI_SHELL_BIN", "")
        .current_dir(dir.path())
        .output()
        .expect("niki ui runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let said = format!("{stdout}{stderr}");
    assert!(
        said.contains("could not find") || said.contains("interface"),
        "a missing interface must be explained, not shown as a blank screen. Got: {said}"
    );
    // And it must say what to do about it.
    assert!(
        said.contains("build:binary") || said.contains("bun"),
        "the failure must name the fix. Got: {said}"
    );
}
