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

// ── an edit must not be able to match text it just wrote ───────────────────
//
// A live revision loop produced this after three rounds:
//
//     numbers.iter().sum()    numbers.iter().sum()    numbers.iter().sum()
//
// The per-round artifacts show round 1 emitting a `replace` that began with the
// very `search` it matched, so applying it left that text in place and round 2
// matched again. The corruption and the legitimate shape — "emit the signature,
// then fill it in" — are the same edit to a string-comparing validator.
//
// Rejecting the shape was tried and measured to be worse: it failed the
// overwhelmingly common case. So the result is made *stable* instead. An edit
// whose output is already present, and whose anchor is gone, has been applied.

#[test]
fn re_applying_the_same_extension_edit_is_a_no_op_rather_than_a_second_copy() {
    use niki::sandbox::edit_format::apply_single_edit_block;

    let file = "pub fn tally(numbers: &[i32]) -> i32 {\n    numbers.iter().sum()\n}\n";
    // The common model shape: the anchor is the signature line, the replacement
    // is that line plus the body.
    let search = "pub fn tally(numbers: &[i32]) -> i32 {";
    let replace = "pub fn tally(numbers: &[i32]) -> i32 {\n    numbers.iter().sum()";

    // Round 0: applies.
    let after_first = apply_single_edit_block(file, search, replace)
        .expect("applies")
        .expect("matched");
    assert!(after_first.contains("numbers.iter().sum()"));
    assert_eq!(
        after_first.matches("numbers.iter().sum()").count(),
        1,
        "the first application is a normal edit"
    );

    // Round 1: the same edit re-issued against the new file must not duplicate.
    let after_second = apply_single_edit_block(&after_first, search, replace)
        .expect("applies")
        .expect("matched");
    assert_eq!(
        after_second.matches("numbers.iter().sum()").count(),
        1,
        "re-issuing the same extension must converge, not duplicate. Got:\n{after_second}"
    );
    assert_eq!(after_second, after_first, "and it must be a true no-op");
}

#[test]
fn an_ordinary_replacement_is_still_applied_every_time_it_differs() {
    use niki::sandbox::edit_format::apply_single_edit_block;

    let file = "let x = 1;\n";
    let out = apply_single_edit_block(file, "let x = 1;", "let x = 2;")
        .expect("applies")
        .expect("matched");
    assert_eq!(out, "let x = 2;\n");

    // The idempotence guard must not swallow a second, different edit that
    // happens to be issued against text the first one produced.
    let again = apply_single_edit_block(&out, "let x = 2;", "let x = 3;")
        .expect("applies")
        .expect("matched");
    assert_eq!(again, "let x = 3;\n");
}

// ── a stream that dies mid-response ────────────────────────────────────────
//
// Establishing the connection retries three times. A connection that drops
// *partway through* a long response did not: it returned immediately, and it is
// the more common of the two against a local model.
//
// Measured: three live runs against qwen2.5-coder:3b. The Coder succeeded in
// all three, and two of them then died at the Tester with exactly this error.
// The work was finished and thrown away by a socket.

use niki::agents::is_mid_stream_retryable;

#[test]
fn a_dropped_connection_is_worth_re_asking_for() {
    for msg in [
        "Stream error: error decoding response body",
        "connection reset by peer",
        "connection closed before message completed",
        "incomplete message",
    ] {
        assert!(
            is_mid_stream_retryable(&anyhow::anyhow!("{msg}")),
            "{msg:?} is a transport failure and should be retried"
        );
    }
}

#[test]
fn a_model_that_refused_is_not_asked_again() {
    // The cost of getting this wrong is the user's tokens. A second identical
    // request to a model that refused, or that rejected the request, fails the
    // same way — and the user pays for the demonstration.
    for msg in [
        "content filter triggered",
        "invalid request: messages must be non-empty",
        "this request exceeds the context length",
        "prompt is too long",
        "the model refused to generate output",
    ] {
        assert!(
            !is_mid_stream_retryable(&anyhow::anyhow!("{msg}")),
            "{msg:?} is a permanent failure; retrying burns tokens to learn nothing"
        );
    }
}

#[test]
fn the_retry_is_bounded_and_covers_the_transport_classes() {
    // Source-level, because the bound itself is the property: an unbounded retry
    // against a local model that is reliably dropping connections is a hang.
    let src = include_str!("../src/agents/mod.rs");
    assert!(
        src.contains("const MAX_MID_STREAM_RETRIES: u32 = 2;"),
        "the mid-stream retry must be bounded"
    );
    assert!(
        src.contains("continue 'attempt;"),
        "a mid-stream failure must re-establish the request, not just be noted"
    );
    // A repeat of the SAME error is the transport being down, not one unlucky
    // read. Asserted on the *decision*, not on the source text: the previous
    // version grepped for the guard's condition, and every attempt to mutate
    // it away either failed to compile or still matched, so it pinned the
    // shape of the code rather than the behaviour.
    let dropped = || anyhow::anyhow!("Stream error: error decoding response body");
    let refused = || anyhow::anyhow!("refusal: the model declined");

    // First failure: a transport error, nothing seen before → retry.
    assert!(
        niki::agents::should_retry_mid_stream(&dropped(), 0, None),
        "a first dropped connection is worth re-establishing"
    );
    // Second identical failure → stop. This is the whole point of the guard:
    // a live run restarted a stage three times on the identical error and then
    // died, having spent three times the wall clock to learn nothing.
    assert!(
        !niki::agents::should_retry_mid_stream(&dropped(), 1, Some(&dropped().to_string())),
        "an unchanged repeat must not be retried"
    );
    // A *different* transport error is a different problem, and gets its retry.
    let reset = || anyhow::anyhow!("connection reset by peer");
    assert!(
        niki::agents::should_retry_mid_stream(&reset(), 1, Some(&dropped().to_string())),
        "a different error is a different problem and still gets its retry"
    );
    // The bound still holds, whatever the errors look like.
    assert!(
        !niki::agents::should_retry_mid_stream(
            &reset(),
            niki::agents::MAX_MID_STREAM_RETRIES,
            Some(&dropped().to_string())
        ),
        "the retry must be bounded; an unbounded retry against a local model that is \
         reliably dropping connections is a hang"
    );
    // A permanent failure is never retried, however many attempts remain.
    assert!(
        !niki::agents::should_retry_mid_stream(&refused(), 0, None),
        "a refusal is the model's own answer, not a transport problem"
    );

    assert!(
        src.contains("full_content.clear();"),
        "and must reset the partially-collected content, or the retry splices two \\
         responses together"
    );
}
