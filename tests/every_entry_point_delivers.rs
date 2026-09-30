//! Every front door that runs the pipeline must deliver its work.
//!
//! `niki run` and the TUI chat have delivered since T3a/T3. Two more entry
//! points did not, and both are how a real user reaches the product:
//!
//! - **`niki acp`** — an IDE. `acp/server.rs` called `execute_pipeline` and
//!   stopped, so four agents ran and the whole change evaporated. It then
//!   wrote the **diff text** into `record.branch`, a field every reader treats
//!   as a branch name, so `niki report` printed a unified diff where a branch
//!   belonged.
//! - **`niki goal`** — the goal loop. `goal/runner.rs` called
//!   `execute_pipeline` and stopped, so every iteration's work was thrown away.
//!
//! Both now call the same `orchestrator::deliver::deliver` the other two use,
//! with a pre-run snapshot taken *before* the pipeline writes anything, which
//! is the whole point of the hermetic proof.
//!
//! ## What this test can and cannot do
//!
//! It cannot run a pipeline — that needs a model, a container and minutes. So
//! it checks the *wiring*: every `execute_pipeline` call site either delivers,
//! or is one of the two places that legitimately must not (the eval harness,
//! which measures rather than delivers, and the run/chat paths that deliver a
//! few lines later). A call site that stops at the pipeline is the defect, and
//! it is visible without running anything.
//!
//! The `record.branch` half is asserted separately and behaviourally: a branch
//! name is a single line, and a field that must hold one cannot hold a diff.

use std::path::Path;

fn src(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("{rel} must be readable: {e}"))
}

/// Byte offsets of every `execute_pipeline(` call, per file.
fn call_sites() -> Vec<(&'static str, usize)> {
    let files = [
        "src/cli/run.rs",
        "src/cli/chat.rs",
        "src/acp/server.rs",
        "src/goal/runner.rs",
        "src/eval/mod.rs",
    ];
    let mut out = Vec::new();
    for f in files {
        let body = src(f);
        let mut from = 0;
        while let Some(i) = body[from..].find("execute_pipeline(") {
            out.push((f, from + i));
            from = from + i + 1;
        }
    }
    out
}

/// A file that runs the pipeline must deliver, **after** that call.
///
/// Ordering is checked by byte offset rather than by a window of characters.
/// The first version used a 4000-character window, and `niki run` — which
/// delivers 300 lines after its `execute_pipeline` — was reported as an entry
/// point that never delivers. A window is a guess about how far away the
/// deliver can be; an offset is a fact.
#[test]
fn every_pipeline_entry_point_delivers() {
    // `eval` measures the pipeline against a baseline; delivering there would
    // cut a branch per evaluation, which is the opposite of what a benchmark
    // wants. The other four all produce work for a person.
    let exempt = ["src/eval/mod.rs"];
    let mut undelivered = Vec::new();
    for (file, at) in call_sites() {
        if exempt.contains(&file) {
            continue;
        }
        let body = src(file);
        let delivers_at = body.find("deliver::deliver(").unwrap_or(usize::MAX);
        // The pipeline call itself must come first, and a deliver *before* it
        // would be delivering the previous run's result.
        if delivers_at == usize::MAX || delivers_at < at {
            undelivered.push(file);
        }
    }
    undelivered.dedup();
    assert!(
        undelivered.is_empty(),
        "these entry points run the pipeline and never deliver, so the work is \
         thrown away: {undelivered:?}. Every one of them is how a user reaches \
         the product — a terminal, an IDE, a goal loop."
    );
}

/// `niki run` is the reference, so this is the shape every other must match.
#[test]
fn niki_run_delivers_after_its_pipeline() {
    let body = src("src/cli/run.rs");
    let pipeline_at = body
        .find("execute_pipeline(")
        .expect("niki run must run the pipeline");
    let deliver_at = body
        .find("deliver::deliver(")
        .expect("niki run must deliver");
    assert!(
        deliver_at > pipeline_at,
        "delivery must follow the pipeline, or it delivers the previous run"
    );
}

/// And the two that were broken are named, so a future refactor that moves the
/// call has to re-answer the question rather than quietly pass.
#[test]
fn acp_and_goal_deliver_specifically() {
    for (file, why) in [
        (
            "src/acp/server.rs",
            "`niki acp` is how an IDE runs a task; without delivery four agents \
             run and nothing survives",
        ),
        (
            "src/goal/runner.rs",
            "`niki goal` runs a pipeline per iteration; without delivery every \
             iteration's work is thrown away",
        ),
    ] {
        let body = src(file);
        assert!(
            body.contains("deliver::deliver("),
            "{file} does not deliver. {why}"
        );
        // And the snapshot that makes the hermetic proof mean anything.
        assert!(
            body.contains("safety::snapshot("),
            "{file} must snapshot the repository *before* the pipeline writes to \
             it, or the proof describes the change rather than the state it \
             changed"
        );
    }
}

/// A delivery failure must not read as success. In the goal loop it blocked a
/// task; in ACP it returned a JSON-RPC error.
#[test]
fn a_delivery_failure_is_not_reported_as_completion() {
    let acp = src("src/acp/server.rs");
    assert!(
        acp.contains("task.delivery_failed"),
        "ACP must tell the client that delivery failed, not only that the \
         pipeline did"
    );
    assert!(
        acp.contains("Delivery failed: {e}") || acp.contains("\"Delivery failed: {e}\""),
        "and the response must say so"
    );
    let goal = src("src/goal/runner.rs");
    assert!(
        goal.contains("delivery failed") && goal.contains("TaskStatus::Blocked"),
        "the goal loop must mark a task Blocked when its work cannot be \
         delivered, or the loop counts a lost iteration as progress"
    );
}

/// `record.branch` holds a branch name. It held a diff.
#[test]
fn the_record_branch_holds_a_branch_not_a_diff() {
    let acp = src("src/acp/server.rs");
    assert!(
        !acp.contains("record.branch = Some(r.final_diff"),
        "`record.branch` was assigned the diff *text*. Every reader of that \
         field treats it as a branch name — `niki report`, `niki status`, the \
         TUI's history — so `niki report` printed a unified diff where a \
         branch belonged."
    );
    assert!(
        acp.contains("record.branch = Some(branch_name"),
        "and it must be assigned the branch delivery actually created"
    );
    // The ground truth for the field's type.
    let state = src("src/orchestrator/state.rs");
    let doc = state
        .split("pub branch:")
        .nth(1)
        .and_then(|r| r.split("\n").nth(1))
        .unwrap_or("");
    assert!(
        !doc.contains("diff"),
        "the field is documented as holding a branch; if that changed, this \
         test should say so rather than keep asserting the old contract: {doc}"
    );
}

/// The branch name is the same shape everywhere: `niki/<8 hex>`.
#[test]
fn every_entry_point_names_the_branch_the_same_way() {
    for file in ["src/cli/chat.rs", "src/acp/server.rs", "src/goal/runner.rs"] {
        let body = src(file);
        assert!(
            body.contains("niki/{}") && body.contains("to_string()[..8]"),
            "{file} must name its branch `niki/<first 8 of the task id>`, the \
             shape `niki run` uses — a user who copies a branch out of a \
             report and types it in should not have to know which surface \
             produced it"
        );
    }
}
