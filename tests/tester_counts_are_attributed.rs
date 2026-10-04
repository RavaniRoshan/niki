//! The Tester's test counts are the model's claim; NIKI's verification is the
//! measured one. They must not be presented as the same kind of fact.
//!
//! This file exists because of a real run against
//! `nvidia/nemotron-3-super-120b-a12b`. The Tester reported
//! `{passed: 4, total: 4}` and named `test_calc_add.py` as the file it had
//! written. It had written no file. The suite executed `1 passed`. NIKI
//! printed:
//!
//! ```text
//! 4/4 tests passed — 3 edge cases identified
//! ```
//!
//! Nothing unsafe shipped — the branch was gated on the real exit code — but
//! the number a user reads was the model's, stated as fact, with the model's
//! fiction attached to it. The Reviewer was fooled by the same claim and
//! reviewed a test file that did not exist.

use niki::agents::tester::{TestExecution, VerificationStatus};
use niki::artifacts::types::TestReport;
use niki::display::artifact_render::{render_test_report_summary, render_verification_line};

fn report(passed: u32, total: u32) -> TestReport {
    let json = format!(
        r#"{{
            "test_results": {{"passed": {passed}, "failed": 0, "errors": 0, "skipped": 0, "total": {total}}},
            "tests_written": [
                {{"name": "test_add_positive", "file_path": "test_calc_add.py",
                  "description": "add() returns the sum", "oracle_source": "spec",
                  "spec_reference": "The spec", "error_message": null,
                  "status": "passed"}}
            ],
            "edge_cases_found": ["Large integers", "Floats", "Large negatives"],
            "tester_notes": "All new tests pass."
        }}"#
    );
    serde_json::from_str(&json).expect("TestReport shape")
}

fn execution(exit_code: i64, stdout: &str) -> TestExecution {
    TestExecution {
        command: "python3 -m pytest -q".into(),
        exit_code,
        passed: exit_code == 0,
        // The recorded verdict, alongside `passed`. Mechanical: the fixture already
        // derives `passed` from `exit_code`, so the status follows from the same input.
        status: if exit_code == 0 {
            VerificationStatus::Passed
        } else {
            VerificationStatus::Failed
        },
        stdout: stdout.into(),
        stderr: String::new(),
        truncated: false,
        note: None,
        mutation: None,
    }
}

/// The claim must be attributed. A bare `4/4 tests passed` is the exact string
/// that shipped a fiction, and this fails if the attribution is dropped.
#[test]
fn the_tester_line_attributes_the_counts() {
    let line = render_test_report_summary(&report(4, 4)).join(" ");

    assert!(
        line.contains("Tester reported"),
        "the counts read as a measurement: {line}"
    );
    assert!(
        line.starts_with("Tester reported 4/4"),
        "the model's own numbers should still be shown, attributed: {line}"
    );
}

/// The specific false claim, pinned. `1 passed` is what actually ran.
#[test]
fn a_claim_the_suite_contradicts_is_not_presented_as_the_result() {
    let te = execution(0, ".  [100%]\n1 passed in 0.01s\n");
    let rendered = render_test_report_summary(&report(4, 4))
        .into_iter()
        .chain(render_verification_line(Some(&te)))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered.contains("1 passed") || rendered.contains("exited 0"),
        "the measured result is missing entirely:\n{rendered}"
    );
    // Nothing anywhere may present 4/4 as NIKI's own measurement.
    assert!(
        !rendered.contains("\n4/4 tests passed") && !rendered.starts_with("4/4 tests passed"),
        "the claim is still stated as fact:\n{rendered}"
    );
}

/// The measured line reports an exit code, not a pass count. Counting would
/// mean parsing pytest/cargo/jest output, and a parser that guesses is worse
/// than no number — so the contract is that NIKI states what it ran and what
/// it got back.
#[test]
fn the_measured_line_reports_the_exit_code_not_a_guessed_count() {
    let te = execution(0, ".  [100%]\n1 passed in 0.01s\n");
    let line = render_verification_line(Some(&te)).expect("a line");

    assert!(line.contains("python3 -m pytest -q"), "{line}");
    assert!(line.contains("exited 0"), "{line}");
    assert!(
        !line.contains("1/1") && !line.contains("4/4"),
        "the measured line must not invent a per-test count: {line}"
    );
}

/// A failing suite must say so in the measured line, not only in the gate
/// decision further down.
#[test]
fn a_failing_suite_is_stated_as_failing() {
    let te = execution(1, "F");
    let line = render_verification_line(Some(&te)).expect("a line");

    assert!(line.contains("exited 1"), "{line}");
    assert!(line.contains("failed"), "{line}");
}

/// No suite ran — nothing was measured, so there is no measured line. This is
/// the case where the Tester's attributed claim stands alone, and it must not
/// acquire a fake verification behind it.
#[test]
fn no_execution_means_no_measured_line() {
    assert!(
        render_verification_line(None).is_none(),
        "a verification line with no execution behind it is a fabricated result"
    );
}
