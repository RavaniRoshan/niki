use niki::config::NikiConfig;
use niki::orchestrator::provenance::{capture, read_manifest, record_completion, write_manifest};
use uuid::Uuid;

fn commit_fixture(repo_path: &std::path::Path) {
    let repo = git2::Repository::init(repo_path).unwrap();
    std::fs::write(repo_path.join("a.txt"), "hello\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = git2::Signature::now("niki-test", "niki@test").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();
}

#[test]
fn manifest_lifecycle_on_real_git_repo() {
    let tmp = tempfile::tempdir().unwrap();
    commit_fixture(tmp.path());
    // A local niki.toml is fingerprinted.
    std::fs::write(tmp.path().join("niki.toml"), "[general]\n").unwrap();

    let task_dir = tmp.path().join(".niki").join("tasks").join("task-1");
    let id = Uuid::new_v4();

    // Capture + write (what execute_pipeline does before the Planner).
    let manifest = capture(&NikiConfig::default(), tmp.path(), &id, &[]);
    assert_eq!(manifest.run_id, id);
    assert!(manifest.repo_identity.commit_sha.is_some());
    assert!(manifest.config_fingerprint.content_hash.is_some());
    write_manifest(&task_dir, &manifest).unwrap();

    // Dry-run style: no branch, empty artifacts — manifest still round-trips.
    let mut dry = manifest.clone();
    dry.dry_run = true;
    write_manifest(&task_dir, &dry).unwrap();
    assert!(read_manifest(&task_dir).unwrap().dry_run);

    // Completion style: result branch stamped with its commit.
    let repo = git2::Repository::open(tmp.path()).unwrap();
    let head = repo
        .revparse_single("HEAD")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    repo.branch("niki/abcd1234", &head, false).unwrap();
    record_completion(&task_dir, tmp.path(), Some("niki/abcd1234"), &[], 0.5);
    let done = read_manifest(&task_dir).unwrap();
    assert_eq!(done.branch.as_deref(), Some("niki/abcd1234"));
    assert_eq!(done.commit_sha, manifest.repo_identity.commit_sha);
    assert_eq!(done.total_cost_usd, 0.5);
    assert!(done.dry_run, "record_completion preserves the dry-run flag");
}

#[test]
fn manifest_fallback_outside_git() {
    let tmp = tempfile::tempdir().unwrap();
    let task_dir = tmp.path().join("task");
    let manifest = capture(&NikiConfig::default(), tmp.path(), &Uuid::new_v4(), &[]);
    assert!(manifest.repo_identity.commit_sha.is_none());
    assert!(manifest.repo_identity.workdir_fingerprint.is_some());
    assert_eq!(manifest.active_snapshot.kind, "nongit");
    write_manifest(&task_dir, &manifest).unwrap();
    // Completion without a branch falls back to the captured snapshot commit
    // (None here) and never errors.
    record_completion(&task_dir, tmp.path(), None, &[], 0.0);
    let done = read_manifest(&task_dir).unwrap();
    assert!(done.branch.is_none());
    assert!(done.commit_sha.is_none());
}
