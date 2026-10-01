//! Which of a turn's tool calls may run at the same time.
//!
//! ## The lock makes it safe; this decides what is *worth* doing
//!
//! [`PathLocks`](super::path_lock::PathLocks) is what makes a concurrent call
//! correct. This module decides the order — and order is not cosmetic here.
//! **Tool results go back to the model in the order the model asked for them.**
//! A model that asked `read a`, `edit b`, `read c` and receives `read c`'s
//! answer first is reading a different conversation, and one where the
//! ordering of its own actions is not preserved is one it cannot reason
//! about. So this plans batches for concurrency and hands results back in
//! call order regardless of which finished first.
//!
//! ## The rule
//!
//! Two calls may share a batch unless they conflict:
//!
//! * two **writes** to the same path — a lost update waiting to happen, and
//!   the path lock would serialise them anyway, so batching them buys nothing;
//! * a **write and a read** of the same path — §8's point: the read must not
//!   observe a half-applied write. It would be *safe* (the lock blocks it) but
//!   pointless, because the lock is what does the waiting;
//! * **anything at all** against a call that touches no known path. `bash` has
//!   no file to lock, so it can conflict with anything and is therefore run
//!   alone.
//!
//! Two **reads** of the same path are fine together: nothing is being
//! written, so there is no half-applied state to observe.

use std::path::PathBuf;

/// What a tool call does to the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Read,
    Write,
}

/// A tool call reduced to what scheduling needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCall {
    pub index: usize,
    pub kind: Kind,
    /// The file it touches, when it touches exactly one. `None` means "cannot
    /// be reasoned about" — which means exclusive, not parallel.
    pub path: Option<PathBuf>,
    pub name: String,
}

/// Classify a call by name and arguments.
///
/// **A best effort, and it is allowed to be wrong in one direction only.** An
/// unrecognised tool is `Write` with no path — the conservative answer, so it
/// runs alone. Getting a read wrong and calling it a write costs a little
/// parallelism; getting a write wrong and calling it a read costs correctness.
/// The asymmetry is the point.
pub fn classify(index: usize, name: &str, args: &serde_json::Value) -> PlannedCall {
    let path = args.get("path").and_then(|p| p.as_str()).map(PathBuf::from);
    let kind = match name {
        "read" | "glob" | "grep" | "webfetch" | "inspect" | "list" => Kind::Read,
        "write" | "edit" | "file_edit" | "str_replace" | "patch" | "create" | "delete" => {
            Kind::Write
        }
        // `bash` and anything unknown: touches paths we cannot see.
        _ => Kind::Write,
    };
    PlannedCall {
        index,
        kind,
        path,
        name: name.to_string(),
    }
}

/// Whether two calls may run in the same batch.
pub fn compatible(a: &PlannedCall, b: &PlannedCall) -> bool {
    // No known path on either side: unknowable, so exclusive.
    let (Some(pa), Some(pb)) = (&a.path, &b.path) else {
        return false;
    };
    if pa != pb {
        // Different files. Even a write and a write are fine together.
        return true;
    }
    // Same file. Two reads observe nothing mutable, so they may share.
    matches!((a.kind, b.kind), (Kind::Read, Kind::Read))
}

/// Group calls into batches, preserving order.
///
/// Greedy: each call joins the earliest batch it fits, so the plan is a
/// function of the call list alone — the same turn always produces the same
/// schedule, which is what makes it testable and what keeps a run
/// reproducible.
pub fn plan(calls: &[PlannedCall]) -> Vec<Vec<usize>> {
    let mut batches: Vec<Vec<PlannedCall>> = Vec::new();
    for call in calls {
        match batches
            .iter_mut()
            .find(|batch| batch.iter().all(|other| compatible(other, call)))
        {
            Some(batch) => batch.push(call.clone()),
            None => batches.push(vec![call.clone()]),
        }
    }
    batches
        .into_iter()
        .map(|b| b.into_iter().map(|c| c.index).collect())
        .collect()
}

/// The order results must be delivered in, whatever order they finish.
///
/// The model asked for call 0, then 1, then 2. If 2's result arrives first
/// the model is reading a conversation where it did something it had not yet
/// asked for. This is the function that puts it right, and it is why the
/// executor is allowed to be concurrent at all.
pub fn ordered<T>(results: Vec<(usize, T)>) -> Vec<T> {
    let mut by_index = results;
    by_index.sort_by_key(|(i, _)| *i);
    by_index.into_iter().map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(i: usize, name: &str, path: Option<&str>) -> PlannedCall {
        classify(i, name, &json!({ "path": path }))
    }

    /// Two writes to one file are a lost update waiting to happen. The lock
    /// would serialise them, so batching them buys nothing.
    #[test]
    fn two_writes_to_one_file_are_never_batched() {
        let calls = vec![
            call(0, "edit", Some("a.rs")),
            call(1, "write", Some("a.rs")),
        ];
        assert!(!compatible(&calls[0], &calls[1]));
        assert_eq!(plan(&calls), vec![vec![0], vec![1]]);
    }

    /// §8's case: a read must not be batched with a write of the same file.
    /// It would be *safe* — the lock blocks it — but the lock is what does
    /// the waiting, so batching them is a queue behind a lock.
    #[test]
    fn a_read_is_not_batched_with_a_write_of_the_same_file() {
        let calls = vec![call(0, "edit", Some("a.rs")), call(1, "read", Some("a.rs"))];
        assert!(!compatible(&calls[0], &calls[1]));
        assert_eq!(plan(&calls), vec![vec![0], vec![1]]);
    }

    /// But two reads of one file observe nothing mutable, so they may share.
    #[test]
    fn two_reads_of_one_file_are_batched() {
        let calls = vec![call(0, "read", Some("a.rs")), call(1, "grep", Some("a.rs"))];
        assert!(compatible(&calls[0], &calls[1]));
        assert_eq!(plan(&calls), vec![vec![0, 1]]);
    }

    /// Different files never conflict, writes included.
    #[test]
    fn different_files_always_batch() {
        let calls = vec![
            call(0, "edit", Some("a.rs")),
            call(1, "write", Some("b.rs")),
        ];
        assert!(compatible(&calls[0], &calls[1]));
        assert_eq!(plan(&calls), vec![vec![0, 1]]);
    }

    /// `bash` touches paths nothing can see, so it runs alone. A model that
    /// ran `bash` beside a `write` would be editing a file while a script
    /// rewrote the same one, and no per-path lock could see it.
    #[test]
    fn a_call_with_no_known_path_runs_alone() {
        let calls = vec![
            call(0, "bash", None),
            call(1, "edit", Some("a.rs")),
            call(2, "read", Some("b.rs")),
        ];
        let plan = plan(&calls);
        assert_eq!(plan[0], vec![0], "bash must be first and alone: {plan:?}");
        assert_eq!(
            plan[1],
            vec![1, 2],
            "and the two file calls may then share: {plan:?}"
        );
    }

    /// An unrecognised tool is treated as a write, not a read.
    ///
    /// The asymmetry is deliberate: calling a read a write costs a little
    /// parallelism, and calling a write a read costs correctness.
    #[test]
    fn an_unrecognised_tool_is_treated_as_a_write() {
        let c = classify(0, "some_new_tool", &json!({ "path": "a.rs" }));
        assert_eq!(c.kind, Kind::Write);
    }

    /// **The order property.** A model that asked for 0, 1, 2 must receive
    /// 0, 1, 2 — whatever order they finished in. Results out of order are a
    /// different conversation.
    #[test]
    fn results_are_delivered_in_the_order_the_model_asked() {
        let finished_out_of_order = vec![(2, "third"), (0, "first"), (1, "second")];
        assert_eq!(
            ordered(finished_out_of_order),
            vec!["first", "second", "third"],
            "a model reading reordered results is reading a conversation where \
             it did something it had not yet asked for"
        );
    }

    /// And a real plan: interleaved reads and writes, with the batches
    /// asserted rather than described.
    #[test]
    fn a_mixed_turn_is_planned_into_sensible_batches() {
        let calls = vec![
            call(0, "read", Some("a.rs")),
            call(1, "edit", Some("b.rs")),
            call(2, "grep", Some("c.rs")),
            call(3, "edit", Some("a.rs")),
        ];
        assert_eq!(
            plan(&calls),
            vec![vec![0, 1, 2], vec![3]],
            "0 reads a.rs, 1 writes b.rs and 2 greps c.rs — all different \\
             files, so all three share; 3 writes a.rs, which 0 was reading, so \\
             it waits for the batch"
        );
    }
}
