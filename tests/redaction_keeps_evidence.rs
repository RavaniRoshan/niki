//! Redaction must not destroy the evidence it sits next to.
//!
//! The catch-all pattern was `[A-Za-z0-9+/]{40,}={0,2}` — "any long unbroken
//! alphanumeric run". Measured against real text, that is a list of things
//! which are not secrets:
//!
//! ```text
//! git sha (40 hex)     REDACTED
//! sha256 (64 hex)       REDACTED
//! long word             REDACTED
//! base64 asset path     REDACTED
//! minified js chunk     REDACTED
//! ```
//!
//! **A git SHA is the worst of them.** `report.md`, `trace.jsonl` and the TUI
//! all reference commits, and a redacted one is an unreferenceable piece of
//! evidence in the middle of a report — the same failure as the empty
//! artifacts in B2-01: the file is intact and useless.
//!
//! What separates an encoded secret from an identifier is **shape, not
//! length**. A base64 rendering of a random secret mixes upper case, lower
//! case and digits; a SHA is lower-case hex; a word is lower-case letters. The
//! run must therefore contain an uppercase letter *and* a digit.
//!
//! Narrowing this is only safe if the things it used to catch are still
//! caught. That is not a claim this file makes — it checks the **real** corpus,
//! which `tests/secret_redaction.rs` owns and which runs on every build. A
//! narrower pattern that catches less is not a fix.

use niki::llm::provider::redact_secrets;

/// Identifiers must survive. This is the whole slice.
#[test]
fn identifiers_are_not_secrets() {
    let long_a = "a".repeat(80);
    for (name, value) in [
        (
            "git sha, 40 hex",
            "9f8e7d6c5b4a39281706f5e4d3c2b1a0987654321",
        ),
        (
            "sha256, 64 hex",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "a long word",
            "supercalifragilisticexpialidociousness1234567890",
        ),
        (
            "a long lowercase identifier",
            "thisisareallylonglowercasefreeidentifiername",
        ),
        ("a run of one character", long_a.as_str()),
    ] {
        let out = redact_secrets(value);
        assert_eq!(
            out, value,
            "{name} was redacted. A commit reference that reads [REDACTED] is \\
             an unreferenceable piece of evidence, and the report around it is \\
             intact and useless."
        );
    }
}

/// And this must keep catching. A narrower pattern that catches less is not a
/// fix.
#[test]
fn encoded_secrets_are_still_caught() {
    for (name, secret) in [
        (
            "base64 of arbitrary bytes",
            "aGVsbG8gd29ybGQgdGhpcyBpcyBhIHRlc3Qgc3RyaW5nIQ",
        ),
        (
            "a high-entropy token",
            "x7Kd93mfPq2ZrT8vB1nL5wY0cJ6hG4sA9dF2uE7iO3pQ",
        ),
    ] {
        let out = redact_secrets(secret);
        assert_ne!(out, secret, "{name} survived redaction: {out}");
        assert!(
            out.contains("[REDACTED]"),
            "{name} must be replaced, not altered: {out}"
        );
    }
}

/// The rule is *shape*, and both edges of the boundary are worth pinning: no
/// uppercase is not an encoded secret, and no digit is not either.
#[test]
fn the_rule_is_shape_not_length() {
    let lower = "x7kd93mfpq2zrt8vB1nL5wY0cJ6hG4sA9dF2uE7iO3p".to_lowercase();
    let lower = format!("{lower}{lower}");
    assert_eq!(
        redact_secrets(&lower),
        lower,
        "a long lower-case run has no upper case, so it is not an encoded \\
         secret and must survive"
    );

    let no_digits = "XKJD".repeat(20);
    assert_eq!(
        redact_secrets(&no_digits),
        no_digits,
        "a long run with no digit is not an encoded secret and must survive"
    );

    let both = format!("{}Ab9", "XKJD7".repeat(16));
    assert!(
        redact_secrets(&both).contains("[REDACTED]"),
        "a long run mixing upper case and digits is the shape of an encoded \\
         secret and must be caught: {}",
        &both[..40]
    );
}

/// Redaction runs over provider error bodies, and a body is where a *real*
/// secret turns up alongside the noise. The catch-all exists for exactly that,
/// so the narrowing must not lose it.
#[test]
fn a_real_key_inside_a_provider_error_body_is_still_caught() {
    let body = "{\"error\":{\"message\":\"invalid request\",\"echo\":\"\
                VGhpc0lzQU5PcGFxdWFFVGVzdFRva2VuMTIzNDU2Nzg5MGFiY2RlZmdoaWprbG1ub3A=\",\
                \"hint\":\"retry\"}}";
    let out = redact_secrets(body);
    assert!(
        out.contains("[REDACTED]"),
        "a base64 token inside a JSON error body must be caught: {out}"
    );
    assert!(
        !out.contains("VGhpc0lzQU5P"),
        "and the token itself must be gone: {out}"
    );
    assert!(
        out.contains("\"message\"") && out.contains("retry"),
        "redaction must not destroy the rest of the body, or the error stops \\
         being readable: {out}"
    );
}
