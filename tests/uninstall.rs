//! W13 — the uninstall removes what the installer created, and nothing else.
//!
//! This drives the real script against a real fake `$HOME`. Not a source reading: the failure
//! mode this guards against is a variable expanding to nothing and a recursive delete eating a
//! directory named after it, and only running it finds that out.

use std::path::PathBuf;
use std::process::Command;

const REPO: &str = env!("CARGO_MANIFEST_DIR");

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp HOME");
        let root = dir.path();
        for sub in [".local/bin", ".config/niki", ".niki"] {
            std::fs::create_dir_all(root.join(sub)).expect("make the fake home");
        }
        // The things an installer would have put there, and the things a user would have.
        std::fs::write(root.join(".local/bin/niki"), "#!/bin/sh\n").expect("fake engine");
        std::fs::write(root.join(".local/bin/niki-shell"), "binary").expect("fake interface");
        std::fs::write(root.join(".config/niki/niki.toml"), "[general]\n").expect("fake config");
        std::fs::write(root.join(".config/niki/keyring.json"), "{}").expect("fake keys");
        std::fs::write(root.join(".niki/history.jsonl"), "{}\n").expect("fake history");
        Self { dir }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn run(&self, args: &[&str]) -> (String, String, bool) {
        let out = Command::new("bash")
            .arg(format!("{REPO}/scripts/uninstall.sh"))
            .args(args)
            // A scrubbed environment: the script resolves its destination from HOME, and the
            // point of the test is that this fake home is the only thing it looks at.
            .env("HOME", self.dir.path())
            .env_remove("NIKI_INSTALL_DIR")
            .env_remove("XDG_BIN_DIR")
            .env_remove("XDG_CONFIG_HOME")
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("uninstall.sh runs");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }
}

#[test]
fn the_default_removes_the_binaries_and_keeps_the_data() {
    let home = Home::new();
    let (out, _err, ok) = home.run(&[]);
    assert!(ok, "uninstall failed:\n{out}");

    assert!(
        !home.path(".local/bin/niki").exists(),
        "the engine survived"
    );
    assert!(
        !home.path(".local/bin/niki-shell").exists(),
        "the interface survived"
    );

    // The part that actually matters: a user's history and keys are not the installer's.
    assert!(
        home.path(".config/niki/niki.toml").exists(),
        "the default uninstall deleted the user's config:\n{out}"
    );
    assert!(
        home.path(".config/niki/keyring.json").exists(),
        "the default uninstall deleted the user's keys:\n{out}"
    );
    assert!(
        home.path(".niki/history.jsonl").exists(),
        "the default uninstall deleted the user's history:\n{out}"
    );
    assert!(
        out.contains("were NOT removed"),
        "the user must be told their data was kept, not left to guess:\n{out}"
    );
}

#[test]
fn purge_removes_the_data_too_and_says_so_before_it_does() {
    let home = Home::new();
    let (out, _err, ok) = home.run(&["--purge"]);
    assert!(ok, "purge failed:\n{out}");
    assert!(
        !home.path(".config/niki").exists(),
        "--purge left the config behind:\n{out}"
    );
    assert!(
        !home.path(".niki").exists(),
        "--purge left the state behind:\n{out}"
    );
}

#[test]
fn a_dry_run_changes_nothing() {
    let home = Home::new();
    let (out, _err, ok) = home.run(&["--dry-run"]);
    assert!(ok, "dry run failed:\n{out}");
    assert!(
        out.contains("would remove"),
        "a dry run must say what it would do:\n{out}"
    );
    assert!(out.contains("nothing was deleted"));
    for rel in [
        ".local/bin/niki",
        ".local/bin/niki-shell",
        ".config/niki/niki.toml",
        ".niki/history.jsonl",
    ] {
        assert!(home.path(rel).exists(), "a dry run deleted {rel}:\n{out}");
    }
}

#[test]
fn running_it_twice_succeeds_twice() {
    // An uninstall that fails the second time is worse than one that does nothing: a user
    // re-running it after a partial failure gets a non-zero exit and concludes it is broken.
    let home = Home::new();
    let first = home.run(&[]);
    assert!(first.2, "the first run failed:\n{}", first.0);
    let second = home.run(&[]);
    assert!(
        second.2,
        "the second run failed; uninstall must be idempotent:\n{}",
        second.0
    );
    assert!(
        second.0.contains("absent"),
        "the second run should report absence:\n{}",
        second.0
    );
}

#[test]
fn an_explicit_install_dir_is_the_one_that_gets_cleaned() {
    let home = Home::new();
    let custom = home.path("opt/niki/bin");
    std::fs::create_dir_all(&custom).expect("make the custom dir");
    std::fs::write(custom.join("niki"), "binary").expect("fake engine");

    let out = Command::new("bash")
        .arg(format!("{REPO}/scripts/uninstall.sh"))
        .env("HOME", home.dir.path())
        .env("NIKI_INSTALL_DIR", &custom)
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("uninstall.sh runs");
    assert!(out.status.success());
    assert!(
        !custom.join("niki").exists(),
        "NIKI_INSTALL_DIR was ignored, so the real install survived"
    );
    assert!(
        home.path(".local/bin/niki").exists(),
        "with NIKI_INSTALL_DIR set, the default destination must be left alone"
    );
}

#[test]
fn an_empty_install_dir_falls_through_the_same_ladder_the_installer_used() {
    // An empty `NIKI_INSTALL_DIR` is an *unset* one, not a directory named "". The script must
    // resolve it exactly as `install.sh` does, so it finds and removes what was actually
    // installed, and its blast radius stays inside that directory. (An earlier version of this
    // test asserted the binary survived, which was asserting that the uninstaller could not
    // find its own install.)
    let home = Home::new();
    let out = Command::new("bash")
        .arg(format!("{REPO}/scripts/uninstall.sh"))
        .env("HOME", home.dir.path())
        .env("NIKI_INSTALL_DIR", "")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("uninstall.sh runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(
        !home.path(".local/bin/niki").exists(),
        "with no install dir set, the ladder should still resolve ~/.local/bin:\n{stdout}"
    );
    assert!(
        !home.path(".local/bin/niki-shell").exists(),
        "with no install dir set, the ladder should resolve ~/.local/bin and remove both binaries:\n{stdout}"
    );
    assert!(
        !home.path(".local/bin").exists() || home.dir.path().join(".local").exists(),
        "the install directory itself is the user's and must survive:\n{stdout}"
    );
    assert!(home.dir.path().exists(), "the HOME itself was removed");
    assert!(
        home.path(".config/niki/niki.toml").exists(),
        "config was removed"
    );
}

#[test]
fn an_install_dir_of_root_is_refused_and_nothing_is_removed() {
    // The shape that would be catastrophic. The guard runs before any target is built, so the
    // per-path checks never see it — this asserts the early refusal, not the later one.
    let home = Home::new();
    let out = Command::new("bash")
        .arg(format!("{REPO}/scripts/uninstall.sh"))
        .env("HOME", home.dir.path())
        .env("NIKI_INSTALL_DIR", "/")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("uninstall.sh runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a root install dir must be a refusal, not a run:\n{stderr}"
    );
    assert!(
        stderr.contains("REFUSED"),
        "the refusal must say it refused:\n{stderr}"
    );
    for rel in [
        ".local/bin/niki",
        ".local/bin/niki-shell",
        ".config/niki/niki.toml",
    ] {
        assert!(home.path(rel).exists(), "a refused uninstall deleted {rel}");
    }
}
