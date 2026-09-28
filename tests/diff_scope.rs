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

/// The contract has to be able to say "this file is new".
///
/// `FileAction::Create` has always existed in the schema, and `apply_patch`
/// now honours it, but `check_semantics` rejected the only edit that can
/// express a creation — an empty `search`, since a file that does not exist has
/// no content to anchor to. So the `docs` task (write a README) was
/// unexpressible, and the model was left inventing an anchor for an empty file,
/// which is the `edits[0] has an empty search` failure four of five breadth
/// runs died on.
///
/// An empty `search` is allowed — it means create or append, not nothing.
///
/// It used to be refused outright, and refusing was actively harmful rather
/// than merely strict: `apply_single_edit` matches an empty string at offset 0,
/// so the edit would have been applied at the *top* of the file anyway. A model
/// writing "add a function to this file" has no natural anchor to quote and
/// puts the new code in `replace` — measured, and it was the single most common
/// way a breadth run failed.
///
/// What is still refused is an edit with nothing on *either* side, which
/// changes nothing at all.
#[test]
fn an_empty_search_is_allowed_because_it_means_create_or_append() {
    let create = serde_json::json!({
        "edits": [{ "search": "", "replace": "# Title\n\nBody." }],
        "files_changed": [
            { "path": "README.md", "action": "create", "language": null }
        ],
        "implementation_notes": "wrote the readme",
        "spec_adherence": "as asked",
        "uncertainties": null
    });
    niki::artifacts::validate::validate_artifact(
        &create.to_string(),
        "schemas/code_diff.schema.json",
    )
    .expect("a creation has no prior content to anchor to, and must be allowed");

    let append = serde_json::json!({
        "edits": [{ "search": "", "replace": "pub fn total() {}" }],
        "files_changed": [
            { "path": "src/lib.rs", "action": "modify", "language": "rust" }
        ],
        "implementation_notes": "appended",
        "spec_adherence": "as asked",
        "uncertainties": null
    });
    niki::artifacts::validate::validate_artifact(
        &append.to_string(),
        "schemas/code_diff.schema.json",
    )
    .expect("adding to a file has no anchor to quote, and must be allowed");
}

/// The patch text has to carry the file binding, or a creation has nowhere to
/// land.
#[test]
fn the_patch_text_binds_blocks_to_the_file_they_change() {
    use niki::artifacts::types::{ChangedFile, CodeDiff, EditBlock, FileAction};

    let diff = CodeDiff {
        edits: vec![EditBlock {
            search: "".into(),
            replace: "# Title\n".into(),
        }],
        files_changed: vec![ChangedFile {
            path: "README.md".into(),
            action: FileAction::Create,
            language: None,
        }],
        implementation_notes: String::new(),
        spec_adherence: String::new(),
        uncertainties: None,
    };
    let text = niki::orchestrator::pipeline::code_diff_to_edit_text(&diff);
    assert!(
        text.starts_with("FILE: README.md"),
        "a creation must be bound to its path, or the applier cannot know what to create: {text}"
    );

    // The single-file case binds modifications too, so a one-file task never
    // depends on a cross-file search finding the right target.
    let single = CodeDiff {
        edits: vec![EditBlock {
            search: "pub fn add".into(),
            replace: "pub fn total".into(),
        }],
        files_changed: vec![ChangedFile {
            path: "src/lib.rs".into(),
            action: FileAction::Modify,
            language: Some("rust".into()),
        }],
        implementation_notes: String::new(),
        spec_adherence: String::new(),
        uncertainties: None,
    };
    let text = niki::orchestrator::pipeline::code_diff_to_edit_text(&single);
    assert!(text.starts_with("FILE: src/lib.rs"), "{text}");
}

/// A `create` edit has to actually create the file, end to end.
///
/// The docs task — "write a README.md for this crate" — is a first-run-sized
/// request, and four of five breadth failures were `edits[0] has an empty
/// search`: the model was being asked for a search anchor in a file that had no
/// content to copy. The contract can now say `action: "create"`, and this
/// proves the applier honours it rather than rejecting the block as unmatched
/// or, worse, writing the body over an unrelated first file.
#[tokio::test(flavor = "multi_thread")]
async fn a_create_edit_writes_the_file_it_names() {
    let dir = fixture_repo();
    let repo = dir.path();
    let sb = worktree_sandbox(repo).await;

    let patch =
        "FILE: README.md\n<<<<<<< SEARCH\n\n=======\n# Crate\n\nDoes a thing.\n>>>>>>> REPLACE\n\n";
    sb.apply_patch(patch, repo)
        .await
        .expect("a create block must be applied, not rejected as unmatched");

    let created = sb.worktree_path.join("README.md");
    assert!(created.exists(), "the named file must exist after the edit");
    assert_eq!(
        std::fs::read_to_string(&created).unwrap(),
        "# Crate\n\nDoes a thing.",
        "the body must be the replacement, verbatim — including the absence of a trailing \
         newline, which is what the patch said. A whole-file write that quietly added one would \
         make every created file differ from what the model asked for."
    );

    // And it must not have been written over something else on the way.
    assert_eq!(
        std::fs::read_to_string(sb.worktree_path.join("tracked.rs")).unwrap(),
        std::fs::read_to_string(repo.join("tracked.rs")).unwrap(),
        "an unbound or mis-targeted block must not rewrite another file"
    );
    sb.destroy().await.unwrap();
}

/// An empty anchor against a file that already exists appends; it never
/// overwrites.
///
/// It used to be refused here, as a would-be creation against an existing
/// file. Refusing lost the common case — "add this to the file" — without
/// protecting anything, because what the model got instead was an empty anchor
/// matched at offset 0, putting the new code at the *top* of the file.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_anchor_against_an_existing_file_never_overwrites_it() {
    let dir = fixture_repo();
    let repo = dir.path();
    let sb = worktree_sandbox(repo).await;
    let before = std::fs::read_to_string(sb.worktree_path.join("tracked.rs")).unwrap();

    let patch = "FILE: tracked.rs\n<<<<<<< SEARCH\n\n=======\nfn a() { 999 }\n>>>>>>> REPLACE\n\n";
    sb.apply_patch(patch, repo)
        .await
        .expect("an append to an existing file is an append, not a would-be creation");

    let after = std::fs::read_to_string(sb.worktree_path.join("tracked.rs")).unwrap();
    assert!(
        after.starts_with(&before),
        "nothing may be lost: the original must still be there, byte for byte, at the front. \
         Got: {after:?}"
    );
    assert!(
        after.trim_end().ends_with("fn a() { 999 }"),
        "and the new code must be at the end: {after:?}"
    );
    sb.destroy().await.unwrap();
}

/// An empty `search` means append — and it used to mean "insert at the top".
///
/// `apply_single_edit` matches the empty string at offset 0, so an empty
/// anchor fell through to the exact-match strategy and the replacement landed
/// at the *top* of the file. A model writing "add a function to this file" has
/// no natural anchor to quote and puts the new code in `replace`; the result
/// was new code above the imports and above the item it was meant to sit
/// beside.
///
/// Appending is the only reading of an empty anchor that cannot scramble a
/// file, and it is what the model meant. `refactor` and `add-function` both
/// died on this, and it was the single most common way a breadth run failed.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_search_appends_instead_of_prepending() {
    let dir = fixture_repo();
    let repo = dir.path();
    let sb = worktree_sandbox(repo).await;
    let target = sb.worktree_path.join("tracked.rs");
    let before = std::fs::read_to_string(&target).unwrap();

    let appended = "fn added() {}\n";
    let patch =
        format!("FILE: tracked.rs\n<<<<<<< SEARCH\n\n=======\n{appended}>>>>>>> REPLACE\n\n");
    sb.apply_patch(&patch, repo)
        .await
        .expect("an empty anchor is an append, not a failure");

    let after = std::fs::read_to_string(&target).unwrap();
    assert!(
        after.starts_with(&before),
        "the existing content must stay first; appending put it at the top: {after:?}"
    );
    assert!(
        after.trim_end().ends_with("fn added() {}"),
        "and the replacement must be at the end: {after:?}"
    );
    sb.destroy().await.unwrap();
}

/// An append to a file with no trailing newline still ends up well-formed.
#[test]
fn an_append_to_a_file_without_a_trailing_newline_still_produces_one() {
    let out =
        niki::sandbox::edit_format::apply_single_edit_block("fn a() { 1 }", "", "fn b() { 2 }")
            .expect("append applies")
            .expect("append produced content");
    assert_eq!(out, "fn a() { 1 }\nfn b() { 2 }\n");
}

/// An empty anchor *and* an empty replacement is nothing, and is refused
/// rather than being read as an append of the empty string.
#[test]
fn an_edit_with_nothing_on_either_side_is_refused() {
    let err = niki::artifacts::validate::validate_artifact(
        &serde_json::json!({
            "edits": [{ "search": "", "replace": "" }],
            "files_changed": [
                { "path": "src/lib.rs", "action": "modify", "language": "rust" }
            ],
            "implementation_notes": "x",
            "spec_adherence": "y",
            "uncertainties": null
        })
        .to_string(),
        "schemas/code_diff.schema.json",
    )
    .expect_err("an edit that changes nothing is not an edit");
    assert!(
        err.to_string().contains("changes nothing"),
        "the error must say why: {err}"
    );
}

/// A whitespace-only anchor is an append too.
///
/// A model that cannot think of an anchor writes `"   "` as readily as `""`,
/// and the two mean the same thing. Only a *blank* anchor — nothing to search
/// for — is an append; an anchor with any content in it is a real search and
/// must match.
#[test]
fn a_whitespace_only_anchor_appends() {
    let out = niki::sandbox::edit_format::apply_single_edit_block(
        "fn a() { 1 }\n",
        "   \n",
        "fn b() { 2 }",
    )
    .expect("a blank anchor is an append")
    .expect("and produces content");
    assert!(
        out.starts_with("fn a() { 1 }"),
        "existing content first: {out:?}"
    );
    assert!(
        out.trim_end().ends_with("fn b() { 2 }"),
        "new content last: {out:?}"
    );

    // A real anchor still has to match: an anchor that is not in the file is
    // "no match", which is `Ok(None)` — the applier reports it as unmatched
    // rather than quietly appending.
    assert_eq!(
        niki::sandbox::edit_format::apply_single_edit_block(
            "fn a() { 1 }\n",
            "fn not_here() {}\n",
            "x",
        )
        .expect("a missing anchor is not an error"),
        None,
        "an anchor that is not in the file must not be treated as an append"
    );
}
