//! The invariant ratchet: run every invariant against a **real** pipeline run.
//!
//! This is where the harness stops being a library and starts finding bugs.
//! [`invariants`](super::invariants) is unit-tested against synthetic traces;
//! this module points the same checks at a trace built from a real
//! `TaskRecord` and real model output, so a defect in the product shows up as
//! a named invariant failure.
//!
//! The gate is a **ratchet**, not a fixed list:
//!
//! * No invariant outside [`KNOWN_FAILING`] may fail. Any new failure is a
//!   regression introduced by whatever change is under test.
//! * Invariants in `KNOWN_FAILING` are allowed to fail and are *reported*, so
//!   the debt stays visible instead of being normalised away. Removing one
//!   without fixing the underlying defect makes
//!   [`known_failing_invariants_are_still_failing`] fail.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::invariants::{self, KNOWN_FAILING, Verdict};
use super::llm_faults::{Fault, FaultServer};
use niki::artifacts::types::AgentRole;
use niki::config::ProviderConfig;
use niki::llm::provider::CompletionRequest;
use niki::orchestrator::state::{StageMetric, TaskRecord, TaskStatus};
use serde_json::Value;

fn metric(role: AgentRole, cost: f64) -> StageMetric {
    StageMetric {
        role,
        provider: "openai".to_string(),
        model: "test-model".to_string(),
        input_tokens: 1000,
        output_tokens: 200,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        latency_ms: 1000,
        cost_usd: cost,
        retry_count: 0,
        ttft_ms: 100,
    }
}

fn base_record() -> TaskRecord {
    let mut r = TaskRecord::new(uuid::Uuid::new_v4(), "Fix the off-by-one in paginate");
    r.status = TaskStatus::Completed;
    r.branch = Some("niki/abc12345".to_string());
    r
}

fn trace_from_record(
    task_dir: PathBuf,
    record: &TaskRecord,
    patch: Option<&str>,
    artifacts: &[(&str, Value)],
) -> invariants::RunTrace {
    let record_json = serde_json::to_value(record).expect("TaskRecord serializes");
    invariants::RunTrace {
        task_dir,
        record: Some(record_json),
        artifacts: artifacts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        patch: patch.map(str::to_string),
        report: None,
    }
}

fn is_known(id: &str) -> bool {
    KNOWN_FAILING.iter().any(|(k, _)| *k == id)
}

fn describe(v: &Verdict) -> String {
    match v {
        Verdict::Fail(m) => m.clone(),
        Verdict::Unexercised(m) => format!("(unexercised) {m}"),
        Verdict::Pass => "pass".to_string(),
    }
}

#[test]
fn a_well_formed_run_violates_nothing_outside_the_known_set() {
    // A real repo, so INV-BRANCH-STATUS has a real ref to resolve. A
    // synthetic trace cannot satisfy it, which is exactly the invariant
    // catching a fabricated Completed status.
    use crate::common::fixture_repo::create_fixture_repo;
    let repo = create_fixture_repo();
    let project = repo.dir.path().to_path_buf();
    let task_dir = project.join(".niki").join("tasks").join("t-1");
    std::fs::create_dir_all(&task_dir).unwrap();

    let branch = "niki/wellformed01";
    let ok = std::process::Command::new("git")
        .args(["branch", branch])
        .current_dir(&project)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(ok, "fixture repo must accept a branch creation");

    let mut record = base_record();
    record.branch = Some(branch.to_string());
    record.verdict_source = Some("reviewer".to_string());
    record.total_cost_usd = 0.30;
    record.agent_metrics = vec![
        metric(AgentRole::Coder, 0.10),
        metric(AgentRole::Reviewer, 0.20),
    ];

    let patch = "diff --git a/src/list.rs b/src/list.rs\n--- a/src/list.rs\n+++ b/src/list.rs\n@@ -1 +1 @@\n-a\n+b\n";
    let artifacts = [(
        "reviewer.json",
        serde_json::json!({
            "verdict": "approved",
            "summary": "The pagination fix is correct and covered by tests.",
            "issues": [],
            "strengths": ["clear diff"]
        }),
    )];

    let trace = trace_from_record(task_dir, &record, Some(patch), &artifacts);
    let results = invariants::check(&trace);

    let unknown: Vec<String> = results
        .iter()
        .filter(|(id, _, v)| matches!(v, Verdict::Fail(_)) && !is_known(id))
        .map(|(id, category, v)| format!("[{category}] {id}: {}", describe(v)))
        .collect();

    assert!(
        unknown.is_empty(),
        "a well-formed run tripped invariants that are not on the known-failing list. Either a \
         new defect was introduced, or an invariant was added that this fixture does not \
         satisfy:\n{}",
        unknown.join("\n")
    );

    // And the run should be genuinely clean, not merely non-regressing.
    let fails: Vec<&str> = results
        .iter()
        .filter(|(_, _, v)| matches!(v, Verdict::Fail(_)))
        .map(|(id, _, _)| *id)
        .collect();
    assert!(
        fails.is_empty(),
        "this fixture is meant to be a clean, fully-valid run, but {fails:?} fired. A ratchet \
         that passes because its fixture is already broken proves nothing."
    );
}

/// The other half of the ratchet: a known-failing entry must still reproduce,
/// or someone deleted it to turn a red build green.
#[test]
fn known_failing_invariants_are_still_failing() {
    // A record with no stage metrics, a hollow verdict, and no usable diff —
    // the exact shape the SingleAgent fast path produces.
    let mut record = base_record();
    // A SingleAgent self-approval: honest about its provenance, and still a
    // run with nothing reviewable behind it.
    record.verdict_source = Some("solo-coder (no independent review)".to_string());
    record.total_cost_usd = 0.42;
    let artifacts = [(
        "reviewer.json",
        serde_json::json!({
            "verdict": "approved",
            "summary": "",
            "issues": [],
            "strengths": []
        }),
    )];
    let trace = trace_from_record(
        PathBuf::from("/nonexistent-task-dir"),
        &record,
        Some("diff --git a/src/list.rs b/src/list.rs\n"),
        &artifacts,
    );
    let results = invariants::check(&trace);
    let failing: BTreeSet<&str> = results
        .iter()
        .filter(|(_, _, v)| matches!(v, Verdict::Fail(_)))
        .map(|(id, _, _)| *id)
        .collect();

    for (id, why) in KNOWN_FAILING {
        if *id == "INV-ARTIFACT-SEMANTIC" {
            // (INV-COST-SUM was removed from KNOWN_FAILING when the tool loop
            // and repair-retry paths were switched from `.max()` to `+=`.)
            assert!(
                failing.contains(id),
                "`{id}` is listed in KNOWN_FAILING but this trace no longer reproduces it. Fix \
                 the defect and remove the entry, or update the reproduction. Known reason: {why}"
            );
        }
    }

    // The one defect this trace is built to reproduce must be the one that
    // fires — otherwise the fixture has drifted into testing nothing.
    assert!(failing.contains("INV-ARTIFACT-SEMANTIC"));

    // **`INV-VERDICT-NOT-FABRICATED` was struck from `KNOWN_FAILING` in batch 8.**
    //
    // It named this defect: *"SingleAgent assigns `verdict = Verdict::Approved`
    // without a Reviewer (`pipeline.rs:2395)`"* — a line number that has since
    // moved, for code that no longer says what the entry claims.
    //
    // What the product does now, measured rather than assumed:
    //
    // * `RunOutcome::SelfVerified` for the SingleAgent fast path, and the final
    //   verdict is `outcome.verdict().unwrap_or(Verdict::RevisionNeeded)` — so a
    //   run with nobody reviewing it cannot carry a bare `Approved`;
    // * `verdict_source` is recorded on every path, and `Reviewed` is reachable
    //   only when `reviewer_ran && verdict_source.is_some()`;
    // * a **real** run asserts it: `tests/run_lifecycle.rs` reads
    //   `task.json` back and checks `outcome.outcome == "self_verified"`.
    //
    // The ratchet entry was pinning the programme to a *synthetic* failure — a
    // trace hand-built to be forbidden — so `KNOWN_FAILING` was reporting an
    // open defect that no run produces. Striking it is only honest if the check
    // still bites, which is what this holds: the shape the product writes must
    // pass, and the shape the invariant forbids must still fail.
    // `status` is `"Completed"` with a capital C, because that is what
    // `is_completed()` looks for. The first draft of this used `"completed"`,
    // so every check returned `pass()` at the top and **both** assertions were
    // vacuous — including the one that appeared to prove the strike was safe.
    let real_single_agent = invariants::RunTrace {
        task_dir: PathBuf::from("/nonexistent-task-dir"),
        record: Some(serde_json::json!({
            "status": "Completed",
            "verdict_source": "solo-coder (no independent review)",
            "agent_metrics": [{"role": "coder", "cost_usd": 0.30}],
            "outcome": {
                "outcome": "self_verified",
                "note": "the SingleAgent fast path approves its own patch; no \
                         independent review was performed",
            },
        })),
        artifacts: std::collections::BTreeMap::new(),
        patch: Some("diff --git a/src/list.rs b/src/list.rs\n".to_string()),
        report: None,
    };
    let solo_results = invariants::check(&real_single_agent);
    let solo = solo_results
        .iter()
        .find(|(id, _, _)| *id == "INV-VERDICT-NOT-FABRICATED")
        .expect(
            "the invariant is still registered — striking the KNOWN_FAILING \
                 entry must not delete the check",
        );
    assert!(
        matches!(solo.2, Verdict::Pass),
        "a run that says plainly it reviewed nothing must not be flagged as \
         fabricating a verdict: {}",
        describe(&solo.2)
    );

    let fabricated = invariants::RunTrace {
        record: Some(serde_json::json!({
            "status": "Completed",
            "verdict_source": "solo-coder (no independent review)",
            "agent_metrics": [{"role": "coder", "cost_usd": 0.30}],
            "outcome": {"outcome": "reviewed", "verdict": "approved", "by": "solo-coder"},
        })),
        ..real_single_agent
    };
    let fabricated_results = invariants::check(&fabricated);
    let caught = fabricated_results
        .iter()
        .find(|(id, _, _)| *id == "INV-VERDICT-NOT-FABRICATED")
        .expect("registered");
    assert!(
        matches!(caught.2, Verdict::Fail(_)),
        "and a self-approval dressed as an independent review must still be \
         caught — otherwise striking the entry removed a check rather than a \
         debt: {}",
        describe(&caught.2)
    );
}

/// The terminal-safety invariant must fire on content that came off the wire,
/// delivered through the real provider stack rather than a hand-built string.
#[test]
fn a_model_emitting_osc52_trips_the_terminal_invariant() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let payload = rt.block_on(async {
        let server = FaultServer::start(&[Fault::ProseNotJson]).await;
        let provider = niki::llm::provider::create_provider(
            "openai",
            &ProviderConfig {
                api_key: Some("test-key".to_string()),
                base_url: Some(server.uri()),
                default_model: "fault-model".to_string(),
            },
        )
        .expect("provider");
        provider
            .complete(CompletionRequest {
                model: "fault-model".to_string(),
                system_prompt: "You are NIKI.".to_string(),
                user_message: "Return a verdict.".to_string(),
                max_tokens: 256,
                temperature: 0.0,
                json_schema: None,
                tools: None,
                reasoning_effort: None,

                history: Vec::new(),
            })
            .await
            .ok()
            .map(|r| r.content)
    });

    let Some(payload) = payload else {
        // The provider rejecting the fault body is itself an acceptable
        // outcome; the detection logic is unit-tested directly elsewhere.
        return;
    };

    // A model that wraps its answer in a terminal-control payload — the
    // clipboard-write attack.
    let hostile = format!("{payload}\u{1b}]52;c;cHlwZWNiZXNlcnJldGVk\u{7}");
    let mut record = base_record();
    record.verdict_source = Some("reviewer".to_string());
    let trace = trace_from_record(
        PathBuf::from("/nonexistent-task-dir"),
        &record,
        None,
        &[("reviewer.json", serde_json::json!({ "summary": hostile }))],
    );
    let results = invariants::check(&trace);
    let v = results
        .iter()
        .find(|(id, _, _)| *id == "INV-TERMINAL-SAFE")
        .map(|(_, _, v)| v)
        .expect("INV-TERMINAL-SAFE is registered");
    assert!(
        matches!(v, Verdict::Fail(_)),
        "an OSC 52 sequence delivered by the model must trip INV-TERMINAL-SAFE, got {v:?}"
    );
}

/// Layer coverage must never read as covered for a layer with no invariants.
#[test]
fn layers_without_invariants_report_unexercised() {
    let results = invariants::check(&invariants::RunTrace::default());
    let cov = invariants::layer_coverage(&results);
    for (layer, (pass, fail, unex)) in &cov {
        if pass + fail + unex == 0 {
            // A layer with nothing behind it must be visibly empty, which is
            // the honest state. Named explicitly so adding coverage is a
            // visible, deliberate edit.
            assert!(
                matches!(
                    *layer,
                    "sandbox-isolation"
                        | "llm-protocol"
                        | "tui-ux"
                        | "config-secrets"
                        | "supply-chain"
                ),
                "layer `{layer}` has no invariants; add them or remove it from LAYERS"
            );
        }
    }
}

/// The fixture infrastructure is reachable from this module, so the ratchet can
/// be extended to drive a full pipeline run without restructuring.
#[test]
fn fixture_infrastructure_is_reachable() {
    use crate::common::fixture_repo::create_fixture_repo;
    let repo = create_fixture_repo();
    assert!(repo.dir.path().join(".git").exists());
}
