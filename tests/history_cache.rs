use niki::config::NikiConfig;
use niki::knowledge::history::mine_history;
use std::path::Path;

fn commit(repo: &git2::Repository, path: &str, content: &str, message: &str) {
    let workdir = repo.workdir().unwrap().to_path_buf();
    if let Some(parent) = workdir.join(path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(workdir.join(path), content).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = git2::Signature::now("t", "t@t").unwrap();
    let parent = repo
        .revparse_single("HEAD")
        .ok()
        .and_then(|o| o.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

fn history_learnings(project: &Path) -> String {
    std::fs::read_to_string(project.join(".niki/history/learnings.jsonl")).unwrap_or_default()
}

#[test]
fn cache_filters_keywords_and_large_diffs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(tmp.path()).unwrap();
    commit(&repo, "src/a.rs", "fn a() {}\n", "initial commit");
    commit(&repo, "src/a.rs", "fn a() { 1 }\n", "fix wrong return in a");
    commit(&repo, "docs/guide.md", "# guide\n", "fix docs typo");
    // 4000-line generated blob with a tempting message.
    commit(
        &repo,
        "src/gen.rs",
        &"x\n".repeat(4000),
        "fix generator output",
    );

    let config = NikiConfig::default();
    let outcome = mine_history(tmp.path(), &config, "niki-task-int");
    assert!(!outcome.unsupported);
    assert!(!outcome.invalidated);
    assert_eq!(outcome.learned, 1, "only the real Rust fix learns");
    assert_eq!(outcome.skipped_large, 1);

    let derived = history_learnings(tmp.path());
    assert!(derived.contains("fix wrong return in a"));
    assert!(!derived.contains("docs typo"), "non-code commits are noise");

    // Cache-as-truth: deleting the derived file and re-mining rebuilds it
    // identically without re-analyzing anything.
    std::fs::remove_file(tmp.path().join(".niki/history/learnings.jsonl")).unwrap();
    let rerun = mine_history(tmp.path(), &config, "niki-task-int");
    assert_eq!(rerun.analyzed, 0);
    let rebuilt = history_learnings(tmp.path());
    assert_eq!(rebuilt.lines().count(), 1);
}

#[test]
fn force_push_equivalent_invalidates_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(tmp.path()).unwrap();
    commit(&repo, "src/a.rs", "fn a() {}\n", "initial commit");
    commit(&repo, "src/a.rs", "fn a() { 1 }\n", "fix return value");
    let config = NikiConfig::default();
    mine_history(tmp.path(), &config, "niki-task-int");
    assert!(history_learnings(tmp.path()).contains("fix return value"));

    // Force-push simulation: hard-reset HEAD back, orphaning the mined head.
    let parent = repo
        .revparse_single("HEAD~1")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    repo.reset(parent.as_object(), git2::ResetType::Hard, None)
        .unwrap();

    let outcome = mine_history(tmp.path(), &config, "niki-task-int2");
    assert!(outcome.invalidated);
    assert!(
        !history_learnings(tmp.path()).contains("fix return value"),
        "orphaned entries are wiped"
    );
    // The invalidation itself is recorded as a project learning.
    let learnings = std::fs::read_to_string(tmp.path().join(".niki/learnings.jsonl")).unwrap();
    assert!(learnings.contains("History rewritten"));
}

#[test]
fn unsupported_repo_marks_state() {
    let tmp = tempfile::tempdir().unwrap();
    let outcome = mine_history(tmp.path(), &NikiConfig::default(), "niki-task-int");
    assert!(outcome.unsupported);
    let state = std::fs::read_to_string(tmp.path().join(".niki/history/state.json")).unwrap();
    assert!(state.contains("unsupported"));
}
