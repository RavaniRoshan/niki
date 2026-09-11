use niki::config::NikiConfig;
use niki::knowledge::structural::{IndexStatus, Precision, build_index, index_root, open_index};
use std::fs;

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        "mod util;\n\nfn main() {\n    util::helper();\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/util.rs"),
        "pub fn helper() {}\n\npub fn other() {\n    helper();\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("app.py"),
        "def serve():\n    start()\n\ndef start():\n    pass\n",
    )
    .unwrap();
    tmp
}

#[test]
fn build_locate_callers_callees_boundary() {
    let tmp = fixture();
    let config = NikiConfig::default();
    let report = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    assert_eq!(report.manifest.status, IndexStatus::Complete);
    assert_eq!(report.reused, 0);
    assert_eq!(report.written, 3);
    assert!(index_root(tmp.path(), &config).join("SCOPE.md").is_file());

    let index = open_index(tmp.path(), &config).unwrap();
    assert_eq!(index.state_ref, "niki-task-test");

    let locs = index.locate_symbol("helper");
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].file, "src/util.rs");
    assert_eq!(locs[0].line, 1);

    // Callers: util::helper() in main + helper() in other.
    let callers = index.find_callers("helper");
    assert!(callers.len() >= 2, "expected 2+ callers, got {callers:?}");
    assert!(
        callers
            .iter()
            .any(|c| c.file == "src/main.rs" && c.caller == "main")
    );

    // Callees of main include helper.
    let callees = index.find_callees("main");
    assert!(callees.iter().any(|c| c.caller == "helper"));

    // Boundary: line inside `other` resolves to its span.
    let boundary = index.function_boundary("src/util.rs", 4).unwrap();
    assert_eq!(boundary.0, 3);
    assert!(matches!(boundary.2, Precision::Ast | Precision::Regex));

    // Reverse deps: util.rs is imported by main.rs (`mod util;`).
    let reverse = index.reverse_dependency_map();
    let importers = reverse.get("src/util.rs").cloned().unwrap_or_default();
    assert!(importers.contains(&"src/main.rs".to_string()));
}

#[test]
fn content_address_reuse_noop_rebuild() {
    let tmp = fixture();
    let config = NikiConfig::default();
    let first = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    let second = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    assert_eq!(second.written, 0);
    assert_eq!(second.reused, first.written);

    // Touching one file re-extracts only that unit.
    std::thread::sleep(std::time::Duration::from_millis(10));
    fs::write(tmp.path().join("app.py"), "def serve():\n    pass\n").unwrap();
    let third = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    assert_eq!(third.written, 1);
    assert_eq!(third.reused, 2);
}

#[test]
fn per_language_fallback_marking_and_honest_queries() {
    let tmp = fixture();
    // Non-UTF8 content → coverage-only unit, queries stay honest.
    fs::write(tmp.path().join("src/blob.rs"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
    let config = NikiConfig::default();
    let report = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    assert_eq!(report.manifest.status, IndexStatus::Partial);
    assert_eq!(report.manifest.units_coverage_only, 1);

    let index = open_index(tmp.path(), &config).unwrap();
    let unit = index
        .units
        .iter()
        .find(|u| u.path == "src/blob.rs")
        .unwrap();
    assert_eq!(unit.backend, "coverage");
    assert!(index.locate_symbol("anything").is_empty());
    assert!(index.function_boundary("src/blob.rs", 1).is_none());
    assert!(index.find_callers("missing").is_empty());
}

#[test]
fn deferred_coverage_on_empty_repo() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("notes.md"), "# nothing to index\n").unwrap();
    let config = NikiConfig::default();
    let report = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    assert_eq!(report.manifest.status, IndexStatus::Empty);
    let index = open_index(tmp.path(), &config).unwrap();
    assert!(index.units.is_empty());
}

#[test]
fn truncation_marks_partial() {
    let tmp = fixture();
    let mut config = NikiConfig::default();
    config.repo_intel.max_units = 2;
    let report = build_index(tmp.path(), &config, "niki-task-test").unwrap();
    assert!(report.manifest.truncated);
    assert_eq!(report.manifest.status, IndexStatus::Partial);
    assert_eq!(report.manifest.units_total, 2);
}
