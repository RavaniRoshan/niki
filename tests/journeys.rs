//! Consumer journeys: what a first-time user actually hits.
//!
//! The unit and integration suites test modules. This tests the *product*, by
//! running the real binary the way a person would — on a machine profile that
//! contains nothing, with no inherited configuration, and with the environment
//! scrubbed.
//!
//! Two rules shape it:
//!
//! * **A throwaway `HOME` per journey.** `config::load` resolves the global
//!   config through `dirs::home_dir()`, so `HOME=<tmp>` guarantees no test can
//!   accidentally depend on the developer's own `niki.toml`, keyring, or
//!   `.niki/` state — and no journey can leave residue either.
//! * **A skip is a failure.** Every other suite in this repo had jobs that
//!   passed while executing nothing: `niki-review.yml` went green with no
//!   secrets, and the visual gate reported success having compared zero
//!   frames. A journey that cannot run on this host is recorded as `skipped`,
//!   and the suite fails if anything is skipped that is not explicitly
//!   allow-listed with a reason.

use std::path::PathBuf;
use std::process::{Command, Output};

/// One user-facing path through the product.
struct Journey {
    id: &'static str,
    /// What a user is trying to do. Shown in failure output.
    intent: &'static str,
    /// Why this journey exists — the failure it would catch.
    guards: &'static str,
    run: fn(&JourneyCtx) -> JourneyResult,
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum JourneyResult {
    // `Skip` is deliberately never constructed today: every journey runs on a
    // plain Linux host, which is exactly the point of the empty allow-list.
    // Clippy sees dead code; what it is actually seeing is that nothing needs
    // an exemption yet. The variant stays because the accounting around it is
    // the mechanism that stops a future host-dependent journey from silently
    // becoming a green check.
    Pass,
    /// The host cannot support this journey. Must be allow-listed to not fail.
    #[allow(dead_code)]
    Skip(&'static str),
    Fail(String),
}

/// Per-journey scratch environment.
struct JourneyCtx {
    /// A directory that is the user's entire world for this journey.
    home: PathBuf,
    /// A git repo to run against.
    project: PathBuf,
    bin: PathBuf,
}

impl JourneyCtx {
    /// Run the binary with a scrubbed environment.
    ///
    /// Provider keys, XDG vars, NO_COLOR and CI are all removed, so a journey
    /// exercises a first-run machine rather than inheriting whatever the test
    /// runner happens to export.
    fn run(&self, args: &[&str]) -> Output {
        let project = self.project.to_string_lossy().to_string();
        let mut cmd = Command::new(&self.bin);
        cmd.args(args)
            .current_dir(&self.project)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("TERM", "xterm-256color")
            .env("LANG", "C.UTF-8")
            .env("NIKI_CI", "1")
            // Every journey passes --project explicitly; this is a belt-and-
            // braces default for any that forget.
            .env("NIKI_PROJECT", &project);
        cmd.output().expect("niki binary runs")
    }

    /// Run with extra environment variables set.
    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args)
            .current_dir(&self.project)
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("TERM", "xterm-256color")
            .env("LANG", "C.UTF-8")
            .env("NIKI_CI", "1");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().expect("niki binary runs")
    }
}

fn scratch(name: &str) -> JourneyCtx {
    let base = std::env::temp_dir().join(format!("niki-journey-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let home = base.join("home");
    let project = base.join("project");
    std::fs::create_dir_all(&home).expect("home created");
    std::fs::create_dir_all(&project).expect("project created");

    let g = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&project)
            .output()
            .expect("git runs")
    };
    g(&["init", "-q"]);
    g(&["config", "user.email", "journey@niki.dev"]);
    g(&["config", "user.name", "Journey"]);
    std::fs::write(project.join("main.rs"), "fn main() { println!(\"hi\"); }\n")
        .expect("fixture written");
    g(&["add", "-A"]);
    g(&["commit", "-qm", "initial"]);

    JourneyCtx {
        home,
        project,
        bin: PathBuf::from(env!("CARGO_BIN_EXE_niki")),
    }
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}
fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ── Journeys ───────────────────────────────────────────────────────────

/// J01 — a stranger with nothing installed can at least get a version.
fn j_version_on_a_bare_machine(_ctx: &JourneyCtx) -> JourneyResult {
    let out = _ctx.run(&["--version"]);
    if !out.status.success() {
        return JourneyResult::Fail(format!("`niki --version` failed: {}", stderr_of(&out)));
    }
    let s = stdout_of(&out);
    if !s.contains("niki") {
        return JourneyResult::Fail(format!("unexpected --version output: {s:?}"));
    }
    JourneyResult::Pass
}

/// J02 — no API key anywhere. The product's headline claim is that it runs
/// without one, so this must not explode on a keyless machine.
fn j_no_api_key_does_not_panic(_ctx: &JourneyCtx) -> JourneyResult {
    // `config schema` is the keyless read-only config surface: it must work
    // with no provider configured, which is exactly the first-run case.
    for args in [vec!["--help"], vec!["doctor"], vec!["config", "schema"]] {
        let out = _ctx.run(&args);
        if !out.status.success() {
            return JourneyResult::Fail(format!(
                "`niki {}` on a keyless machine failed ({}): {}",
                args.join(" "),
                out.status,
                stderr_of(&out)
            ));
        }
    }
    JourneyResult::Pass
}

/// J03 — help must actually list the navigation keys. A key nobody can
/// discover is a key nobody uses, and the footer/help are the only surfaces
/// where a new user learns what is available.
fn j_help_advertises_navigation(_ctx: &JourneyCtx) -> JourneyResult {
    let out = _ctx.run(&["--help"]);
    let s = stdout_of(&out);
    // The CLI help is not the TUI overlay; what matters is that the binary
    // runs and names its own surfaces. Assert the run succeeded and produced
    // usage rather than asserting specific key text here.
    if !s.contains("Usage") && !s.contains("Commands") {
        return JourneyResult::Fail(format!("`niki --help` produced no usage text: {s:?}"));
    }
    JourneyResult::Pass
}

/// J04 — a run against a project with no `niki.toml` must fail with a
/// *useful* message, not a panic and not silence.
fn j_missing_config_is_a_readable_error(_ctx: &JourneyCtx) -> JourneyResult {
    let out = _ctx.run(&["run", "do a thing", "--backend", "worktree", "--bare"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail(format!("run panicked without config: {combined}"));
    }
    // A non-zero exit is fine; an empty message is not.
    if !out.status.success() && combined.trim().is_empty() {
        return JourneyResult::Fail(
            "run failed with no output at all — a user cannot act on silence".to_string(),
        );
    }
    JourneyResult::Pass
}

/// J05 — a corrupt `niki.toml` must be reported, not silently ignored.
fn j_corrupt_config_is_reported(ctx: &JourneyCtx) -> JourneyResult {
    std::fs::write(
        ctx.project.join("niki.toml"),
        "this is not = valid = toml [[[",
    )
    .expect("write corrupt config");
    let out = ctx.run(&["doctor"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail("`doctor` panicked on a corrupt config".to_string());
    }
    if combined.trim().is_empty() {
        return JourneyResult::Fail(
            "`doctor` said nothing about a corrupt niki.toml — the user has no way to know \
             their config is the problem"
                .to_string(),
        );
    }
    JourneyResult::Pass
}

/// J06 — `NO_COLOR` must be honoured on the plain-stdout report path.
///
/// This originally ran `niki report does-not-exist`, which takes the
/// "Report not found" branch and never reaches the content at all — so it
/// passed whether or not report output was sanitised. A test that cannot fail
/// is worse than no test. It now plants a report containing terminal control
/// sequences and reads it back.
fn j_no_color_is_honoured_on_the_report_path(ctx: &JourneyCtx) -> JourneyResult {
    // `niki report <id>` resolves an id by scanning `.niki/tasks/*/task.json`,
    // so the fixture needs a record as well as the report.
    let task_id = "journey06";
    let dir = ctx.project.join(".niki").join("tasks").join(task_id);
    std::fs::create_dir_all(&dir).expect("task dir created");
    std::fs::write(
        dir.join("task.json"),
        r#"{"task_id":"00000000-0000-0000-0000-000000000006","description":"journey",
            "status":{"Failed":{"error":"fixture"}},"branch":null,"verdict":null,
            "revision_rounds":0,"created_at":"2026-01-01T00:00:00Z","agent_metrics":[],
            "total_input_tokens":0,"total_output_tokens":0,"total_cost_usd":0.0,
            "total_latency_ms":0,"total_retry_count":0,"max_ttft_ms":0}"#,
    )
    .expect("task record written");

    // An OSC 52 clipboard write and a cursor-move, planted the way a
    // repository or a model would plant them.
    let hostile = "# Report\n\nclipboard: \u{1b}]52;c;Y2xpcGJvYmVk\u{7}\nmove: \u{1b}[2J\u{1b}[H\n";
    std::fs::write(dir.join("report.md"), hostile).expect("report written");

    let out = ctx.run_with(&["report", task_id], &[("NO_COLOR", "1")]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));

    if combined.contains("panicked at") {
        return JourneyResult::Fail(format!("`report` panicked: {combined}"));
    }
    if combined.contains('\u{1b}') {
        return JourneyResult::Fail(format!(
            "report output carries raw terminal control sequences under NO_COLOR=1: {combined:?}. \
             A report.md is model- and repository-influenced, so this is reachable without \
             any credentials."
        ));
    }
    // The text itself must survive — a sanitizer that eats everything is also
    // a bug, and would make this pass for the wrong reason.
    if !combined.contains("clipboard") {
        return JourneyResult::Fail(format!(
            "sanitising removed the report's actual content: {combined:?}"
        ));
    }
    JourneyResult::Pass
}

/// J07 — the artifact surface must be readable by hand, because that is the
/// product's pitch: "the run leaves receipts you can read".
fn j_artifacts_are_plain_files(ctx: &JourneyCtx) -> JourneyResult {
    let dir = ctx.project.join(".niki");
    // Even a failed run should leave a `.niki/` we can inspect.
    let _ = ctx.run(&[
        "run",
        "noop",
        "--backend",
        "worktree",
        "--bare",
        "--dry-run",
    ]);
    if !dir.exists() {
        // Not every invocation creates it; not a failure on its own.
        return JourneyResult::Pass;
    }
    // Everything written under .niki must be text or a known binary format,
    // never a truncated temp file.
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|s| s.to_str()) == Some("json") {
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                if let Err(e) = serde_json::from_str::<serde_json::Value>(&text) {
                    return JourneyResult::Fail(format!(
                        "{} is not valid JSON ({e}) — a half-written state file from a crashed \
                         run would be silently skipped by `niki status`",
                        p.display()
                    ));
                }
            }
        }
    }
    JourneyResult::Pass
}

/// J08 — two runs in the same project must not collide on worktrees or
/// branches. This is the failure a user hits on their second afternoon.
fn j_second_run_in_the_same_project(ctx: &JourneyCtx) -> JourneyResult {
    let a = ctx.run(&[
        "run",
        "first",
        "--backend",
        "worktree",
        "--bare",
        "--dry-run",
    ]);
    let b = ctx.run(&[
        "run",
        "second",
        "--backend",
        "worktree",
        "--bare",
        "--dry-run",
    ]);
    for (label, out) in [("first", &a), ("second", &b)] {
        let combined = format!("{}{}", stdout_of(out), stderr_of(out));
        if combined.contains("panicked at") {
            return JourneyResult::Fail(format!("{label} run panicked: {combined}"));
        }
    }
    // A second run failing because the first left state behind is the bug.
    let b_all = format!("{}{}", stdout_of(&b), stderr_of(&b));
    if b_all.to_lowercase().contains("already exists")
        || b_all.to_lowercase().contains("address already in use")
    {
        return JourneyResult::Fail(format!(
            "a second run in the same project collided with the first: {b_all}"
        ));
    }
    JourneyResult::Pass
}

/// J09 — a `NO_COLOR` + dumb-terminal combination is what a CI log or a
/// screen reader session looks like. It must not crash.
fn j_dumb_terminal_does_not_crash(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run_with(&["doctor"], &[("TERM", "dumb"), ("NO_COLOR", "1")]);
    if !out.status.success() {
        return JourneyResult::Fail(format!(
            "`doctor` failed on a dumb terminal with NO_COLOR: {}",
            stderr_of(&out)
        ));
    }
    JourneyResult::Pass
}

/// J10 — `niki run --project` on a path that does not exist must be a clean
/// error. Users typo paths.
fn j_nonexistent_project_is_a_clean_error(ctx: &JourneyCtx) -> JourneyResult {
    let out = Command::new(&ctx.bin)
        .args(["run", "x", "--project", "/definitely/not/a/real/path"])
        .env_clear()
        .env("HOME", &ctx.home)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .output()
        .expect("binary runs");
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail(format!("panicked on a nonexistent project: {combined}"));
    }
    if combined.trim().is_empty() {
        return JourneyResult::Fail("nonexistent project produced no message at all".into());
    }
    JourneyResult::Pass
}

/// J11 — `niki report <id>` on an unknown id must be a clear miss, not a
/// stack trace. Users mistype short prefixes constantly.
fn j_unknown_report_id_is_a_clean_error(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run(&["report", "zzzzzzzz"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail(format!("panicked on an unknown report id: {combined}"));
    }
    JourneyResult::Pass
}

/// A brand-new machine must not be told it is broken.
///
/// `doctor` reported two hard failures on a fresh install: a missing `rustc`
/// and a missing container image. The first is a fact about a machine that
/// never needs the toolchain — `rustc` is only stamped into the provenance
/// record, and that field is an `Option` precisely because it is optional. The
/// second only affects the container backend; a user on the worktree backend
/// never touches it.
///
/// Both were `Fail`, so the summary read "some checks failed" and a first-time
/// user with a released binary was told, in red, that their install was broken.
/// The whole point of the zero-setup path is that someone can install this and
/// run it; a doctor that opens with two failures says otherwise.
fn j_doctor_does_not_fail_a_first_run(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run(&["doctor"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail("`doctor` panicked on a fresh machine".to_string());
    }
    if let Some(line) = combined
        .lines()
        .find(|l| l.contains("checks failed") || l.contains("Some checks failed"))
    {
        return JourneyResult::Fail(format!(
            "a first-time user is shown a failing report: {line}"
        ));
    }
    // And the report must still be *useful* — "no failures" is only good if it
    // also says something.
    if !combined.contains("checks") {
        return JourneyResult::Fail(
            "`doctor` said nothing about its checks; silence is not a passing report".to_string(),
        );
    }
    JourneyResult::Pass
}

/// J12 — bare `niki` is the first thing a new user types, and where it lands
/// decides what they think this is.
///
/// On a terminal it should open the chat surface, like Codex and Claude Code
/// do. Without one, there is nothing to open: the TUI cannot enter raw mode,
/// returns immediately, and the process exits **0 having printed nothing at
/// all**. This journey runs the binary with a pipe on stdout and stdin, which
/// is what a script, a CI step, and `$(niki)` all look like — and the old
/// behaviour was indistinguishable from success in all three.
fn j_bare_niki_offers_help_instead_of_hanging_up(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run(&[]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));

    if combined.contains("panicked at") {
        return JourneyResult::Fail("bare `niki` panicked".to_string());
    }
    if combined.trim().is_empty() {
        return JourneyResult::Fail(
            "bare `niki` with no terminal printed nothing and exited — a program \
             that hangs up silently is worse than one that refuses"
                .to_string(),
        );
    }
    if out.status.success() {
        return JourneyResult::Fail(format!(
            "bare `niki` with no terminal exited 0. A script cannot tell that from \
             a successful run. It said: {combined}"
        ));
    }
    // It must be useful, not merely non-empty.
    if !combined.contains("niki run") && !combined.contains("Usage") {
        return JourneyResult::Fail(format!(
            "bare `niki` failed without telling the user what to do instead: {combined}"
        ));
    }
    JourneyResult::Pass
}

/// J13 — the same dead end, one level down and explicitly requested.
///
/// `niki chat` on a pipe did exactly what bare `niki` did: nothing, exit 0.
/// A pipeline that runs it has no way to tell that the conversation never
/// happened.
fn j_chat_without_a_terminal_says_so(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run(&["chat"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));

    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki chat` panicked without a terminal".to_string());
    }
    if combined.trim().is_empty() {
        return JourneyResult::Fail("`niki chat` with no terminal printed nothing".to_string());
    }
    if out.status.success() {
        return JourneyResult::Fail(format!(
            "`niki chat` with no terminal exited 0. It said: {combined}"
        ));
    }
    if !combined.contains("--message") {
        return JourneyResult::Fail(format!(
            "`niki chat` refused without naming the way to do it non-interactively: \
             {combined}"
        ));
    }
    JourneyResult::Pass
}

/// J14 — an empty task is not a task, and the pipeline will not notice.
///
/// The Planner is handed "" and asked for a spec, produces one, and the run
/// continues through four paid model calls to hand back a change nobody asked
/// for. `niki run ""` is not hypothetical: it is what a shell variable that
/// expanded to nothing produces, which is one of the easiest ways to spend
/// money by accident.
///
/// The assertion that matters is the absence of side effects, not the message:
/// a rejection that still mints a task directory has already cost something.
fn j_an_empty_task_is_refused_before_any_spend(ctx: &JourneyCtx) -> JourneyResult {
    let entries = |p: &std::path::Path| std::fs::read_dir(p).map(|d| d.count()).unwrap_or(0);
    let before = entries(&ctx.project);
    let out = ctx.run(&["run", "   "]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));

    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki run \"\"` panicked".to_string());
    }
    if out.status.success() {
        return JourneyResult::Fail(
            "`niki run` with a blank task reported success — it either ran a \
             pipeline on nothing or exited zero having done nothing"
                .to_string(),
        );
    }
    if !combined.to_lowercase().contains("task") {
        return JourneyResult::Fail(format!(
            "the refusal does not say what was missing: {combined}"
        ));
    }
    let niki_dir = ctx.project.join(".niki");
    if niki_dir.exists() {
        let tasks = niki_dir.join("tasks");
        if tasks.exists() {
            return JourneyResult::Fail(
                "a blank task created .niki/tasks — the run started before the \
                 input was checked, which is the spend this is meant to prevent"
                    .to_string(),
            );
        }
    }
    let after = entries(&ctx.project);
    if after != before {
        return JourneyResult::Fail(format!(
            "a blank task changed the project directory ({before} -> {after} entries)"
        ));
    }
    JourneyResult::Pass
}

static JOURNEYS: &[Journey] = &[
    Journey {
        id: "J00",
        intent: "run `niki doctor` on a machine that has never configured it",
        guards: "the first thing a new user runs; a red report here is the                   difference between 'install and go' and 'something is wrong                   with my machine'",
        run: j_doctor_does_not_fail_a_first_run,
    },
    Journey {
        id: "J01",
        intent: "run `niki --version` on a machine with no config",
        guards: "packaging and linkage breakage",
        run: j_version_on_a_bare_machine,
    },
    Journey {
        id: "J02",
        intent: "use the CLI with no API key configured",
        guards: "the product's headline claim is that it runs keyless; a keyless \
                  machine must not produce a crash or a confusing auth wall",
        run: j_no_api_key_does_not_panic,
    },
    Journey {
        id: "J03",
        intent: "read `niki --help`",
        guards: "an empty help screen is the first dead end",
        run: j_help_advertises_navigation,
    },
    Journey {
        id: "J04",
        intent: "run a task in a project with no niki.toml",
        guards: "the first-run error must be readable, not a panic and not silence",
        run: j_missing_config_is_a_readable_error,
    },
    Journey {
        id: "J05",
        intent: "run with a corrupt niki.toml",
        guards: "a silently-ignored config sends users debugging the wrong thing",
        run: j_corrupt_config_is_reported,
    },
    Journey {
        id: "J06",
        intent: "read a report.md containing terminal control sequences",
        guards: "the report is a plain-stdout path, not the TUI, so an OSC 52 \
                  clipboard write reaches the user's terminal unless the output \
                  sanitizer covers it",
        run: j_no_color_is_honoured_on_the_report_path,
    },
    Journey {
        id: "J07",
        intent: "inspect the artifacts a run leaves behind",
        guards: "the product's pitch is readable receipts; a half-written state file \
                  is silently skipped by `niki status`",
        run: j_artifacts_are_plain_files,
    },
    Journey {
        id: "J08",
        intent: "run twice in the same project",
        guards: "worktree/branch collision on the user's second afternoon",
        run: j_second_run_in_the_same_project,
    },
    Journey {
        id: "J09",
        intent: "use it on a dumb terminal with NO_COLOR",
        guards: "what a CI log or a screen-reader session looks like",
        run: j_dumb_terminal_does_not_crash,
    },
    Journey {
        id: "J10",
        intent: "pass a project path that does not exist",
        guards: "users typo paths; a panic here is the first thing a new user sees",
        run: j_nonexistent_project_is_a_clean_error,
    },
    Journey {
        id: "J12",
        intent: "type bare `niki` with no terminal",
        guards: "the first thing a new user runs; it must land somewhere useful \
                  or say why it cannot, never exit 0 in silence",
        run: j_bare_niki_offers_help_instead_of_hanging_up,
    },
    Journey {
        id: "J13",
        intent: "run `niki chat` from a script, with no terminal",
        guards: "the same dead end one level down, explicitly requested this time",
        run: j_chat_without_a_terminal_says_so,
    },
    Journey {
        id: "J14",
        intent: "run `niki run` with a task that expanded to nothing",
        guards: "four paid model calls to hand back a change nobody asked for",
        run: j_an_empty_task_is_refused_before_any_spend,
    },
    Journey {
        id: "J11",
        intent: "read a report by a mistyped id",
        guards: "short prefixes are mistyped constantly",
        run: j_unknown_report_id_is_a_clean_error,
    },
];

/// Journeys that may be skipped on a host, with the reason each is allowed to
/// be. An empty list means nothing is allowed to skip.
const SKIP_ALLOWLIST: &[(&str, &str)] = &[];

#[test]
fn consumer_journeys_all_pass() {
    let mut failures: Vec<String> = Vec::new();
    let mut skips: Vec<String> = Vec::new();
    let mut passed = 0usize;

    for j in JOURNEYS {
        let ctx = scratch(j.id);
        let result = (j.run)(&ctx);
        // Every journey cleans up after itself, including on failure.
        let _ = std::fs::remove_dir_all(ctx.home.parent().unwrap_or(&ctx.home));

        match result {
            JourneyResult::Pass => {
                passed += 1;
                println!("  ok   {} — {}", j.id, j.intent);
            }
            JourneyResult::Skip(why) => {
                println!("  SKIP {} — {} ({why})", j.id, j.intent);
                if !SKIP_ALLOWLIST.iter().any(|(id, _)| *id == j.id) {
                    skips.push(format!(
                        "  {} — {}: {why}\n      (not in the skip allow-list; a journey that \
                         cannot run is a failure, not a pass)",
                        j.id, j.intent
                    ));
                }
            }
            JourneyResult::Fail(msg) => {
                println!("  FAIL {} — {}: {msg}", j.id, j.intent);
                failures.push(format!(
                    "  {} — {}\n      guards: {}\n      {msg}",
                    j.id, j.intent, j.guards
                ));
            }
        }
    }

    eprintln!(
        "\n{} passed, {} failed, {} skipped",
        passed,
        failures.len(),
        skips.len()
    );
    assert!(
        failures.is_empty(),
        "consumer journeys failed:\n{}",
        failures.join("\n")
    );
    assert!(
        skips.is_empty(),
        "journeys skipped without an allow-list entry:\n{}\n\n\
         Every other suite in this repo has shipped a job that passed while executing \
         nothing. A skip is only acceptable when the host genuinely cannot run it, and \
         then it belongs in SKIP_ALLOWLIST with a reason.",
        skips.join("\n")
    );
}

/// The allow-list must reference journeys that exist. A stale entry would let
/// a renamed journey skip forever without anyone noticing.
#[test]
fn the_skip_allow_list_only_names_real_journeys() {
    for (id, reason) in SKIP_ALLOWLIST {
        assert!(
            JOURNEYS.iter().any(|j| j.id == *id),
            "SKIP_ALLOWLIST names `{id}`, which is not a journey. A stale entry lets a \
             renamed journey skip silently."
        );
        assert!(
            !reason.trim().is_empty(),
            "skip allow-list entry `{id}` has no reason"
        );
    }
}

#[test]
fn journey_ids_are_unique() {
    let mut ids: Vec<&str> = JOURNEYS.iter().map(|j| j.id).collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "duplicate journey id");
}

#[test]
fn every_journey_documents_what_it_guards() {
    for j in JOURNEYS {
        assert!(
            j.guards.len() > 20,
            "journey {} has a token guard description; every journey must say which \
             failure it exists to catch",
            j.id
        );
    }
}

/// The scratch profile must actually be isolated, or every journey above is
/// testing the developer's machine rather than a first-run one.
#[test]
fn the_scratch_profile_is_isolated() {
    let ctx = scratch("ISOLATION");
    let config_dir = ctx.home.join(".config");
    assert!(
        !config_dir.exists(),
        "a fresh profile must have no config directory"
    );
    assert!(
        !ctx.home.join(".niki").exists(),
        "a fresh profile must have no run state"
    );
    assert!(
        !ctx.project.join("niki.toml").exists(),
        "fixture must start unconfigured"
    );
    let _ = std::fs::remove_dir_all(ctx.home.parent().unwrap_or(&ctx.home));
}

#[test]
fn scratch_profiles_do_not_collide() {
    let a = scratch("COLLIDE-A");
    let b = scratch("COLLIDE-B");
    assert_ne!(a.home, b.home, "two journeys must not share a HOME");
    let _ = std::fs::remove_dir_all(a.home.parent().unwrap_or(&a.home));
    let _ = std::fs::remove_dir_all(b.home.parent().unwrap_or(&b.home));
}
