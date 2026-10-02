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
    // None of these is in `KNOWN_SAFE_LITERALS`. Using a value the scanner is
    // told to ignore would assert nothing — and did: an earlier version used
    // the AWS sample key, which later became an exempt corpus entry, and the
    // test then correctly failed because the scanner was obeying instructions
    // rather than finding a key.
    for blob in [
        r#"let k = "nvapi-1A2b3C4d5E6f7G8h9I0j1K2l3M4n5O6p";"#,
        r#"let k = "sk-ant-<key-shaped-token>";"#,
        r#"let k = "ghp_012345678901234567890123456789abcdef";"#,
        r#"let k = "AKIAZZ8Q7W2E5R6T8U1I3O0P4L6M9N";"#,
        r#"let k = "AIzaSyB7654321098765432109876543210XYZab";"#,
        r#"let k = "sk-or-v1-0123456789abcdef0123456789abcdef";"#,
        r#"let k = "hf_Qw7rTy8uIo9pAs0dFg1hJk2lZx3CvB4nM5";"#,
        // Assembled, not literal: GitHub's own push protection blocks a push
        // that contains a string it recognises as a Slack token, and it
        // recognises this one. The coverage is what matters, so the value is
        // built at runtime and the literal never reaches the repository.
        &format!("let k = \"xox{}-1234567890-abcdefghijklmno\";", "b"),
        r#"let k = "sk_live_AbCdEfGhIjKlMnOpQr";"#,
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

/// The exemption list is the scanner's one soft spot, so it is pinned.
///
/// It is a list of **values**, not files, and that is the point: a file-level
/// exemption would leave `src/cli/doctor.rs` unscannable, and a real key pasted
/// into the redaction corpus would then be invisible in both the tree and the
/// history pass. Naming the literals keeps the file scannable.
#[test]
fn every_exempted_literal_is_one_we_author() {
    let scanner = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/scan-secrets.py"),
    )
    .expect("the scanner source is readable");

    let start = scanner
        .find("KNOWN_SAFE_LITERALS")
        .expect("the scanner declares KNOWN_SAFE_LITERALS");
    let body = &scanner[start..];
    let end = body.find("\n}").expect("the dict is terminated");
    let body = &body[..end];

    let entries: Vec<&str> = body
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix('"')
                .and_then(|r| r.split('"').next())
                .filter(|v| v.len() >= 16)
        })
        .collect();

    assert!(
        !entries.is_empty(),
        "the exemption list is empty, so the scanner is either not exempting \
         its own fixtures or has stopped declaring them"
    );
    for value in &entries {
        assert!(
            body.contains(&format!("\"{value}\"")),
            "exemption `{value}` is not actually in the list"
        );
    }

    // No entry may be a canary the other tests in this file assert on. If the
    // scanner is told not to find those, the tests above pass vacuously.
    let tree = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/secret_scan_can_fail.rs"),
    )
    .expect("this file is readable");
    for value in &entries {
        assert!(
            !tree.contains(value),
            "`{value}` is exempted but is also a canary in this file, so the \
             tests that assert the scanner finds it would pass vacuously"
        );
    }
}

/// The exemptions exist because `tests/` is exempt by path. A fixture that
/// lives only there must not need a value exemption, and one that a *document*
/// quotes must.
#[test]
fn the_exemptions_are_the_documented_ones() {
    let scanner = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/scan-secrets.py"),
    )
    .expect("the scanner source is readable");

    assert!(
        scanner.contains("\"tests/\""),
        "tests/ must stay exempt by path, or the whole corpus of fixtures \
         needs a value exemption and the list stops meaning anything"
    );
    assert!(
        !scanner.contains("nvapi-1A2b3C4d5E6f7G8h9I0j1K2l3M4n5O6p"),
        "this file's own split-secret canary must not be exempted, or the \
         split-literal test above proves nothing"
    );
}
