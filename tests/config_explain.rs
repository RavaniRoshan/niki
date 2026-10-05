//! W2 — every effective configuration value says where it came from.
//!
//! Layering is invisible until it is wrong: three files, an environment and a pile of defaults,
//! and a setting that is not doing what the file says has no symptom other than being wrong.
//! `niki config check` says whether a file *parses*. This says what the loader used.
//!
//! Every assertion here is about the **source label**, because a wrong value with a right label
//! is the failure that costs a user an afternoon.

use std::path::{Path, PathBuf};
use std::process::Command;

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    fn new(project_toml: Option<&str>, user_toml: Option<&str>) -> Self {
        let dir = tempfile::tempdir().expect("temp home");
        let root = dir.path();
        std::fs::create_dir_all(root.join("proj")).expect("project dir");
        if let Some(body) = project_toml {
            std::fs::write(root.join("proj/niki.toml"), body).expect("write project toml");
        }
        if let Some(body) = user_toml {
            let p = root.join(".config/niki/niki.toml");
            std::fs::create_dir_all(p.parent().expect("parent")).expect("make config dir");
            std::fs::write(p, body).expect("write user toml");
        }
        Self { dir }
    }

    fn run(&self, envs: &[(&str, &str)]) -> String {
        let mut cmd = Command::new(niki_bin());
        cmd.args(["config", "explain"])
            .current_dir(self.dir.path().join("proj"));
        cmd.env("HOME", self.dir.path());
        // Scrub every variable this report consults, so a value the developer happens to have
        // exported cannot make an assertion pass or fail for the wrong reason.
        for v in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_MODEL",
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "OPENAI_MODEL",
        ] {
            cmd.env_remove(v);
        }
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("niki runs");
        assert!(
            out.status.success(),
            "config explain failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// The line for one setting, as printed.
fn line_for<'a>(out: &'a str, setting: &str) -> &'a str {
    out.lines()
        .find(|l| l.trim_start().starts_with(setting))
        .unwrap_or_else(|| panic!("no line for {setting} in:\n{out}"))
}

#[test]
fn a_value_set_in_the_project_file_names_that_file() {
    let env = Env::new(
        Some("[general]\nspend_cap_usd = 2.50\n[docker]\nbackend = \"worktree\"\n"),
        None,
    );
    let out = env.run(&[]);
    let line = line_for(&out, "general.spend_cap_usd");
    assert!(line.contains("2.5"), "the value is wrong:\n{line}");
    assert!(
        line.contains("proj/niki.toml"),
        "the source must name the file that set it:\n{line}"
    );
    assert!(
        line_for(&out, "docker.backend").contains("worktree"),
        "a second setting in the same file must also be attributed to it:\n{out}"
    );
}

#[test]
fn an_environment_variable_beats_the_file_and_says_so() {
    // `ANTHROPIC_MODEL` is one the loader really reads (`apply_env_lookup`). The first version
    // of this test used an invented `NIKI_CODER_MODEL` and asserted a precedence the engine
    // does not implement — it "passed" only because the report listed that variable.
    let env = Env::new(
        Some("[providers.anthropic]\ndefault_model = \"from-the-file\"\n"),
        None,
    );
    let out = env.run(&[("ANTHROPIC_MODEL", "from-the-env")]);
    let line = line_for(&out, "providers.anthropic.default_model");
    assert!(
        line.contains("from-the-env"),
        "the env did not win:\n{line}"
    );
    assert!(
        line.contains("environment ANTHROPIC_MODEL"),
        "the source must name the variable that won:\n{line}"
    );
}

#[test]
fn a_secret_is_reported_as_set_and_never_printed() {
    // A diagnostic that prints an API key writes it to the terminal, the scrollback buffer and
    // whatever CI captured. The report says "set"; it never says what.
    let env = Env::new(
        Some("[providers.openai]\napi_key = \"sk-from-the-file\"\n"),
        None,
    );
    let out = env.run(&[("ANTHROPIC_API_KEY", "sk-from-the-env")]);
    assert!(
        !out.contains("sk-from-the-env"),
        "the report printed a key from the environment:\n{out}"
    );
    assert!(
        !out.contains("sk-from-the-file"),
        "the report printed a key from a file:\n{out}"
    );
    let line = line_for(&out, "providers.openai.api_key");
    assert!(
        line.contains("(set)"),
        "a set key must still be reported as set:\n{line}"
    );
    assert!(
        line.contains("niki.toml"),
        "the source must still be reported, secret or not:\n{line}"
    );
}

#[test]
fn a_value_nothing_sets_is_labelled_a_built_in_default() {
    let env = Env::new(None, None);
    let out = env.run(&[]);
    let line = line_for(&out, "general.max_revision_rounds");
    assert!(
        line.contains("built-in default"),
        "an unset value must say where it came from:\n{line}"
    );
    assert!(
        line.contains('3'),
        "the documented default changed:\n{line}"
    );
}

#[test]
fn the_project_file_beats_the_user_file() {
    let env = Env::new(
        Some("[general]\noutput_dir = \"from-project\"\n"),
        Some("[general]\noutput_dir = \"from-user\"\nspend_cap_usd = 9.0\n"),
    );
    let out = env.run(&[]);
    let project_line = line_for(&out, "general.output_dir");
    assert!(
        project_line.contains("from-project") && project_line.contains("proj/niki.toml"),
        "the project file must win:\n{project_line}"
    );
    // …and the user file still supplies what the project file is silent about.
    let user_line = line_for(&out, "general.spend_cap_usd");
    assert!(
        user_line.contains("9") && user_line.contains(".config/niki/niki.toml"),
        "a value only the user file sets must be attributed to it:\n{user_line}"
    );
}

#[test]
fn setting_one_section_does_not_claim_another() {
    // A file that sets `general.output_dir` says nothing about `docker.backend`, and must not
    // be reported as the source for it. Attributing by file rather than by key is the version
    // of this bug that makes `explain` actively misleading.
    let env = Env::new(Some("[general]\noutput_dir = \"only-this\"\n"), None);
    let out = env.run(&[]);
    assert!(
        line_for(&out, "general.output_dir").contains("proj/niki.toml"),
        "the set value is misattributed:\n{out}"
    );
    assert!(
        line_for(&out, "docker.backend").contains("built-in default"),
        "an unset value was attributed to a file that does not mention it:\n{out}"
    );
    assert!(
        line_for(&out, "permissions.mode").contains("built-in default"),
        "an unset value was attributed to a file that does not mention it:\n{out}"
    );
}

#[test]
fn the_report_names_both_files_and_states_the_precedence() {
    let env = Env::new(Some("[general]\n"), Some("[general]\n"));
    let out = env.run(&[]);
    assert!(
        out.contains("project file:"),
        "no project file named:\n{out}"
    );
    assert!(out.contains("user file:"), "no user file named:\n{out}");
    assert!(
        out.contains("environment, then project file, then user file, then built-in default"),
        "the precedence must be stated, not implied:\n{out}"
    );
}

#[test]
fn a_value_that_is_a_number_or_a_bool_prints_as_written() {
    // `spend_cap_usd = 2.50` must not come back as `"2.50"` (a string), and a boolean must not
    // come back quoted. A label that lies about the type is worse than no label.
    let env = Env::new(
        Some("[general]\nspend_cap_usd = 2.50\nmax_diff_lines = 250\n"),
        None,
    );
    let out = env.run(&[]);
    let line = line_for(&out, "general.spend_cap_usd");
    assert!(
        !line.contains("\"2.5"),
        "a number was rendered as a string:\n{line}"
    );
    assert!(line_for(&out, "general.max_diff_lines").contains("250"));
}

/// The scan this suite relies on must actually find lines, or every test above would pass
/// against an empty report.
#[test]
fn the_report_is_not_empty() {
    let env = Env::new(None, None);
    let out = env.run(&[]);
    let tracked = out
        .lines()
        .filter(|l| l.contains("built-in default") || l.contains("niki.toml"))
        .count();
    assert!(
        tracked >= 8,
        "the report printed {tracked} attributed rows; a scan that finds almost nothing is not \
         scanning the settings it claims to:\n{out}"
    );
    assert!(
        Path::new(&env.dir.path().join("proj")).exists(),
        "the project directory vanished"
    );
}
