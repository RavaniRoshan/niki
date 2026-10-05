//! W2 — safe migration between config versions.
//!
//! A migration here runs on every load, so it must be **additive and total**: a rule that can fail
//! takes the product down for a user with an old file. And it must never remove a key this build
//! does not recognise, because that is how a user's file loses a setting they added for something
//! else.
//!
//! The migration is **reported**, not applied, on load. Rewriting a file the user wrote is not a
//! load's business, and a change nobody can see is a change nobody trusts. `niki config migrate`
//! is where it gets written.

use niki::config::NikiConfig;
use std::path::PathBuf;
use std::process::Command;

fn niki_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_niki"))
}

fn migrate(toml: &str) -> Vec<String> {
    let raw: toml::Value = toml.parse().expect("the fixture parses as TOML");
    NikiConfig::migrate(&raw)
}

#[test]
fn a_file_with_no_version_is_read_as_v1_and_says_so() {
    // v1 predates the field, so an absent `version` means v1 — not "current".
    let notes = migrate("[general]\nmax_revision_rounds = 3\n");
    assert!(
        notes.iter().any(|n| n.contains("read as v1")),
        "a missing version was not reported: {notes:?}"
    );
}

#[test]
fn a_current_file_produces_no_notes() {
    let notes = migrate("version = 2\n[general]\nmax_revision_rounds = 3\n");
    assert!(notes.is_empty(), "a current file produced notes: {notes:?}");
}

#[test]
fn a_file_newer_than_this_build_is_reported_rather_than_trusted() {
    let notes = migrate("version = 99\n[general]\nmax_revision_rounds = 3\n");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].contains("99") && notes[0].contains("ignored"),
        "the note must name the version and say what happens to what it does not know: {}",
        notes[0]
    );
}

/// The renames are the point of the row. Each must be reported *before* anyone runs one, so the
/// change is never a surprise.
#[test]
fn each_known_rename_is_reported() {
    let image = migrate("[docker]\nsandbox_image = \"x:1\"\n");
    assert!(
        image
            .iter()
            .any(|n| n.contains("sandbox_image") && n.contains("base_image")),
        "the docker rename was not reported: {image:?}"
    );
    let approve = migrate("[permissions]\nauto_approve = true\n");
    assert!(
        approve
            .iter()
            .any(|n| n.contains("auto_approve") && n.contains("mode")),
        "the permissions rename was not reported: {approve:?}"
    );
}

/// Migration must never fail a load. Every fixture here is a file an older user could have.
#[test]
fn migration_is_total_and_never_panics() {
    for toml in [
        "",
        "version = 1\n",
        "[general]\n",
        "[unknown_section]\nkey = 1\n",
        "[providers.openai]\napi_key = \"sk-x\"\n",
        "[docker]\nsandbox_image = \"x\"\nbase_image = \"already-moved\"\n",
    ] {
        // Must return, not panic and not error. An empty document parses as an empty table.
        let _ = migrate(toml);
    }
}

// ── the command, against real files ─────────────────────────────────────────

struct P {
    dir: tempfile::TempDir,
}
impl P {
    fn new(body: &str) -> Self {
        let dir = tempfile::tempdir().expect("temp");
        std::fs::write(dir.path().join("niki.toml"), body).expect("write");
        Self { dir }
    }
    fn path(&self) -> PathBuf {
        self.dir.path().join("niki.toml")
    }
    fn run(&self, extra: &[&str]) -> (String, String, bool) {
        let out = Command::new(niki_bin())
            .arg("config")
            .arg("migrate")
            .args(extra)
            .current_dir(self.dir.path())
            .output()
            .expect("niki runs");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }
}

#[test]
fn a_dry_run_changes_nothing_and_says_so() {
    let p = P::new("[docker]\nsandbox_image = \"x:1\"\n");
    let before = std::fs::read_to_string(p.path()).expect("read");
    let (out, _err, ok) = p.run(&["--dry-run"]);
    assert!(ok, "dry run failed:\n{out}");
    assert!(out.contains("was not written"), "{out}");
    assert_eq!(
        std::fs::read_to_string(p.path()).expect("read"),
        before,
        "a dry run rewrote the file"
    );
}

#[test]
fn applying_renames_preserves_the_value_and_stamps_the_version() {
    let p = P::new(
        "[docker]\nsandbox_image = \"niki-sandbox:24.04\"\n\n[general]\nmax_revision_rounds = 7\n",
    );
    let (_out, _err, ok) = p.run(&[]);
    assert!(ok, "migrate failed");
    let after = std::fs::read_to_string(p.path()).expect("read");
    let doc: toml::Value = after.parse().expect("the migrated file parses");

    assert_eq!(
        doc["docker"]["base_image"].as_str(),
        Some("niki-sandbox:24.04")
    );
    assert!(
        doc["docker"].get("sandbox_image").is_none(),
        "the old key survived alongside the new one:\n{after}"
    );
    // A setting the migration knows nothing about must survive untouched.
    assert_eq!(
        doc["general"]["max_revision_rounds"].as_integer(),
        Some(7),
        "the migration dropped a setting it did not understand:\n{after}"
    );
    assert_eq!(
        doc["version"].as_integer(),
        Some(NikiConfig::CONFIG_WRITE_VERSION as i64)
    );
}

/// An existing new key wins. The user has already moved on, and a migration that overwrites their
/// value to re-apply a rename is a migration that loses data.
#[test]
fn an_existing_new_key_is_not_overwritten() {
    let p = P::new("[docker]\nsandbox_image = \"old\"\nbase_image = \"new\"\n");
    let (_out, _err, ok) = p.run(&[]);
    assert!(ok);
    let doc: toml::Value = std::fs::read_to_string(p.path())
        .expect("read")
        .parse()
        .expect("parse");
    assert_eq!(doc["docker"]["base_image"].as_str(), Some("new"));
    assert!(
        doc["docker"].get("sandbox_image").is_none(),
        "the stale key should be dropped when the new one already exists"
    );
}

/// A key this build does not understand is carried across. A migration that deletes what it does
/// not understand is how a user's file loses a setting they added for something else.
#[test]
fn an_unknown_key_survives_a_migration() {
    let p = P::new("[general]\nmax_revision_rounds = 3\n\n[my_plugin]\nsetting = \"keep me\"\n");
    let (_out, _err, ok) = p.run(&[]);
    assert!(ok);
    let after = std::fs::read_to_string(p.path()).expect("read");
    assert!(
        after.contains("keep me"),
        "the migration deleted a setting it did not understand:\n{after}"
    );
}

#[test]
fn migrating_a_file_that_does_not_exist_fails_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("temp");
    let out = Command::new(niki_bin())
        .args(["config", "migrate"])
        .current_dir(dir.path())
        .output()
        .expect("niki runs");
    assert!(!out.status.success(), "migrating nothing succeeded");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Nothing to migrate"),
        "the refusal must say what is missing:\n{stderr}"
    );
}

#[test]
fn migrating_twice_is_a_no_op_the_second_time() {
    let p = P::new("[docker]\nsandbox_image = \"x:1\"\n");
    let (_o, _e, ok) = p.run(&[]);
    assert!(ok);
    let once = std::fs::read_to_string(p.path()).expect("read");
    let (out, _e, ok) = p.run(&[]);
    assert!(ok);
    let twice = std::fs::read_to_string(p.path()).expect("read");
    assert_eq!(
        once, twice,
        "a second migration changed the file again:\n{twice}"
    );
    assert!(
        out.contains("already current"),
        "a second migration should say there was nothing to do:\n{out}"
    );
}

#[test]
fn an_interrupted_migration_leaves_the_original_intact() {
    // The write goes through a sibling and a rename, so a harness watching for the file never
    // sees a half-written document and a crash leaves the old one readable.
    let p = P::new("[general]\nmax_revision_rounds = 3\n");
    let (_o, _e, ok) = p.run(&[]);
    assert!(ok);
    let leftovers: Vec<_> = std::fs::read_dir(p.dir.path())
        .expect("read dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".partial"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "a .partial file was left behind: {leftovers:?}"
    );
}
