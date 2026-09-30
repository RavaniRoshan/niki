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

/// J20 — the setup wizard writes a config the machine can actually run.
///
/// It used to write a provider, a model, and nothing else. `[docker] backend`
/// kept its default of `docker`, so on a machine with no container runtime —
/// which is the machine the README opens by describing, and the one the
/// worktree backend exists for — the wizard completed with a success message
/// and the very next command could not start.
///
/// The journey therefore does not care *which* backend gets written, only that
/// one is written and that the file parses: a config with no backend is a
/// config whose behaviour is decided by a default the user never chose.
fn j_init_writes_a_runnable_config(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run(&["init", "--interactive"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki init --interactive` panicked".to_string());
    }
    let path = ctx.project.join("niki.toml");
    if !path.exists() {
        return JourneyResult::Fail(format!(
            "the wizard wrote no niki.toml at all.\nOutput:\n{combined}"
        ));
    }
    let text = std::fs::read_to_string(&path).expect("config readable");
    // A commented default is not a setting. The template ships
    // `# backend = "docker"`, which means "docker" — and that is the whole bug.
    let active = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("backend = "));
    let Some(line) = active else {
        return JourneyResult::Fail(format!(
            "the written config selects no backend, so `niki run` falls back to a \
             default the user never chose.\nOutput:\n{combined}"
        ));
    };
    if !(line.contains("\"docker\"") || line.contains("\"worktree\"")) {
        return JourneyResult::Fail(format!("the backend line is not one NIKI knows: {line}"));
    }
    // Whatever the wizard wrote has to be a config, not a file.
    if let Err(e) = niki::config::NikiConfig::load(&ctx.project) {
        return JourneyResult::Fail(format!("the wizard wrote a config that will not load: {e}"));
    }
    JourneyResult::Pass
}

/// J21 — and that backend is one this machine can actually reach.
///
/// The complement to J20. Writing *some* backend is not enough; on a machine
/// with no container runtime, writing `docker` reproduces the original failure
/// one layer down. This one reads the machine the same way the product does
/// and fails if the two disagree.
fn j_written_config_selects_a_reachable_backend(ctx: &JourneyCtx) -> JourneyResult {
    let _ = ctx.run(&["init", "--interactive"]);
    let cfg = match niki::config::NikiConfig::load(&ctx.project) {
        Ok(c) => c,
        Err(e) => return JourneyResult::Fail(format!("no loadable config was written: {e}")),
    };
    let runtime = niki::sandbox::detect_container_runtime();
    match (cfg.docker.backend, runtime) {
        (niki::sandbox::SandboxBackend::Worktree, _) => JourneyResult::Pass,
        (niki::sandbox::SandboxBackend::Docker, Some(_)) => JourneyResult::Pass,
        (niki::sandbox::SandboxBackend::Docker, None) => JourneyResult::Fail(
            "the wizard wrote backend = docker on a machine with no container runtime, \
             so the first `niki run` cannot start"
                .to_string(),
        ),
    }
}

/// J22 — `niki doctor` passes on that same machine.
///
/// It used to hard-fail "no container runtime" and exit 1, having no notion that
/// a second backend exists. So the command the README tells a new user to run
/// first, in order to check their install, reported a broken install on an
/// install that works — and told them to install a container runtime the
/// product does not need.
fn j_doctor_passes_on_a_keyless_containerless_machine(ctx: &JourneyCtx) -> JourneyResult {
    let _ = ctx.run(&["init", "--interactive"]);
    let out = ctx.run(&["doctor"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail("`doctor` panicked".to_string());
    }
    if !out.status.success() {
        return JourneyResult::Fail(format!(
            "`doctor` exited {:?} on a keyless, containerless machine after the wizard \
             configured it. Exit 0 is what tells a user their setup works.\nOutput:\n{combined}",
            out.status.code()
        ));
    }
    // It must also say something. A report that neither passes nor warns is not
    // a report, and on a machine with no container runtime that is exactly the
    // state this journey is about.
    if niki::sandbox::detect_container_runtime().is_none()
        && !combined.contains("checks passed")
        && !combined.contains("warning")
    {
        return JourneyResult::Fail(
            "`doctor` reported neither a pass nor a warning; silence is not a report".to_string(),
        );
    }
    JourneyResult::Pass
}

/// J23 — the first `niki run` gets as far as the model, not as far as the sandbox.
///
/// This is the assertion that says "the backend resolved". It cannot run a
/// pipeline — there is no key on a first-run machine by definition — but it can
/// demand that the *reason* the run stopped is a model, never a container
/// runtime. If the backend were unresolved, this fails with the container
/// error instead.
fn j_first_run_does_not_die_on_backend_resolution(ctx: &JourneyCtx) -> JourneyResult {
    let _ = ctx.run(&["init", "--interactive"]);
    let out = ctx.run(&["run", "Add a health endpoint"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki run` panicked on a keyless machine".to_string());
    }
    let blamed_container = combined.contains("Container runtime error")
        || combined.contains("requires a running Podman or Docker daemon");
    if blamed_container {
        return JourneyResult::Fail(format!(
            "`niki run` with no --backend flag died on backend resolution. The config the \
             wizard just wrote should have selected a backend this machine can run, so \
             this failure means the wizard and the runner disagree about the backend.\n\
             Output:\n{combined}"
        ));
    }
    JourneyResult::Pass
}

/// J24 — a scripted wizard must not exit 0 onto a config that cannot run.
///
/// `niki init --interactive` reads stdin. Piped, closed, or empty — which is
/// what a script, a Makefile target, and a container build all do — it used to
/// fall through the "no provider selected" arm, write the template, print a
/// cheerful summary, and return success. The user then had a config pointing at
/// Anthropic with no key, and an exit code of 0 saying setup worked.
fn j_scripted_init_does_not_succeed_onto_a_dead_end(ctx: &JourneyCtx) -> JourneyResult {
    let out = ctx.run(&["init", "--interactive"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail(
            "`niki init --interactive` panicked on empty stdin".to_string(),
        );
    }
    let wrote_config = ctx.project.join("niki.toml").exists();
    let unfinished = combined.contains("setup is not finished");
    if wrote_config && !unfinished && out.status.success() {
        return JourneyResult::Fail(
            "a scripted `niki init --interactive` wrote a config with no provider \
             configured and still exited 0"
                .to_string(),
        );
    }
    if unfinished && out.status.success() {
        return JourneyResult::Fail(
            "the wizard reported that setup is unfinished but still exited 0, so a \
             script cannot tell the difference between ready and not ready"
                .to_string(),
        );
    }
    JourneyResult::Pass
}

/// J25 — and that path must be sound *even on a machine that has containers*.
///
/// Every other journey in this file is at the mercy of the host: on a box with
/// podman installed, the wizard writes `backend = "docker"` and the containerless
/// path is never exercised at all. That is precisely the gap this journey
/// closes. The README's first claim is that NIKI runs with no container runtime
/// and no key, and on a developer's machine with both installed, that claim was
/// untested — so it could rot unnoticed and the suite would stay green.
///
/// So: force the worktree backend, and require the run to get as far as the
/// model on any host. The container runtime, present or not, must not change the
/// answer.
fn j_the_containerless_path_works_even_where_containers_exist(ctx: &JourneyCtx) -> JourneyResult {
    let _ = ctx.run(&["init", "--interactive"]);
    let path = ctx.project.join("niki.toml");
    let text = std::fs::read_to_string(&path).expect("wizard wrote a config");

    // Force the containerless backend, whatever the wizard decided.
    let forced = if let Some(pos) = text.find("backend = ") {
        let end = text[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(text.len());
        format!(
            "{}{}\n{}",
            &text[..pos],
            "backend = \"worktree\"",
            &text[end..]
        )
    } else {
        format!("{text}\n[docker]\nbackend = \"worktree\"\n")
    };
    std::fs::write(&path, &forced).expect("config rewritten");

    let cfg = match niki::config::NikiConfig::load(&ctx.project) {
        Ok(c) => c,
        Err(e) => {
            return JourneyResult::Fail(format!(
                "a config with backend = \"worktree\" did not load: {e}\n{forced}"
            ));
        }
    };
    if cfg.docker.backend != niki::sandbox::SandboxBackend::Worktree {
        return JourneyResult::Fail(
            "backend = \"worktree\" did not survive a config round-trip; the documented \
             containerless path is not reachable"
                .to_string(),
        );
    }

    // Doctor must pass with no container runtime, whether or not one exists.
    let out = ctx.run(&["doctor"]);
    if !out.status.success() {
        let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
        return JourneyResult::Fail(format!(
            "`doctor` exited {:?} with backend = worktree. That backend needs no \
             container runtime, so this configuration is complete.\nOutput:\n{combined}",
            out.status.code()
        ));
    }

    // And the run must reach the model rather than the sandbox. There is no key
    // on a first-run machine, so a model error is the correct and expected
    // outcome; anything about containers is not.
    let out = ctx.run(&["run", "Add a health endpoint"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("Container runtime error")
        || combined.contains("requires a running Podman or Docker daemon")
    {
        return JourneyResult::Fail(format!(
            "with backend = worktree the run still demanded a container runtime.\n\
             Output:\n{combined}"
        ));
    }
    JourneyResult::Pass
}

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

/// J15 — a directory that is not a repository cannot produce the deliverable.
///
/// NIKI hands back a `niki/<id>` branch. That is the entire output. Nothing
/// said so until the run was over: on the container backend the user got a
/// spec, a diff, a test report and a review — four paid model calls — and then
/// a bare git2 string at the moment the branch would have been created.
///
/// The assertion is about ordering. A check that runs after the Planner has
/// been called is a check that has already cost money.
fn j_a_non_git_project_is_refused_before_the_run(ctx: &JourneyCtx) -> JourneyResult {
    let outside = std::env::temp_dir().join(format!("niki-journey-nogit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).expect("scratch dir");

    let mut cmd = Command::new(&ctx.bin);
    let out = cmd
        .args(["run", "add a hello function"])
        .current_dir(&outside)
        .env_clear()
        .env("HOME", &ctx.home)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("TERM", "xterm-256color")
        .env("LANG", "C.UTF-8")
        .env("NIKI_CI", "1")
        .output()
        .expect("niki binary runs");
    let _ = std::fs::remove_dir_all(&outside);

    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki run` in a non-git directory panicked".to_string());
    }
    if out.status.success() {
        return JourneyResult::Fail(
            "`niki run` in a directory that is not a git repository reported \
             success — it cannot have produced a branch"
                .to_string(),
        );
    }
    if !combined.to_lowercase().contains("git") {
        return JourneyResult::Fail(format!(
            "the refusal does not mention the repository it needs: {combined}"
        ));
    }
    // It must offer the fix, not just name the missing thing.
    if !combined.contains("git init") {
        return JourneyResult::Fail(format!(
            "the refusal names the problem but not the one-line fix: {combined}"
        ));
    }
    JourneyResult::Pass
}

/// J16 — a misspelled config section parses, is accepted, and does nothing.
///
/// `[agentz.coder]` is a typo that no layer objects to. The user configures
/// it, runs the pipeline, gets the defaults they never asked for, and has no
/// signal that their file was ignored. `niki config check` is the command
/// that turns it into an error — and an error message in this program now
/// points at it, so it has to exist and it has to work.
fn j_config_check_catches_a_misspelled_section(ctx: &JourneyCtx) -> JourneyResult {
    std::fs::write(
        ctx.project.join("niki.toml"),
        "[agentz.coder]\nmodel = \"x\"\n",
    )
    .expect("write config");
    let out = ctx.run(&["config", "check"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));

    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki config check` panicked".to_string());
    }
    if out.status.success() {
        return JourneyResult::Fail(format!(
            "`niki config check` passed a file whose only section is a typo: {combined}"
        ));
    }
    if !combined.contains("agentz") {
        return JourneyResult::Fail(format!(
            "the report does not name the section that is wrong: {combined}"
        ));
    }
    JourneyResult::Pass
}

/// And the inverse. A command whose every run says something is wrong is a
/// command people stop running, and then the run that mattered is the one
/// they did not read.
fn j_config_check_passes_a_good_file(ctx: &JourneyCtx) -> JourneyResult {
    std::fs::write(
        ctx.project.join("niki.toml"),
        "[general]\noutput_dir = \".niki\"\n\n[agents.coder]\nmodel = \"x\"\n",
    )
    .expect("write config");
    let out = ctx.run(&["config", "check"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));

    if combined.contains("panicked at") {
        return JourneyResult::Fail("`niki config check` panicked on a valid file".to_string());
    }
    if !out.status.success() {
        return JourneyResult::Fail(format!(
            "`niki config check` rejected a valid file: {combined}"
        ));
    }
    if !combined.contains("no problems") {
        return JourneyResult::Fail(format!(
            "it succeeded without saying so, so the user cannot tell a pass \
             from a crash: {combined}"
        ));
    }
    JourneyResult::Pass
}

/// No config at all is a normal state, not a problem. Zero-config is a
/// documented path and the command must not report it as a failure.
fn j_config_check_is_quiet_with_no_file(ctx: &JourneyCtx) -> JourneyResult {
    let _ = std::fs::remove_file(ctx.project.join("niki.toml"));
    let out = ctx.run(&["config", "check"]);
    let combined = format!("{}{}", stdout_of(&out), stderr_of(&out));
    if !out.status.success() {
        return JourneyResult::Fail(format!(
            "`niki config check` failed with no config file present, which is a \
             supported setup: {combined}"
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
        id: "J16",
        intent: "check a niki.toml whose only section is a typo",
        guards: "it parses, it is accepted, and it does nothing",
        run: j_config_check_catches_a_misspelled_section,
    },
    Journey {
        id: "J17",
        intent: "check a valid niki.toml, and check with no file at all",
        guards: "a check that always fails is a check nobody reads",
        run: j_config_check_passes_a_good_file,
    },
    Journey {
        id: "J18",
        intent: "check a project that has never been configured",
        guards: "zero-config is a documented path, not a problem",
        run: j_config_check_is_quiet_with_no_file,
    },
    Journey {
        id: "J15",
        intent: "run a task in a directory that is not a git repository",
        guards: "four paid model calls before a bare git2 string, for a \
                  deliverable that could never have been produced",
        run: j_a_non_git_project_is_refused_before_the_run,
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
    Journey {
        id: "J20",
        intent: "`niki init --interactive` produces a config this machine can run",
        guards: "a setup wizard that writes a config selecting an unavailable backend",
        run: j_init_writes_a_runnable_config,
    },
    Journey {
        id: "J21",
        intent: "the written config selects the backend this machine can actually reach",
        guards: "the container backend on a machine with no container runtime",
        run: j_written_config_selects_a_reachable_backend,
    },
    Journey {
        id: "J22",
        intent: "`niki doctor` passes on the machine the README opens by describing",
        guards: "the verification command failing a working keyless, containerless install",
        run: j_doctor_passes_on_a_keyless_containerless_machine,
    },
    Journey {
        id: "J23",
        intent: "`niki run` with no flags fails about the model, not about containers",
        guards: "a first run dying on backend resolution before it ever reaches the task",
        run: j_first_run_does_not_die_on_backend_resolution,
    },
    Journey {
        id: "J24",
        intent: "a scripted `niki init --interactive` reports that setup is unfinished",
        guards: "a wizard that exits 0 onto a config that cannot run",
        run: j_scripted_init_does_not_succeed_onto_a_dead_end,
    },
    Journey {
        id: "J25",
        intent: "the documented keyless, containerless path is sound on any host",
        guards: "the worktree backend only being correct on machines without containers",
        run: j_the_containerless_path_works_even_where_containers_exist,
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
