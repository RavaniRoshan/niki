//! A skipped test must say why, and where it is actually covered.
//!
//! `tests/headless_tui.py` carries two unconditional `pytest.skip`s, and both
//! are honest — the docstrings name the missing capability (a Kitty-protocol
//! terminal; a live LLM) and the path that does cover the behaviour. Marking
//! them skipped rather than faking them is the right call, and it is the call
//! batch 1 made deliberately.
//!
//! What was missing is anything that keeps them honest. A skip whose reason is
//! empty, or that does not say where the behaviour *is* covered, is a permanent
//! silent hole: it looks like a test, passes every run, and covers nothing. So
//! this asserts the property, which is the part that can rot.
//!
//! The Rust side of the same finding is `tests/tui_perf.rs`, whose wall-clock
//! budgets were calibrated on one machine and sat close enough to their
//! measurements that a slower host turned them red. Those are now a printed
//! smoke check plus one machine-independent assertion.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// Every `pytest.skip` must carry a reason, and the reason must be specific.
#[test]
fn every_python_skip_says_why() {
    let src = read("tests/headless_tui.py");
    let skips: Vec<(usize, String)> = src
        .lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with("pytest.skip("))
        .map(|(i, l)| (i + 1, l.to_string()))
        .collect();

    assert!(
        !skips.is_empty(),
        "the skip scan found none — if they were all removed, this test is \
         asserting nothing and should be deleted with them"
    );
    for (line_no, line) in &skips {
        assert!(
            line.contains("\"") && line.matches('"').count() >= 2,
            "tests/headless_tui.py:{line_no} skips with no reason: {line}"
        );
        let reason = line
            .split('"')
            .nth(1)
            .expect("a reason was checked above")
            .to_lowercase();
        assert!(
            reason.len() >= 20,
            "tests/headless_tui.py:{line_no} has a reason too short to \
             diagnose: {reason:?}"
        );
    }
}

/// And the test must say **where the behaviour is covered**, so a skip is a
/// redirect rather than a hole. This is the part that decays: the covering
/// path moves, and nothing notices.
#[test]
fn every_python_skip_names_where_it_is_covered() {
    let src = read("tests/headless_tui.py");
    // A skip is a redirect, so the function above it must say where.
    let all: Vec<&str> = src.lines().collect();
    let mut checked = 0;
    for (i, line) in all.iter().enumerate() {
        if !line.trim_start().starts_with("pytest.skip(") {
            continue;
        }
        // Walk back over the docstring and any decorators to the `def`.
        let mut header = String::new();
        let mut j = i;
        while j > 0 {
            j -= 1;
            header.insert_str(0, &format!("{}\n", src.lines().nth(j).unwrap_or("")));
            if src
                .lines()
                .nth(j)
                .unwrap_or("")
                .trim_start()
                .starts_with("async def")
                || src
                    .lines()
                    .nth(j)
                    .unwrap_or("")
                    .trim_start()
                    .starts_with("def")
            {
                break;
            }
        }
        let lower = header.to_lowercase();
        assert!(
            lower.contains("exercised elsewhere")
                || lower.contains("covered by")
                || lower.contains("unit-test")
                || lower.contains("elsewhere"),
            "the test skipped at line {} does not say where the behaviour is \\
             covered. A skip with no redirect is a permanent silent hole: it \\
             looks like a test and covers nothing. Docstring:\\n{header}",
            i + 1
        );
        checked += 1;
    }
    assert_eq!(
        checked, 2,
        "expected the two known skips; the count moved, so re-read them before \
         trusting this test"
    );
}

/// The Rust perf tests must no longer *fail* on a slow host.
///
/// The absolute budgets stay, printed, as a smoke check — they are useful
/// information in a CI log. What must not happen is a test whose result depends
/// on how fast the machine is. So: a wall-clock assertion may not stand alone
/// in any perf test.
#[test]
fn perf_assertions_are_not_only_wall_clock() {
    let src = read("tests/tui_perf.rs");
    assert!(
        src.contains("fn report_relative_to_baseline("),
        "the machine-independent perf helper is gone. Without it every \\
         assertion in this file is a wall-clock threshold calibrated on one \\
         machine, and a slower host reports a regression that did not happen."
    );
    assert!(
        src.contains("fn perf_is_machine_independent("),
        "and the test that uses it must exist — a helper nothing calls is the \\
         same shape of defect as a tool nothing calls."
    );
    // And the smoke check must not silently become the only check: `report`
    // must not assert on its own budget any more.
    let report = src
        .split("fn report(name: &str")
        .nth(1)
        .and_then(|r| r.split("\n}\n").next())
        .expect("report must exist");
    assert!(
        !report.contains("assert!"),
        "`report` asserts on its wall-clock budget again, so a slow host can \\
         fail the suite. It must print a note instead: {report}"
    );
}

/// And the docstring must not claim a cold/warm distinction the code does not
/// have — the mistake this slice's first version of that test made.
#[test]
fn the_perf_test_does_not_claim_a_cache_it_does_not_use() {
    let src = read("tests/tui_perf.rs");
    assert!(
        !src.contains("let cold"),
        "the test compares a cold and a warm pass, but `render_once` builds a \
         fresh `TestBackend` and `Terminal` on every call, so there is no \
         cache to reuse and the two are the same measurement. The ratio sits \
         at 1.00 and the assertion fails about one run in three."
    );
}
