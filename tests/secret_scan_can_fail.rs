//! The G5 secret scan has to be able to fail.
//!
//! It reported PASS while a **live NVIDIA API key** sat in `src/cli/doctor.rs`
//! — committed as two adjacent string literals, so no regex over raw bytes
//! could see it, under a pattern list that did not mention `nvapi` at all.
//! Every other row of that corpus used an obviously fake value; this one was
//! real.
//!
//! So this file asserts the scanner finds the exact thing that slipped past it.
//! A scan that has only ever been clean is not known to be a scan.
//!
//! The scanner is invoked as a subprocess rather than linked, because it is a
//! Python script that walks the git index — and because a test that imported
//! the thing it is testing would not be testing it.

use std::io::Write;
use std::process::{Command, Stdio};

fn scan_stdin(blob: &str) -> (bool, String) {
    let mut child = Command::new("python3")
        .arg("scripts/scan-secrets.py")
        .arg("--stdin")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("scanner starts");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(blob.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("scanner finishes");
    (
        !out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

/// The exact shape that got through: one key, split across two adjacent
/// string literals, concatenated at runtime. Neither half is long enough to
/// match on its own.
#[test]
fn a_credential_split_across_adjacent_literals_is_found() {
    let blob = r#"
fn sample() -> String {
    ["nvapi-1A2b3C4d5E6f7G8h9I0j1K2l3M4n5O6p7Q8r9S0t",
     "1U2v3W4x5Y6z7A8b9C0d1E2f3G4h5I6j7K8l9M0n1O2p3"].concat()
}
"#;

    let (found, output) = scan_stdin(blob);
    assert!(
        found,
        "a credential split across two literals was NOT found. This is the \
         exact shape that reached master. Output was:\n{output}"
    );
    assert!(
        output.contains("as spliced text"),
        "the finding should say it came from the spliced pass, so the \
         mechanism is visible when this ever fires again:\n{output}"
    );
}

/// The control: the split form is found by the spliced pass specifically, so
/// the test above is not just detecting the presence of the word `nvapi`.
#[test]
fn a_single_literal_credential_is_found_by_the_raw_pass() {
    let blob = r#"
const KEY: &str = "nvapi-1A2b3C4d5E6f7G8h9I0j1K2l3M4n5O6p7Q8r9S0t";
"#;

    let (found, output) = scan_stdin(blob);
    assert!(found, "a plain credential was NOT found:\n{output}");
    assert!(
        !output.contains("as spliced text"),
        "a whole-line credential should be caught by the raw pass, not only \
         the spliced one:\n{output}"
    );
}

/// `nvapi` was missing from the pattern list entirely — a whole provider
/// this project supports had no coverage at all.
#[test]
fn nvidia_keys_are_covered() {
    for blob in [
        r#"let k = "nvapi-1A2b3C4d5E6f7G8h9I0j1K2l3M4n5O6p";"#,
        r#"let k = "sk-ant-<key-shaped-token>";"#,
        r#"let k = "ghp_012345678901234567890123456789abcdef";"#,
        r#"let k = "AKIAIOSFODNN7EXAMPLE";"#,
        r#"let k = "AIzaSyA0123456789012345678901234567890A";"#,
    ] {
        let (found, output) = scan_stdin(blob);
        assert!(found, "not covered:\n{blob}\nscanner said:\n{output}");
    }
}

/// The opposite direction. A gate nobody can keep green gets switched off, so
/// ordinary code must not trip it — this asserts the scanner is not simply
/// reporting everything.
#[test]
fn ordinary_code_does_not_trip_the_scan() {
    let blob = r#"
// A normal Rust file.
fn add(a: i32, b: i32) -> i32 { a + b }
const RETRY: u32 = 3;
let url = "https://api.example.com/v1/chat/completions";
let hash = "d41d8cd98f00b204e9800998ecf8427e";
// sk- is far too short to be a key.
let short = "sk-12345";
"#;

    let (found, output) = scan_stdin(blob);
    assert!(
        !found,
        "ordinary source was flagged, so the gate would get switched off:\n{output}"
    );
}

/// The allowlist is the scanner's one soft spot, so it is pinned. Adding an
/// entry to it has to fail this test on purpose, which means somebody reads
/// the reason first.
#[test]
fn the_allowlist_is_exactly_one_file_with_a_reason() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/scan-secrets.py"),
    )
    .expect("the scanner source is readable");

    let start = source
        .find("ALLOWED_FILES")
        .expect("the scanner declares ALLOWED_FILES");
    let body = &source[start..];
    let end = body.find("\n}").expect("the dict is terminated");
    let body = &body[..end];

    let entries: Vec<&str> = body
        .lines()
        .filter(|l| l.trim_start().starts_with('"') && l.contains(".rs"))
        .collect();

    assert_eq!(
        entries.len(),
        1,
        "the allowlist grew to {entries:?}. Every entry is a file the scanner \
         can no longer check, so each one has to be argued for deliberately."
    );
    assert!(
        body.contains("src/cli/doctor.rs"),
        "the one allowed file should still be the redaction corpus: {body}"
    );
    assert!(
        body.matches("redaction_corpus").count() >= 1,
        "the allowlist entry lost its reason: {body}"
    );
}
