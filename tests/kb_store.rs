use niki::cli::architecture::build_architecture;
use niki::config::NikiConfig;
use niki::knowledge::kb::{Authority, Sidecar, read_snapshot_stamp};
use std::fs;

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    fs::write(root.join("web/app.ts"), "export const x = 1;\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    tmp
}

#[test]
fn architecture_build_writes_stamped_kb() {
    let tmp = fixture();
    let summary = build_architecture(tmp.path()).unwrap();

    let kb = tmp.path().join(".niki").join("kb");
    // Every Markdown file starts with the KB_SNAPSHOT stamp.
    let mut md_files = vec![kb.join("architecture.md"), kb.join("history.md")];
    md_files.extend(
        fs::read_dir(kb.join("entities"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path()),
    );
    assert!(md_files.len() >= 3, "expected entities, got {md_files:?}");
    for path in &md_files {
        let stamp = read_snapshot_stamp(path)
            .unwrap_or_else(|| panic!("missing KB_SNAPSHOT stamp in {}", path.display()));
        assert!(stamp.starts_with("<!-- KB_SNAPSHOT:"));
    }

    // Sidecar roundtrip: provenance fields survive the write.
    let deps: Sidecar<serde_json::Value> =
        serde_json::from_str(&fs::read_to_string(kb.join("dependencies.json")).unwrap()).unwrap();
    assert_eq!(deps.generated_by, "architecture-build");
    assert!(!deps.state_ref.is_empty());
    assert_eq!(
        serde_json::from_value::<Authority>(serde_json::json!("inferred")).unwrap(),
        Authority::Inferred
    );

    // Manifest lists everything written (the atomic commit point).
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(kb.join("manifest.json")).unwrap()).unwrap();
    let files = manifest.get("files").and_then(|f| f.as_array()).unwrap();
    assert!(files.iter().any(|f| f == "architecture.md"));
    assert!(files.iter().any(|f| f == "dependencies.json"));

    // Content checks: languages, entry points, entities.
    let arch = fs::read_to_string(kb.join("architecture.md")).unwrap();
    assert!(arch.contains("Rust"));
    assert!(arch.contains("src/main.rs"));
    assert!(summary.entities >= 2);
    assert!(summary.learnings_included == 0);
}

#[test]
fn architecture_build_includes_learnings_and_is_idempotent() {
    let tmp = fixture();
    let config = NikiConfig::default();
    niki::knowledge::learnings::append_learning(
        tmp.path(),
        &config,
        &niki::knowledge::learnings::LearningEntry::new(
            "verification_failure",
            "niki-task-test",
            "tester",
            "authoritative",
            "flaky test in web/app".to_string(),
        ),
    )
    .unwrap();

    let first = build_architecture(tmp.path()).unwrap();
    assert_eq!(first.learnings_included, 1);
    let arch = fs::read_to_string(tmp.path().join(".niki/kb/architecture.md")).unwrap();
    assert!(arch.contains("flaky test in web/app"));

    // Rebuild from scratch: same file set, no duplication.
    let second = build_architecture(tmp.path()).unwrap();
    assert_eq!(first.files_written, second.files_written);
}

#[test]
fn architecture_build_doc_header_roundtrip() {
    // The stamp header survives a rebuild (headers are rewritten, not piled).
    let tmp = fixture();
    build_architecture(tmp.path()).unwrap();
    build_architecture(tmp.path()).unwrap();
    let arch = fs::read_to_string(tmp.path().join(".niki/kb/architecture.md")).unwrap();
    assert_eq!(
        arch.lines()
            .filter(|l| l.starts_with("<!-- KB_SNAPSHOT:"))
            .count(),
        1
    );
}
