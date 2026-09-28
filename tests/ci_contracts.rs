//! CI workflow contracts: the properties of `.github/workflows/ci.yml` that a
//! YAML parse cannot tell you and a green run cannot prove.
//!
//! Every assertion here is about a check that can pass while proving nothing,
//! or a job graph that can be reshaped into something slower or weaker without
//! anything failing. A workflow file is configuration; a test is the only thing
//! that notices when the configuration stops meaning what its comment says.

use std::path::Path;
use std::collections::BTreeMap;

fn ci_yml() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml"))
        .expect(".github/workflows/ci.yml")
}

/// The headless-TUI vacuity check must not be anchored at the start of a line.
///
/// `pytest -v` wraps its summary in a rule:
///
///     ======================== 17 passed, 2 skipped in 11.84s ====================
///
/// while `pytest -q` puts it on its own line. This step was rewritten to run
/// the suite ONCE with `-v` and `tee` the output (it used to run it twice per
/// job, and the suite is timing-sensitive, so the second run doubled the load
/// on the one job whose failures are hardest to distinguish from flakiness).
/// The vacuity grep kept the `-q` form of the pattern — `^[0-9]+ passed` — and
/// therefore matched nothing: the job would have failed on a green suite with
/// 17 passing tests.
///
/// The failure mode is silent in the worst way: it is not a false pass, it is a
/// false *fail* that looks like "the TUI broke", and the fix people reach for
/// is to delete the check. So the regex is pinned here, against a sample of
/// real output from both invocations.
#[test]
fn the_tui_vacuity_check_matches_both_pytest_output_formats() {
    let ci = ci_yml();
    assert!(
        ci.contains(r#"grep -qE '[0-9]+ passed'"#),
        "ci.yml's headless-TUI vacuity check must use the UNANCHORED pattern \
         `[0-9]+ passed`. `pytest -v` wraps its summary in a `====` rule, so an \
         anchored `^[0-9]+ passed` matches nothing and fails a green suite."
    );
    assert!(
        !ci.contains(r#"grep -qE '^[0-9]+ passed'"#),
        "ci.yml's headless-TUI vacuity check is anchored with `^` — see this test's doc comment"
    );

    // The pattern has to work on the two real shapes, not just look right.
    let anchored = |s: &str| {
        s.lines()
            .any(|l| l.starts_with(|c: char| c.is_ascii_digit()) && l.contains(" passed"))
    };
    let unanchored = |s: &str| s.contains(" passed");

    let quiet = ".......ss..........                    [100%]\n17 passed, 2 skipped in 11.56s\n";
    let verbose = "tests/headless_tui.py::test_chat PASSED [  5%]\n\
                   ======================== 17 passed, 2 skipped in 11.84s ========================\n";

    assert!(anchored(quiet), "sanity: the anchored form matches -q output");
    assert!(
        !anchored(verbose),
        "sanity: this is the bug — the anchored form does NOT match -v output"
    );
    assert!(unanchored(quiet) && unanchored(verbose), "the unanchored form matches both");
}

/// The headless-TUI suite must run once per job, not twice.
///
/// It used to run once with `-v` for the log and again with `-q` purely to grep
/// the second run's output. That is two full suites per job, four jobs, on the
/// one suite that fails when the machine is loaded. Counting the invocations
/// keeps the "read the same run" property from being undone by an edit.
#[test]
fn the_headless_tui_suite_runs_once_per_job() {
    let ci = ci_yml();
    for (job_name, tee_path) in [("tui-headless", "/tmp/tui.txt"), ("tui-pty", "/tmp/pty.txt")] {
        let job = ci
            .split(&format!("\n  {job_name}:"))
            .nth(1)
            .and_then(|rest| rest.split("\n  # ──").next())
            .unwrap_or_else(|| panic!("job {job_name} exists in ci.yml"));

        // Two lines in the single invocation (one per colour branch) is correct;
        // what must not happen is a *second step* invoking the suite again.
        let invocations = job
            .lines()
            .filter(|l| l.contains("python3 -m pytest -c pytest_headless.ini"))
            .count();
        assert!(
            invocations <= 2,
            "job `{job_name}` invokes the pytest suite {invocations} times; it must run once per \
             branch and the vacuity check must read that same run's output. The suite is \
             timing-sensitive, so a second run doubles the load on the one job whose failures \
             are hardest to tell from flakiness."
        );
        assert!(
            job.contains(&format!("tee {tee_path}")),
            "job `{job_name}`'s invocation must tee to {tee_path}, so the vacuity check reads \
             the same run rather than starting another"
        );
        assert_eq!(
            job.matches("python3 -m pytest -c pytest_headless.ini -q").count(),
            0,
            "job `{job_name}` still has a second `-q` invocation of the suite; that run exists \
             only to be grepped"
        );
    }
}

/// Every job that a comment claims consumes no test output must actually not
/// declare `needs: test`.
///
/// Four jobs (`build-matrix`, `e2e-mock`, `visual`, `product-verify`) were
/// changed from `needs: test` to `needs: [check]`. Each builds its own release
/// binary and runs its own fixtures, so nothing from the `test` job reaches
/// them; gating them behind it put a 7-minute job in front of work that did not
/// depend on it, and for `build-matrix` — whose artifact feeds
/// `validate-intel-mac` — that chain was the critical path.
///
/// The assertion is deliberately on the *pair*: a job may not both claim the
/// relaxation in a comment and still declare the dependency.
#[test]
fn jobs_that_consume_no_test_output_do_not_depend_on_it() {
    let ci = ci_yml();
    for job in ["build-matrix", "e2e-mock", "visual", "product-verify"] {
        let body = ci
            .split(&format!("\n  {job}:"))
            .nth(1)
            .and_then(|rest| rest.split("\n  # ──").next())
            .unwrap_or_else(|| panic!("job {job} exists in ci.yml"));

        // Parse the `needs:` *line*, not the prose. A substring search for
        // "needs: test" matches the job's own comment explaining why it used to
        // depend on `test` — which is exactly what happened the first time this
        // test was written, and it made the assertion pass for the wrong reason.
        let needs_line = body
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("needs:"))
            .unwrap_or_else(|| panic!("job `{job}` declares no needs"));

        assert_eq!(
            needs_line, "needs: [check]",
            "job `{job}` should be gated by `check` alone: it builds its own release binary and \
             consumes nothing from the `test` job, so gating it on `test` put 7 minutes in front \
             of work that did not depend on it. Found: {needs_line:?}"
        );
    }
}

/// `product-verify.sh` must not re-run the whole test suite in CI.
///
/// It did — `cargo fmt`, `cargo clippy --all-targets`, and `cargo test` — behind
/// `needs: test`, so the entire suite ran twice per workflow, and that job was
/// the 14-minute critical path. The static and suite layers are now opt-in via
/// `NIKI_VERIFY_FULL=1`, which is how you run the script locally, where being
/// the only gate is the point.
#[test]
fn product_verify_does_not_rerun_the_suite_in_ci() {
    let script = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/product-verify.sh"))
        .expect("scripts/product-verify.sh");

    assert!(
        script.contains("NIKI_VERIFY_FULL"),
        "product-verify.sh must gate its static/suite layers behind NIKI_VERIFY_FULL so CI can \
         skip them; without the flag there is no way to run the product checks without paying \
         for a duplicate suite"
    );
    // The full path must still exist — a fix that deleted the layers would pass
    // the assertion above and take the local gate with it.
    assert!(
        script.contains("cargo clippy --all-targets"),
        "product-verify.sh must still run the full static gate under NIKI_VERIFY_FULL=1"
    );
    assert!(
        script.contains("cargo test --quiet"),
        "product-verify.sh must still run the full suite under NIKI_VERIFY_FULL=1"
    );
}

/// The vacuity gate must actually fail, and it must fail on the cases it exists
/// for. This *runs* `scripts/require-layers.sh` rather than grepping for its
/// name, because the previous version of this test only checked that the string
/// `MUST_HAVE_RUN` appeared in product-verify.sh — and it passed unchanged when
/// the line that actually records a passing layer was deleted, taking the whole
/// guard with it. A check that cannot be executed is a comment.
#[test]
fn the_vacuity_gate_fails_when_a_mandatory_layer_did_not_run() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("scripts/require-layers.sh");
    assert!(script.exists(), "scripts/require-layers.sh must exist");

    let dir = std::env::temp_dir().join(format!("niki-vacuity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let run = |ledger: &str, layers: &[&str]| -> bool {
        let path = dir.join(ledger);
        let out = std::process::Command::new(&script)
            .arg(&path)
            .args(layers)
            .output()
            .expect("require-layers.sh runs");
        out.status.success()
    };

    // Every required layer recorded: the gate passes.
    std::fs::write(dir.join("all"), "CLI smoke\nTUI PTY smoke\nAgent workflow E2E\n").unwrap();
    assert!(
        run("all", &["CLI smoke", "TUI PTY smoke", "Agent workflow E2E"]),
        "with every layer recorded the gate must pass"
    );

    // One required layer never recorded (the exact bug: a runner script absent,
    // so its `else` branch called skip_check) while the rest passed.
    assert!(
        !run("all", &["CLI smoke", "TUI PTY smoke", "Agent workflow E2E", "Visual regression"]),
        "a layer that did not run must fail the gate, whatever else passed"
    );

    // Requiring only a subset of what was recorded is a pass — the gate asks
    // whether the *required* layers ran, not whether everything ran.
    assert!(
        run("all", &["CLI smoke", "TUI PTY smoke"]),
        "requiring a subset of recorded layers must pass"
    );

    // Nothing ran at all.
    std::fs::write(dir.join("none"), "").unwrap();
    assert!(!run("none", &["CLI smoke"]), "an empty ledger must fail the gate");

    // The ledger itself is gone — the recording was removed.
    assert!(
        !run("does-not-exist", &["CLI smoke"]),
        "a missing ledger must fail the gate, not pass vacuously"
    );

    // A near-miss is not a match: "CLI smoke test" is not "CLI smoke".
    std::fs::write(dir.join("near"), "CLI smoke test\n").unwrap();
    assert!(
        !run("near", &["CLI smoke"]),
        "the gate must match whole lines; a substring is not a pass"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `product-verify.sh` must actually call the gate, and must still keep the
/// full local path.
///
/// Both halves matter: wiring the gate in without the ledger append would be a
/// gate that always fails, and deleting the static layers outright would take
/// the local "run one thing and trust it" workflow with them.
#[test]
fn product_verify_wires_the_gate_and_keeps_the_full_path() {
    let script = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/product-verify.sh"))
        .expect("scripts/product-verify.sh");

    assert!(
        script.contains("require-layers.sh"),
        "product-verify.sh must call scripts/require-layers.sh, not reimplement the check inline"
    );
    assert!(
        script.contains(r#"echo "$1" >>"$PASSED_LEDGER""#),
        "product-verify.sh must record every passing layer into the ledger — without this the \
         gate can only ever fail"
    );
    assert!(
        script.contains("cargo clippy --all-targets"),
        "product-verify.sh must still run the full static gate under NIKI_VERIFY_FULL=1"
    );
    assert!(
        script.contains("cargo test --quiet"),
        "product-verify.sh must still run the full suite under NIKI_VERIFY_FULL=1"
    );
}

/// The workflow must still trigger on pull requests, at the workflow level.
///
/// A job-level `workflow_dispatch:` was added by mistake once. PyYAML accepted
/// it; GitHub rejected the entire file, so the workflow produced *no jobs* and
/// the `pull_request` run stopped being created while four other workflows
/// stayed green — a silently disarmed CI, discovered much later. The assertion
/// is that the top-level `on:` block carries the three triggers.
#[test]
fn ci_triggers_are_declared_at_the_workflow_level() {
    let ci = ci_yml();
    let on_block = ci
        .split("\non:")
        .nth(1)
        .and_then(|rest| rest.split("\njobs:").next())
        .expect("ci.yml has a top-level `on:` block");

    for trigger in ["workflow_dispatch", "push", "pull_request"] {
        assert!(
            on_block.contains(trigger),
            "ci.yml's top-level `on:` must include `{trigger}`"
        );
    }
}

/// Every job the workflow declares must be reachable — no orphan that can never
/// run because a `needs:` names a job that does not exist.
///
/// GitHub silently skips a job whose dependency is unsatisfiable, and the run
/// still reports success. A typo in a `needs:` is therefore invisible.
#[test]
fn every_needs_target_exists() {
    let ci = ci_yml();
    let jobs_block = ci
        .split("\njobs:")
        .nth(1)
        .expect("ci.yml has a jobs block");

    let mut job_names: Vec<String> = Vec::new();
    let mut needs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in jobs_block.lines() {
        if let Some(rest) = line.strip_prefix("  ").filter(|l| !l.starts_with("   ")) {
            if let Some(name) = rest.strip_suffix(':') {
                if !name.contains(' ') {
                    current = Some(name.to_string());
                    job_names.push(name.to_string());
                    continue;
                }
            }
        }
        if let (Some(job), Some(rest)) = (current.as_ref(), line.trim().strip_prefix("needs:")) {
            let targets = rest
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
            needs.insert(job.clone(), targets);
        }
    }

    assert!(job_names.len() >= 15, "parsed only {} jobs — the parser drifted", job_names.len());
    for (job, targets) in &needs {
        for t in targets {
            assert!(
                job_names.contains(t),
                "job `{job}` needs `{t}`, which is not a job in this workflow. GitHub skips a \
                 job whose dependency cannot be satisfied and the run still reports success."
            );
        }
    }
}
