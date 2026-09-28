//! Phase 5.1: diffs are scoped to agent-produced changes.
//!
//! - Pre-existing dirty/untracked files stay out of `changes.patch` and the commit.
//! - Brand-new agent files appear (scoped intent-to-add, both backends).
//! - Edit-format application is all-or-nothing per stage.

use niki::artifacts::types::AgentRole;
use niki::config::{DockerConfig, SecurityPolicyConfig};
use niki::sandbox::Sandbox;
use std::sync::mpsc;
use tempfile::TempDir;
use uuid::Uuid;

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn fixture_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "t@t"]);
    git(repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("tracked.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.join("dirty.rs"), "fn dirty() {}\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", "init"]);
    // Pre-existing dirt: a modified tracked file + an untracked file the
    // agent never touches.
    std::fs::write(repo.join("dirty.rs"), "fn dirty() { 1 }\n").unwrap();
    std::fs::write(repo.join("user-scratch.txt"), "user notes\n").unwrap();
    dir
}

#[test]
fn scoped_diff_excludes_preexisting_dirt() {
    let dir = fixture_repo();
    let repo = dir.path();

    // Agent changes: modify a tracked file, create a brand-new file.
    std::fs::write(repo.join("tracked.rs"), "fn a() { 42 }\n").unwrap();
    std::fs::write(repo.join("agent-new.rs"), "fn fresh() {}\n").unwrap();

    let patch = niki::output::git::working_tree_diff_scoped(
        repo,
        &["tracked.rs".to_string(), "agent-new.rs".to_string()],
    );
    assert!(patch.contains("tracked.rs"), "agent edit must be present");
    assert!(patch.contains("agent-new.rs"), "new agent file must appear");
    assert!(
        !patch.contains("dirty.rs"),
        "pre-existing dirty file excluded"
    );
    assert!(
        !patch.contains("user-scratch"),
        "pre-existing untracked dirt excluded"
    );
}

#[test]
fn scoped_diff_leaves_host_index_untouched_for_unlisted_files() {
    let dir = fixture_repo();
    let repo = dir.path();
    std::fs::write(repo.join("agent-new.rs"), "fn fresh() {}\n").unwrap();

    let before = git(repo, &["status", "--porcelain"]);
    let _ = niki::output::git::working_tree_diff_scoped(repo, &["agent-new.rs".to_string()]);
    let after = git(repo, &["status", "--porcelain"]);
    // Only the agent file may gain an intent-to-add entry; user dirt lines
    // must be byte-identical before/after.
    for line in after.lines() {
        if line.contains("agent-new.rs") {
            continue;
        }
        assert!(
            before.lines().any(|b| b == line),
            "host index changed for unlisted file: {line}"
        );
    }
    assert!(before.contains("user-scratch.txt"));
    assert!(after.contains("user-scratch.txt"));
}

#[test]
fn empty_agent_files_yield_empty_diff_without_host_mutation() {
    let dir = fixture_repo();
    let repo = dir.path();
    let before = git(repo, &["status", "--porcelain"]);
    let patch = niki::output::git::working_tree_diff_scoped(repo, &[]);
    assert!(patch.is_empty());
    assert_eq!(git(repo, &["status", "--porcelain"]), before);
}

#[test]
fn agent_files_helper_parses_coder_json() {
    let json = serde_json::json!({
        "edits": [],
        "files_changed": [
            {"path": "src/a.rs", "action": "modify", "language": "rust"},
            {"path": "src/b.rs", "action": "create", "language": "rust"},
        ],
        "implementation_notes": "n",
        "spec_adherence": "s",
    })
    .to_string();
    let files = niki::output::git::agent_files_from_coder_json(Some(&json));
    assert_eq!(files, vec!["src/a.rs", "src/b.rs"]);
    assert!(niki::output::git::agent_files_from_coder_json(None).is_empty());
    assert!(niki::output::git::agent_files_from_coder_json(Some("not json")).is_empty());
}

async fn worktree_sandbox(repo: &std::path::Path) -> niki::sandbox::worktree::WorktreeSandbox {
    let (tx, _) = mpsc::channel();
    niki::sandbox::worktree::WorktreeSandbox::create(
        AgentRole::Coder,
        repo,
        &Uuid::new_v4(),
        &DockerConfig::default(),
        &niki::config::NikiConfig::default(),
        SecurityPolicyConfig::default(),
        tx,
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn worktree_diff_includes_new_agent_files() {
    // Phase 5.1: the worktree backend previously skipped `-N`, dropping new
    // files from `final_diff`. Scoped intent-to-add fixes it.
    let dir = fixture_repo();
    let repo = dir.path();
    let sb = worktree_sandbox(repo).await;

    std::fs::write(sb.worktree_path.join("brand-new.rs"), "fn brand_new() {}\n").unwrap();
    let diff = sb.get_diff(&["brand-new.rs".to_string()]).await.unwrap();
    assert!(
        diff.contains("brand-new.rs"),
        "new file must appear: {diff}"
    );

    // Unlisted content stays out.
    std::fs::write(sb.worktree_path.join("noise.log"), "noise\n").unwrap();
    let diff = sb.get_diff(&["brand-new.rs".to_string()]).await.unwrap();
    assert!(!diff.contains("noise.log"));
    sb.destroy().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn edit_apply_is_all_or_nothing() {
    // Phase 5.1: a stage with ANY unmatched block writes NOTHING.
    let dir = fixture_repo();
    let repo = dir.path();
    let sb = worktree_sandbox(repo).await;
    let target = sb.worktree_path.join("tracked.rs");
    let before = std::fs::read_to_string(&target).unwrap();

    let patch = "FILE: tracked.rs\n<<<<<<< SEARCH\nfn a() {}\n=======\nfn a() { 42 }\n>>>>>>> REPLACE\n\nFILE: ghost.rs\n<<<<<<< SEARCH\nnothing here\n=======\nsomething\n>>>>>>> REPLACE\n";
    let err = sb.apply_patch(patch, repo).await.unwrap_err();
    assert!(err.to_string().contains("unmatched"));
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        before,
        "matched file must be untouched when a sibling block fails"
    );
    sb.destroy().await.unwrap();
}

// ── an edit that cannot converge is not an edit ────────────────────────────
//
// A live revision loop produced this file after three rounds:
//
//     pub fn tally(numbers: &[i32]) -> i32 {
//         numbers.iter().sum()    numbers.iter().sum()    numbers.iter().sum()
//     }
//
// The per-round artifacts show why. Round 0 replaced the file with a clean
// version. Round 1 emitted a `replace` that was that same text with the tail
// chopped off — and which therefore *begins with the very `search` it matched*.
// Applying it leaves the search text in place, so round 2 matched again and
// inserted again. Nothing in the harness was re-applying anything; the model was
// handing back a non-converging edit and the harness believed it.
//
// The identical-search rule that already existed catches only the degenerate
// case. This is the general form, and it is what a truncated response looks like.

fn code_diff_json(edit: serde_json::Value) -> String {
    serde_json::json!({
        "edits": [edit],
        "files_changed": [{"path": "src/lib.rs", "action": "modify", "language": "rust"}],
        "implementation_notes": "n",
        "spec_adherence": "s"
    })
    .to_string()
}

#[test]
fn an_edit_whose_replacement_starts_with_its_search_is_rejected() {
    let diff = code_diff_json(serde_json::json!({
        "search": "pub fn tally(numbers: &[i32]) -> i32 {\n    numbers.iter().sum()",
        "replace": "pub fn tally(numbers: &[i32]) -> i32 {\n    numbers.iter().sum()\n}\n\n"
    }));
    let err = niki::artifacts::validate::validate_artifact(&diff, "schemas/code_diff.schema.json")
        .expect_err("a non-converging edit must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("cannot converge"),
        "the message must explain the problem: {msg}"
    );
    assert!(
        msg.contains("truncated"),
        "and name the likely cause, which is what tells a user what to do: {msg}"
    );
}

#[test]
fn an_ordinary_edit_still_passes() {
    // The other half, and the one that matters most: a rule that rejects real
    // work is worse than no rule. Every one of these is a legitimate edit.
    //
    // Note what is NOT here: `X -> X + "\n"`. That is a real thing a model
    // means to do, and this rule rejects it — correctly. Applying it leaves `X`
    // in place, so the next round matches again and appends another newline; it
    // never converges either. The model has to widen its `search` to include
    // the line it wants to replace rather than re-inserting the line it
    // matched. That cost is the point.
    for (search, replace) in [
        ("old line\n", "new line\n"),
        // A replacement that *contains* the search later on, but does not start
        // with it, is fine: the search text is removed.
        ("    return 0;\n", "    let total = 0;\n    return total;\n"),
        // Deleting outright.
        ("obsolete helper\n", ""),
    ] {
        let diff = code_diff_json(serde_json::json!({"search": search, "replace": replace}));
        assert!(
            niki::artifacts::validate::validate_artifact(&diff, "schemas/code_diff.schema.json")
                .is_ok(),
            "a legitimate edit was rejected: {search:?} -> {replace:?}"
        );
    }
}
