//! CI workflow contracts: the properties of `.github/workflows/ci.yml` that a
//! YAML parse cannot tell you and a green run cannot prove.
//!
//! Every assertion here is about a check that can pass while proving nothing,
//! or a job graph that can be reshaped into something slower or weaker without
//! anything failing. A workflow file is configuration; a test is the only thing
//! that notices when the configuration stops meaning what its comment says.

use std::collections::BTreeMap;
use std::path::Path;

/// The lines of a job body that are not comments.
///
/// Two of these assertions were, at different times, satisfied by the very
/// comment explaining the bug they guard: one matched `needs: test` in the
/// comment that says why the job no longer needs it, and one matched the old
/// `v0.8.0` inside the note explaining that it was removed. A comment is
/// documentation; a gate that reads documentation is a gate that can be made to
/// pass by writing prose.
fn job_body_without_comments(ci: &str, job: &str) -> String {
    let body = ci
        .split(&format!("\n  {job}:"))
        .nth(1)
        .and_then(|rest| rest.split("\n  # ──").next())
        .unwrap_or_else(|| panic!("job {job} exists in ci.yml"));
    body.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

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

    assert!(
        anchored(quiet),
        "sanity: the anchored form matches -q output"
    );
    assert!(
        !anchored(verbose),
        "sanity: this is the bug — the anchored form does NOT match -v output"
    );
    assert!(
        unanchored(quiet) && unanchored(verbose),
        "the unanchored form matches both"
    );
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
    for (job_name, tee_path) in [
        ("tui-headless", "/tmp/tui.txt"),
        ("tui-pty", "/tmp/pty.txt"),
    ] {
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
            job.matches("python3 -m pytest -c pytest_headless.ini -q")
                .count(),
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
        let body = job_body_without_comments(&ci, job);

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
    let script = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/product-verify.sh"),
    )
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
    std::fs::write(
        dir.join("all"),
        "CLI smoke\nTUI PTY smoke\nAgent workflow E2E\n",
    )
    .unwrap();
    assert!(
        run("all", &["CLI smoke", "TUI PTY smoke", "Agent workflow E2E"]),
        "with every layer recorded the gate must pass"
    );

    // One required layer never recorded (the exact bug: a runner script absent,
    // so its `else` branch called skip_check) while the rest passed.
    assert!(
        !run(
            "all",
            &[
                "CLI smoke",
                "TUI PTY smoke",
                "Agent workflow E2E",
                "Visual regression"
            ]
        ),
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
    assert!(
        !run("none", &["CLI smoke"]),
        "an empty ledger must fail the gate"
    );

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
    let script = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/product-verify.sh"),
    )
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
    let jobs_block = ci.split("\njobs:").nth(1).expect("ci.yml has a jobs block");

    let mut job_names: Vec<String> = Vec::new();
    let mut needs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in jobs_block.lines() {
        if let Some(name) = line
            .strip_prefix("  ")
            .filter(|l| !l.starts_with("   "))
            .and_then(|rest| rest.strip_suffix(':'))
            .filter(|name| !name.contains(' '))
        {
            current = Some(name.to_string());
            job_names.push(name.to_string());
            continue;
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

    assert!(
        job_names.len() >= 15,
        "parsed only {} jobs — the parser drifted",
        job_names.len()
    );
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

/// The demo job must actually assert something about the demo's output.
///
/// `scripts/demo.sh` exits non-zero when it produces no branch, so a job that
/// only runs it already catches the worst case. What it does not catch is a
/// branch with an empty diff — which is exactly what a mock server that stops
/// matching its fixture produces, and it would leave the demo "passing" while
/// handing a visitor a branch with nothing in it. The assertion is on
/// insertions, and it depends on `--keep`, without which the script deletes the
/// scratch project on exit and the check inspects a directory that no longer
/// exists.
#[test]
fn the_demo_job_checks_the_branch_is_not_empty() {
    let ci = ci_yml();
    let body = ci
        .split("\n  demo:")
        .nth(1)
        .and_then(|rest| rest.split("\n  # ──").next())
        .expect("the demo job exists in ci.yml");

    assert!(
        body.contains("./scripts/demo.sh --keep"),
        "the demo job must pass --keep, or the scratch project is deleted before the \
         result can be inspected"
    );
    assert!(
        body.contains("*insertion*"),
        "the demo job must assert the produced branch contains insertions; a branch with an \
         empty diff is what a mock that stopped matching its fixture looks like, and it \
         would pass a bare exit-code check"
    );
    assert!(
        body.contains("%(refname:short)"),
        "the demo job must read the branch with --format='%(refname:short)'; \
         `git branch --list` decorates the checked-out branch with '* ', which puts a \
         literal asterisk into the refspec"
    );
}

/// The demo is the first thing a stranger runs, so the README has to point at it
/// where someone will look — and it has to say plainly which half is real.
#[test]
fn the_readme_documents_the_demo_and_its_limits() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("README.md");

    assert!(
        readme.contains("./scripts/demo.sh"),
        "README.md must tell a first-time visitor how to try the product without a key"
    );
    assert!(
        readme.contains("no API key") || readme.contains("no api key"),
        "the demo's selling point — no key required — must be stated where it is offered"
    );
    assert!(
        readme.contains("canned"),
        "the README must say the demo's model answers are canned. A visitor who assumes \
         otherwise concludes the tool is worse than it is, in the one place they are \
         deciding whether to trust it."
    );
}

/// A CI job that runs a file git does not ship.
///
/// This is not hypothetical. `.gitignore` had a bare `demo.sh` entry meant for
/// a local scratch script at the repo root; a gitignore pattern with no slash
/// matches at every depth, so it silently excluded `scripts/demo.sh` — the demo
/// script the README tells a stranger to run, and the one the `demo` job
/// executes. The job failed in CI with "No such file or directory" while
/// passing everywhere the file happened to exist on disk, which is every
/// developer's machine and no runner's checkout.
///
/// Asserting the job's *shape* did not catch it; several tests here check that
/// the demo job runs and checks the right things, and all of them were green.
/// The gap is that nothing asked whether the file is tracked.
#[test]
fn every_script_ci_runs_is_tracked_by_git() {
    let ci = ci_yml();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    // The scripts the workflow shells out to.
    let mut referenced: Vec<String> = Vec::new();
    for line in ci.lines() {
        for token in line.split_whitespace() {
            let token = token.trim_matches(|c| c == '"' || c == '\'' || c == '`');
            if let Some(path) = token.strip_prefix("./scripts/") {
                let path = path.split_whitespace().next().unwrap_or("");
                if !path.is_empty() && !path.contains('$') {
                    referenced.push(format!("scripts/{path}"));
                }
            }
        }
    }
    referenced.sort();
    referenced.dedup();
    assert!(
        referenced.len() >= 3,
        "parsed only {referenced:?} — the extractor drifted"
    );

    for path in &referenced {
        assert!(
            root.join(path).exists(),
            "ci.yml runs `{path}` but it does not exist in the tree"
        );
        // The load-bearing check, and the only one that can work here.
        // `git check-ignore` reports nothing for a TRACKED file, so it passes
        // for `scripts/demo.sh` the moment it is added — which is exactly when
        // the guard matters least. `git ls-files` asks the question that
        // actually decides whether CI works: is this file in the repository?
        let tracked = std::process::Command::new("git")
            .args(["ls-files", "--error-unmatch", path])
            .current_dir(root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(
            tracked,
            "`{path}` is run by ci.yml but is not tracked by git, so it is absent from a \
             fresh checkout and the job fails with 'No such file or directory' — on every \
             runner, and on no developer machine, where the file merely exists on disk."
        );
    }
}

/// The install job must install the version the package managers name.
///
/// It used to install `${TAG:-v0.8.0}`. `TAG` is set nowhere in the workflow,
/// so the fallback always won: the job installed a release from before last,
/// ran `--version` on it, and went green. A gate that cannot fail is the exact
/// failure mode this repo has shipped before, and this one was invisible because
/// a green job looks identical to a green gate.
///
/// A user's binary comes from `scoop/niki.json`, `homebrew/niki.rb` or the
/// winget manifest, so the job reads the version out of one of those rather than
/// hardcoding anything. Cross-manifest agreement is `tests/dist_install.rs`.
#[test]
fn the_install_job_installs_the_manifest_version() {
    let ci = ci_yml();
    let body = job_body_without_comments(&ci, "install");

    assert!(
        !body.contains("v0.8.0"),
        "the install job must not hardcode a released version; it would keep passing against a \
         release from before last while the manifests point somewhere newer"
    );
    assert!(
        !body.contains("${TAG"),
        "`TAG` is never set in this workflow, so a TAG-with-default fallback is a constant \
         wearing a variable's name"
    );
    assert!(
        body.contains("scoop/niki.json"),
        "the install job must read the version from a package manifest, so it tests what a user \
         actually gets"
    );
    assert!(
        body.contains("::warning"),
        "when the named version is not released yet, the job must say so loudly rather than \
         quietly passing — an untested install path reported as green is the failure being fixed"
    );
}

/// Every job that compiles the workspace must cache cargo, and must do it
/// before compiling.
///
/// `git2` uses vendored-libgit2, so a cold build pays a full C compile of
/// libgit2 — and a dozen jobs in this workflow each build the workspace from
/// scratch on their own runner, every run.
///
/// The ordering half is the one that is easy to get wrong and impossible to
/// notice: a cache step placed after the build is valid YAML, runs without
/// error, and does nothing at all. So the assertion is on line order within a
/// job, not on presence.
#[test]
fn cargo_caching_precedes_every_workspace_build() {
    let ci = ci_yml();
    let lines: Vec<&str> = ci.lines().collect();

    // Cargo invocations that compile this workspace. `audit`'s `cargo install`
    // of cargo-audit/cargo-deny is deliberately absent: it builds unrelated
    // tools and is not what this is about.
    let compiles = [
        "cargo build --release",
        "cargo build --no-default-features",
        "cargo nextest run",
        "cargo clippy --all-targets",
        "cargo check --all-targets",
        "./scripts/product-verify.sh",
    ];

    // Job boundaries: two-space-indented `name:` lines.
    let mut bounds: Vec<(String, usize, usize)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let indent = l.len() - l.trim_start().len();
        let t = l.trim();
        if indent == 2 && t.ends_with(':') && !t.starts_with('#') {
            if let Some(prev) = bounds.last_mut() {
                prev.2 = i;
            }
            bounds.push((t.trim_end_matches(':').to_string(), i, lines.len()));
        }
    }

    let mut checked = 0;
    for (name, start, end) in &bounds {
        let body = &lines[*start..*end];
        let first_build = body.iter().position(|l| {
            let t = l.trim();
            compiles
                .iter()
                .any(|c| t == format!("run: {c}").as_str() || t.ends_with(c))
        });
        let Some(first_build) = first_build else {
            continue;
        };
        checked += 1;

        let cache_at = body.iter().position(|l| l.contains("Swatinem/rust-cache"));

        assert!(
            cache_at.is_some(),
            "job `{name}` compiles the workspace but has no cargo-cache step"
        );
        assert!(
            cache_at < Some(first_build),
            "job `{name}` caches cargo at offset {cache_at:?}, after its first compile at \
             offset {first_build}. A cache placed after the build is valid YAML that does nothing."
        );
    }
    assert!(
        checked >= 10,
        "only matched {checked} compiling jobs out of {} — the extractor drifted as the \
         workflow changed",
        bounds.len()
    );
}
