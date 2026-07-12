use niki::config::NikiConfig;
use niki::knowledge::indexer::index_project;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_knowledge_indexes_project_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("index.js"), "console.log('hi');").unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{ "name": "x", "dependencies": { "express": "^4.0.0" } }"#,
    )
    .unwrap();

    let knowledge = index_project(dir.path(), &NikiConfig::default())
        .await
        .expect("index project");

    let rendered = knowledge.render();
    assert!(rendered.contains("index.js"), "file tree should list index.js");
    assert!(
        rendered.contains("JavaScript"),
        "JavaScript should be detected as a language"
    );
    assert!(
        rendered.contains("express"),
        "package.json dependencies should be indexed"
    );
}
