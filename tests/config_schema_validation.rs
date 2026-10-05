//! W2 — a `niki.toml` is validated against the schema the engine generates.
//!
//! The failure this closes is quiet and total: `[general] spend_cap = 1` parses perfectly, serde
//! accepts it because every field carries `#[serde(default)]`, and the setting does nothing. A
//! user can run with defaults for weeks without the program saying a word.
//!
//! The validator reads `NikiConfig::config_schema_json()` — the same document `niki config schema`
//! hands an editor — so a key added to one is automatically known to the other. A validator with
//! its own hand-written key list is the version that drifts and then lies.

use niki::config::NikiConfig;
use std::path::PathBuf;

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

fn problems(toml: &str) -> Vec<String> {
    let raw: toml::Value = toml.parse().expect("the fixture parses as TOML");
    NikiConfig::validate_against_schema(&raw)
}

#[test]
fn a_correct_file_reports_nothing() {
    let found = problems(
        r#"
[general]
max_revision_rounds = 3
output_dir = ".niki"
spend_cap_usd = 2.5

[docker]
backend = "worktree"
memory_limit = "2g"

[session]
enabled = true
"#,
    );
    assert!(found.is_empty(), "a valid file was rejected: {found:#?}");
}

#[test]
fn a_misspelled_key_is_named_and_says_it_is_ignored() {
    let found = problems("[general]\nspend_cap = 1\n");
    assert_eq!(found.len(), 1, "expected one problem, got {found:#?}");
    assert!(
        found[0].contains("general.spend_cap"),
        "the message must name the key: {}",
        found[0]
    );
    assert!(
        found[0].contains("ignored"),
        "the message must say what happens to it, which is the whole point: {}",
        found[0]
    );
}

#[test]
fn a_wrong_scalar_type_is_reported_against_the_schema() {
    let found = problems("[general]\nmax_revision_rounds = \"three\"\n");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("integer") && found[0].contains("string"),
        "the message must state the schema type and the file's type: {}",
        found[0]
    );
}

#[test]
fn a_value_outside_an_enum_is_reported_with_the_allowed_set() {
    let found = problems("[docker]\nbackend = \"pods\"\n");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("docker") && found[0].contains("worktree"),
        "the message must list what is allowed: {}",
        found[0]
    );
}

#[test]
fn a_misspelled_key_inside_a_named_provider_is_caught_too() {
    // `[providers.openai]` is a map of objects, described by `additionalProperties` rather than
    // `properties`. A scanner that only walks `properties` sees nothing inside it.
    let found = problems("[providers.openai]\nbase_urll = \"http://x\"\n");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("providers.openai.base_urll"),
        "the message must name the nested key: {}",
        found[0]
    );
}

#[test]
fn an_integer_satisfies_a_number_schema() {
    // JSON-Schema says an integer is a number. Rejecting `cpu_limit = 2` because it has no decimal
    // point would be a validator that disagrees with the schema it claims to enforce.
    assert!(
        problems("[docker]\ncpu_limit = 2\n").is_empty(),
        "an integer was rejected for a `number` field"
    );
}

/// The validator must not become a second opinion about the schema. If the schema gains a key,
/// the validator must accept it with no code change — and if it loses one, the validator must
/// reject it. That is what "the schema is the source of truth" has to mean in practice.
#[test]
fn the_validator_reads_the_generated_schema_rather_than_its_own_list() {
    let schema: serde_json::Value =
        serde_json::from_str(&NikiConfig::config_schema_json()).expect("the schema parses");
    let declared = schema["properties"]["general"]["properties"]
        .as_object()
        .expect("general has properties")
        .clone();
    assert!(
        !declared.is_empty(),
        "the schema declares no general properties; the validator has nothing to check against"
    );

    for (key, spec) in declared.iter() {
        // A value of the right type per the schema. The first version of this test used `= 1` for
        // every key and the validator correctly complained that `output_dir` is a string — the
        // test's fixture was wrong, not the check.
        let literal = match spec.get("type").and_then(|t| t.as_str()) {
            Some("string") => "\"x\"",
            Some("boolean") => "true",
            Some("array") => "[]",
            _ => "1",
        };
        let found = problems(&format!("[general]\n{key} = {literal}\n"));
        assert!(
            found.is_empty(),
            "`{key}` is declared in the generated schema as {} but the validator rejected a \
             conforming value: {found:?}",
            spec.get("type").and_then(|t| t.as_str()).unwrap_or("?")
        );
    }
}

/// The gate has to run on the real command, not only on the library, or `niki config check` could
/// stop calling it without anything noticing.
#[test]
fn config_check_refuses_a_misspelled_key_and_says_where() {
    let dir = tempfile::tempdir().expect("temp project");
    let path = dir.path().join("niki.toml");
    std::fs::write(
        &path,
        "[general]\nspend_cap_usd = 1.0\nmax_revision_rounds = 3\n",
    )
    .expect("write");

    assert!(
        NikiConfig::load_file_only(&path).is_ok(),
        "a valid file must pass"
    );

    std::fs::write(&path, "[general]\nspend_cap = 1.0\n").expect("write the typo");
    let err = NikiConfig::load_file_only(&path).expect_err("a misspelled key must be refused");
    assert!(err.contains("general.spend_cap"), "{err}");
    assert!(
        err.contains(path.to_str().expect("utf-8 path")),
        "the message must name the file: {err}"
    );

    let out = std::process::Command::new(niki_bin())
        .args(["config", "check"])
        .current_dir(dir.path())
        .output()
        .expect("niki runs");
    assert!(
        !out.status.success(),
        "`niki config check` exited 0 on a file with a key it does not read"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("general.spend_cap"),
        "the command's own output must name the key:\n{stdout}"
    );
}

/// Every section the schema declares now lists its fields, so "check nothing" is no longer an
/// escape hatch. This pins that.
///
/// It started as the opposite test — a bare `"type": "object"` section must not have its contents
/// rejected — and that became wrong the moment the schema was completed. A test asserting a
/// property the design has deliberately moved past is a test that will be "fixed" by making the
/// code worse.
#[test]
fn no_section_declared_by_the_schema_is_left_unchecked() {
    let schema: serde_json::Value =
        serde_json::from_str(&NikiConfig::config_schema_json()).expect("the schema parses");
    let props = schema["properties"].as_object().expect("properties");

    let mut bare: Vec<&String> = Vec::new();
    for (name, decl) in props {
        if decl.get("additionalProperties").is_some() {
            continue;
        }
        match decl.get("properties") {
            Some(p) if !p.as_object().expect("properties is an object").is_empty() => {}
            _ => bare.push(name),
        }
    }
    assert!(
        bare.is_empty(),
        "these sections are declared without their fields, so a typo inside them is invisible: \
         {bare:?}"
    );
}

/// …and that means a typo inside one of them is caught. `ui` is the section a user is most
/// likely to hand-edit.
#[test]
fn a_typo_inside_a_ui_section_is_caught() {
    let found = problems("[ui]\ntheme = \"niki\"\ntranscrpt = true\n");
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("ui.transcrpt"),
        "the message must name the key: {}",
        found[0]
    );
}
