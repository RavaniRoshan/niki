//! `niki doctor` must not report a security property it never measured.
//!
//! The security section of `niki doctor` used to contain:
//!
//! ```rust
//! CheckResult::Pass("always-on: provider keys redacted from logs, reports,
//!                    artifacts (provider.rs)".to_string())
//! ```
//!
//! A constant. No corpus, no assertion, nothing that could return `Fail` — and
//! two of the shapes it claimed to cover were leaking:
//!
//! - a Hugging Face token (`hf_…`), which the 40-char base64 catch-all can
//!   never reach because the prefix is two characters plus an underscore; and
//! - **any key in a JSON body** (`{"api_key": "…"}`), because the field
//!   patterns required an `=`. That is the shape a provider error arrives in,
//!   and provider error bodies are the one place `redact_secrets` is applied.
//!
//! The check is now the same corpus the tests assert, so it cannot pass for a
//! reason the tests would not also catch. The second half of the file is the
//! other half of the job: a redactor that eats the report it is protecting
//! produces a report with `[REDACTED]` where the evidence was, which is not
//! evidence.

use niki::cli::doctor::{redaction_corpus, redaction_failures};
use niki::llm::provider::redact_secrets;

#[test]
fn every_known_key_shape_is_redacted() {
    let failures = redaction_failures();
    assert!(
        failures.is_empty(),
        "`niki doctor` would now report Fail, and these shapes reach logs and \
         report.md: {}",
        failures.join(", ")
    );
}

#[test]
fn the_corpus_is_not_empty_and_not_hollow() {
    // A check over a corpus of nothing passes. Pin the shapes that were
    // actually leaking, so deleting a row cannot quietly green the check.
    let corpus = redaction_corpus();
    assert!(corpus.len() >= 10, "corpus shrank: {}", corpus.len());
    let names: Vec<&str> = corpus.iter().map(|(n, _, _)| *n).collect();
    for required in ["Hugging Face token", "JSON body", "nested JSON field"] {
        assert!(
            names.contains(&required),
            "the corpus must keep the {required:?} case — it is the one that leaked"
        );
    }
}

#[test]
fn the_json_shape_specifically_is_caught() {
    // The regression that motivated the slice, stated on its own so a future
    // rewrite of the patterns has to confront it.
    let body = r#"{"error":{"message":"invalid api_key","api_key":"Zq8Kw3Lm2Np7Rt4Yu1Ih6"}}"#;
    let out = redact_secrets(body);
    assert!(
        !out.contains("Zq8Kw3Lm2Np7Rt4Yu1Ih6"),
        "a key in a JSON error body survived: {out}"
    );
    assert!(
        out.contains("invalid api_key"),
        "redaction must not eat the surrounding message: {out}"
    );
}

#[test]
fn ordinary_output_survives_redaction() {
    // The other failure mode. A diff line that assigns to a variable called
    // `secret_count` is evidence in `report.md`; blanking it makes the report
    // unreadable where it matters most.
    let cases: &[(&str, &str)] = &[
        ("model = \"gpt-4o-mini\"", "gpt-4o-mini"),
        (
            "+  let secret_count = compute_secret();",
            "compute_secret()",
        ),
        (
            "The plan stores the API key rotation policy.",
            "rotation policy",
        ),
        (r#"{"status":"ok","token_count":1234}"#, "1234"),
        ("test_password_reset_flow()", "password_reset"),
        ("https://example.com/docs/api-keys-guide", "api-keys-guide"),
    ];
    for (input, must_survive) in cases {
        let out = redact_secrets(input);
        assert!(
            out.contains(must_survive),
            "redaction over-reached on {input:?} -> {out:?}; a report with \
             [REDACTED] where the evidence was is not evidence"
        );
    }
}

#[test]
fn the_doctor_check_reads_the_same_corpus_the_tests_do() {
    // If the check ever grows its own private list, it can go green while the
    // redactor regresses — which is precisely the state this slice was
    // written to end.
    let s = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli/doctor.rs"),
    )
    .expect("doctor.rs must be readable");

    assert!(
        !s.contains("always-on: provider keys redacted from logs, reports, artifacts"),
        "the hardcoded Pass is back: a check that cannot fail is not a check"
    );
    assert!(
        s.contains("redaction_failures()"),
        "the check must consult the corpus, not assert a constant"
    );
    assert!(
        s.contains("CheckResult::Fail("),
        "and it must have a Fail arm, or it is still a label"
    );
}
