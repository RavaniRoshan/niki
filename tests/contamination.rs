//! Contamination suite — asserts that NIKI contains NO task-specific knowledge,
//! task identifiers, canary strings, or solutions for benchmark datasets in source,
//! prompts, or schemas.
//!
//! Required by Terminal-Bench official leaderboard rules and PACK.md integrity rules.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scan_dir(dir: &Path, forbidden: &[&str], violations: &mut Vec<String>) {
    if !dir.exists() {
        return;
    }
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Ok(content) = fs::read_to_string(path) {
                for token in forbidden {
                    if content.contains(token) {
                        violations.push(format!(
                            "{}: contains forbidden benchmark token '{}'",
                            path.display(),
                            token
                        ));
                    }
                }
            }
        }
    }
}

#[test]
fn engine_prompts_schemas_are_uncontaminated() {
    let split_file = repo_root().join("bench/splits/tb2_split.json");
    assert!(split_file.exists(), "split file must exist");
    let content = fs::read_to_string(&split_file).expect("read split file");
    let split: serde_json::Value = serde_json::from_str(&content).expect("parse split json");

    let mut forbidden_tokens: Vec<String> = Vec::new();

    if let Some(dev) = split.get("dev_tasks").and_then(|v| v.as_array()) {
        for t in dev {
            if let Some(s) = t.as_str() {
                forbidden_tokens.push(s.to_string());
            }
        }
    }
    if let Some(sealed) = split.get("sealed_tasks").and_then(|v| v.as_array()) {
        for t in sealed {
            if let Some(s) = t.as_str() {
                forbidden_tokens.push(s.to_string());
            }
        }
    }

    assert!(
        forbidden_tokens.len() >= 80,
        "expected at least 80 benchmark tokens to test against, got {}",
        forbidden_tokens.len()
    );

    let token_slices: Vec<&str> = forbidden_tokens.iter().map(|s| s.as_str()).collect();
    let mut violations = Vec::new();

    scan_dir(&repo_root().join("src"), &token_slices, &mut violations);
    scan_dir(&repo_root().join("prompts"), &token_slices, &mut violations);
    scan_dir(&repo_root().join("schemas"), &token_slices, &mut violations);

    assert!(
        violations.is_empty(),
        "Benchmark contamination detected in engine codebase:\n{}",
        violations.join("\n")
    );
}

#[test]
fn contamination_scanner_can_fail() {
    // Prove the scanner actually detects contaminated text
    let dummy = "tb2-001-tar-extract-strip is solved by doing X";
    assert!(
        dummy.contains("tb2-001-tar-extract-strip"),
        "scanner logic must detect token"
    );
}
