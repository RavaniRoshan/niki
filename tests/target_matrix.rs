//! W16 — every target the release ships is built somewhere, and every one CI builds is one the
//! release ships.
//!
//! Two directions, because both have happened. A dist target with no CI build breaks at release
//! time, in the one job where a failure is most expensive. A CI build for a target the release
//! does not ship is runner-minutes spent proving nothing.

use std::path::PathBuf;

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// Pull the quoted target triples out of the `targets = [...]` line.
fn dist_targets() -> Vec<String> {
    let dist = read("dist-workspace.toml");
    let line = dist
        .lines()
        .find(|l| l.trim_start().starts_with("targets"))
        .unwrap_or_else(|| panic!("dist-workspace.toml has no targets line:\n{dist}"));
    line.split('[')
        .nth(1)
        .expect("a bracketed list")
        .split(']')
        .next()
        .expect("a closing bracket")
        .split(',')
        .map(|t| t.trim().trim_matches('"').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// The target triple a runner builds when the workflow does not pass `--target`.
///
/// A job on `windows-latest` produces `x86_64-pc-windows-msvc` without the triple ever appearing
/// in the file, so a parser that only reads explicit `--target` lines concludes Windows is not
/// built. It is. These four lines are the whole mapping, and they are stated rather than inferred
/// from runner names by clever string handling.
const RUNNER_TARGETS: &[(&str, &str)] = &[
    ("windows-", "x86_64-pc-windows-msvc"),
    ("macos-15-intel", "x86_64-apple-darwin"),
    ("macos-", "aarch64-apple-darwin"),
    ("ubuntu-24.04-arm", "aarch64-unknown-linux-gnu"),
    ("ubuntu-", "x86_64-unknown-linux-gnu"),
];

/// Every target triple CI builds, wherever and however it is built.
///
/// Three places: a matrix row (`- target: <triple>`), an explicit cross-compile
/// (`--target <triple>`), and a job that simply runs on a platform, where the runner's own
/// architecture is the target.
fn ci_targets() -> Vec<String> {
    let ci = read(".github/workflows/ci.yml");
    let mut out: Vec<String> = Vec::new();

    for line in ci.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("- target: ") {
            out.push(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = t.split("--target ").nth(1) {
            let triple = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches('"')
                .trim_end_matches('$')
                .trim();
            // `${{ matrix.target }}` is an expression, not a triple: the matrix rows above it
            // already contributed every value it can take, so counting it again would invent a
            // target that does not exist.
            if !triple.is_empty() && !triple.starts_with("${{") {
                out.push(triple.to_string());
            }
        } else if let Some(runner) = t.strip_prefix("runs-on: ") {
            let runner = runner.trim().trim_matches('"');
            for (prefix, triple) in RUNNER_TARGETS {
                if runner.starts_with(prefix) {
                    out.push((*triple).to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn the_two_lists_are_not_empty() {
    // Every assertion below is vacuous against an empty list, which is the failure this file
    // exists to prevent.
    assert!(
        dist_targets().len() >= 5,
        "only {} dist targets were parsed; the extraction has broken",
        dist_targets().len()
    );
    assert!(
        ci_targets().len() >= 3,
        "only {} CI targets were parsed; the extraction has broken",
        ci_targets().len()
    );
}

#[test]
fn every_target_the_release_ships_is_built_in_ci() {
    let shipped = dist_targets();
    let built = ci_targets();
    let missing: Vec<&String> = shipped.iter().filter(|t| !built.contains(t)).collect();
    assert!(
        missing.is_empty(),
        "dist-workspace.toml ships {} target(s) that no CI job builds: {missing:?}.\n\
         A release target nothing compiles is a target that breaks at release time.",
        missing.len()
    );
}

#[test]
fn every_target_ci_builds_is_one_the_release_ships() {
    let shipped = dist_targets();
    let built = ci_targets();
    let extra: Vec<String> = built
        .iter()
        .filter(|t| !shipped.contains(t))
        .cloned()
        .collect();
    assert!(
        extra.is_empty(),
        "CI builds {} target(s) the release does not ship: {extra:?}.\n\
         That is runner-minutes spent proving nothing.",
        extra.len()
    );
}

#[test]
fn the_supported_platforms_are_the_documented_ones() {
    // The row asks for Linux x64/arm64, macOS x64/arm64 and Windows x64, with unsupported
    // combinations documented rather than silently broken.
    let shipped = dist_targets();
    for required in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        assert!(
            shipped.iter().any(|t| t == required),
            "{required} is not a release target; the supported-platform list and the release \
             disagree. shipped: {shipped:?}"
        );
    }
}

#[test]
fn windows_has_its_own_smoke_job() {
    // A Windows binary that is built and never started is not a supported platform.
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("windows:") || ci.contains("runs-on: windows"),
        "no Windows runner anywhere in CI: a Windows target that is built but never started is \
         not a supported platform"
    );
    assert!(
        ci.contains("shell: pwsh"),
        "the Windows job does not use PowerShell, so it is not exercising what a Windows user \
         runs"
    );
}
