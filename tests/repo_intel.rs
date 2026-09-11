use niki::config::NikiConfig;
use niki::repo_intel::build_manifest;
use std::fs;

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    fs::write(root.join("tests/cli.rs"), "#[test]\nfn t() {}\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    fs::write(root.join("niki.toml"), "[general]\n").unwrap();
    fs::write(root.join("target/debug/blob"), "generated").unwrap();
    tmp
}

#[test]
fn manifest_invariants_on_rust_fixture() {
    let tmp = fixture();
    let manifest = build_manifest(tmp.path(), &NikiConfig::default());
    assert!(manifest.languages.contains(&"Rust".to_string()));
    assert!(manifest.entry_points.contains(&"src/main.rs".to_string()));
    assert!(manifest.entry_points.contains(&"src/lib.rs".to_string()));
    assert!(manifest.test_paths.iter().any(|p| p == "tests/cli.rs"));
    assert!(manifest.config_paths.contains(&"niki.toml".to_string()));
    assert!(manifest.config_paths.contains(&"Cargo.toml".to_string()));
    assert!(manifest.build_files.contains(&"Cargo.toml".to_string()));
    assert!(manifest.vendor_dirs.contains(&"target".to_string()));
    // 6 fixture files minus the vendored blob.
    assert_eq!(manifest.files, 5);
    assert!(!manifest.truncated);
}

#[test]
fn manifest_language_detection_multi() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.py"), "def f():\n    pass\n").unwrap();
    fs::write(tmp.path().join("b.go"), "package main\n").unwrap();
    fs::write(tmp.path().join("notes.md"), "# hi\n").unwrap();
    let manifest = build_manifest(tmp.path(), &NikiConfig::default());
    assert!(manifest.languages.contains(&"Python".to_string()));
    assert!(manifest.languages.contains(&"Go".to_string()));
    assert!(!manifest.languages.iter().any(|l| l == "Markdown"));
}

#[test]
fn manifest_risk_signal_from_config_denylist() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/auth_middleware.rs"), "").unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    // Baseline: risk signals only cover test/config/build paths.
    let base = build_manifest(tmp.path(), &NikiConfig::default());
    assert!(base.risk_signals.is_empty());
    // A custom denylist hitting a build file surfaces a signal.
    let mut config = NikiConfig::default();
    config.risk.denylist_patterns = vec!["cargo".to_string()];
    let flagged = build_manifest(tmp.path(), &config);
    assert_eq!(flagged.risk_signals.len(), 1);
    assert_eq!(flagged.risk_signals[0].rule, "denylist");
}

#[test]
fn manifest_byte_bounded_and_truncating() {
    let tmp = fixture();
    // A 2 MB generated file must not break or dominate the manifest.
    fs::write(tmp.path().join("big.log"), "x".repeat(2_000_000)).unwrap();
    let manifest = build_manifest(tmp.path(), &NikiConfig::default());
    assert_eq!(manifest.files, 6);
    assert!(!manifest.truncated);

    let mut config = NikiConfig::default();
    config.repo_intel.max_units = 3;
    let capped = build_manifest(tmp.path(), &config);
    assert!(capped.truncated);
    assert_eq!(capped.files, 3);
}
