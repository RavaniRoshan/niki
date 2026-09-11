//! Post-run reflection: derives durable learnings from a finished pipeline
//! and appends them to `<output_dir>/learnings.jsonl`.
//!
//! Kinds (all advisory, all inferred):
//! - `verification_failure`: the executed suite failed, or a Reviewer
//!   rejected/asked revision on test grounds (`TestGap`).
//! - `review_correction`: the Critic rejected a verdict as ungrounded (it
//!   changed the run's trajectory exactly once).
//! - `security_fix`: the SecurityAuditor reported High/Critical findings.
//!
//! `cost_anomaly` is reserved for future spend analysis. Reflection is gated
//! on `[repo_intel] enabled` and never fails the run. Derived entries flow
//! back into the Planner via the KB build + context pack — never into the
//! Reviewer, to avoid untrusted bias in the gate itself.

use crate::artifacts::types::{
    AgentRole, CriticDisposition, IssueCategory, ReviewVerdict, SecuritySeverity, SecurityVerdict,
    Verdict,
};
use crate::config::NikiConfig;
use crate::knowledge::learnings::{LearningEntry, append_learning};
use crate::orchestrator::pipeline::PipelineResult;
use std::path::Path;

/// Derive learning entries from a finished run (pure; no I/O).
/// `snapshot_id` anchors the entries to the run's provenance.
pub fn derive_learnings(snapshot_id: &str, result: &PipelineResult) -> Vec<LearningEntry> {
    let mut out = Vec::new();
    let task_id = result.task_id.to_string();

    // 1. Verification failures: real suite failure, or reviewer pushback on
    // test grounds (any round — the artifact trail keeps every verdict).
    let mut verification_notes = Vec::new();
    if let Some(te) = &result.test_execution
        && !te.passed
    {
        verification_notes.push(format!(
            "executed suite `{}` failed (exit {})",
            te.command, te.exit_code
        ));
        if let Some(mutation) = te.mutation.as_ref()
            && !mutation.passed
        {
            verification_notes.push(format!(
                "mutation gate `{}` failed (exit {})",
                mutation.command, mutation.exit_code
            ));
        }
    }
    for (role, json) in &result.artifacts {
        if *role != AgentRole::Reviewer {
            continue;
        }
        if let Ok(verdict) = serde_json::from_str::<ReviewVerdict>(json)
            && !matches!(verdict.verdict, Verdict::Approved)
            && verdict
                .issues
                .iter()
                .any(|i| matches!(i.category, IssueCategory::TestGap))
        {
            verification_notes.push(format!(
                "reviewer {:?} on test grounds: {}",
                verdict.verdict,
                verdict
                    .issues
                    .iter()
                    .filter(|i| matches!(i.category, IssueCategory::TestGap))
                    .map(|i| i.description.chars().take(160).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    if !verification_notes.is_empty() {
        let mut entry = LearningEntry::new(
            "verification_failure",
            snapshot_id,
            "reflect",
            "inferred",
            format!(
                "task {task_id}: {} (final verdict: {:?})",
                verification_notes.join("; "),
                result.verdict
            ),
        );
        entry.task_id = Some(task_id.clone());
        out.push(entry);
    }

    // 2. Review corrections: every Critic Reject changed the trajectory once.
    for (role, json) in &result.artifacts {
        if *role != AgentRole::Critic {
            continue;
        }
        if let Ok(critique) = serde_json::from_str::<crate::artifacts::types::Critique>(json)
            && matches!(critique.disposition, CriticDisposition::Reject)
        {
            let mut entry = LearningEntry::new(
                "review_correction",
                snapshot_id,
                "reflect",
                "inferred",
                format!(
                    "task {task_id}: critic rejected a verdict as ungrounded ({} unsupported claim(s)): {}",
                    critique.unsupported_claims.len(),
                    critique
                        .unsupported_claims
                        .iter()
                        .map(|c| c.chars().take(160).collect::<String>())
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            );
            entry.task_id = Some(task_id.clone());
            out.push(entry);
        }
    }

    // 3. Security fixes: High/Critical auditor findings, whatever the verdict
    // (Approved-with-findings means they were patched in-loop).
    for (role, json) in &result.artifacts {
        if *role != AgentRole::SecurityAuditor {
            continue;
        }
        if let Ok(sv) = serde_json::from_str::<SecurityVerdict>(json) {
            let serious: Vec<String> = sv
                .findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.severity,
                        SecuritySeverity::Critical | SecuritySeverity::High
                    )
                })
                .map(|f| {
                    format!(
                        "{:?}/{:?}: {}",
                        f.severity,
                        f.category,
                        f.description.chars().take(160).collect::<String>()
                    )
                })
                .collect();
            if !serious.is_empty() {
                let mut entry = LearningEntry::new(
                    "security_fix",
                    snapshot_id,
                    "reflect",
                    "inferred",
                    format!(
                        "task {task_id}: auditor {:?} with {} high/critical finding(s): {}",
                        sv.verdict,
                        serious.len(),
                        serious.join("; ")
                    ),
                );
                entry.task_id = Some(task_id.clone());
                out.push(entry);
            }
        }
    }

    out
}

/// Snapshot anchor for reflections: the run's manifest when present,
// otherwise an explicit unknown marker (never fabricated).
fn snapshot_for(task_dir: &Path) -> String {
    crate::orchestrator::provenance::read_manifest(task_dir)
        .map(|m| m.active_snapshot.snapshot_id)
        .unwrap_or_else(|_| "niki-task-unknown".to_string())
}

/// Derive + append reflections for a finished run. Returns entries written.
/// Gated on `[repo_intel] enabled`; append failures warn and are skipped.
pub fn record_reflections(
    project_path: &Path,
    config: &NikiConfig,
    task_dir: &Path,
    result: &PipelineResult,
) -> usize {
    if !config.repo_intel.enabled {
        return 0;
    }
    let snapshot_id = snapshot_for(task_dir);
    let mut written = 0usize;
    for entry in derive_learnings(&snapshot_id, result) {
        if let Err(e) = append_learning(project_path, config, &entry) {
            eprintln!("Warning: could not record learning ({}): {e}", entry.kind);
            continue;
        }
        written += 1;
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::compression::ContextBudget;
    use crate::orchestrator::state::PipelineState;
    use serde_json::json;
    use uuid::Uuid;

    fn empty_result() -> PipelineResult {
        let id = Uuid::new_v4();
        PipelineResult {
            task_id: id,
            context_budget: ContextBudget::new(200_000),
            state: PipelineState::new(id),
            final_diff: String::new(),
            diff_guardwarn: None,
            verdict: Verdict::Approved,
            revision_rounds: 0,
            artifacts: vec![],
            metrics: vec![],
            safety_proof: None,
            isolation: vec![],
            topology: crate::config::types::TopologyMode::MultiAgent,
            topology_reason: String::new(),
            risk_level: "low".to_string(),
            risk_rationale: String::new(),
            test_execution: None,
        }
    }

    fn reviewer_json(verdict: Verdict, category: IssueCategory) -> String {
        let verdict_str = match verdict {
            Verdict::Approved => "approved",
            Verdict::RevisionNeeded => "revision_needed",
            Verdict::Rejected => "rejected",
        };
        let category_str = match category {
            IssueCategory::TestGap => "test_gap",
            IssueCategory::Logic => "logic",
            _ => "style",
        };
        json!({
            "verdict": verdict_str,
            "overall_assessment": "x",
            "quality_scores": {"correctness": 5, "code_quality": 5, "test_coverage": 3, "spec_adherence": 5},
            "issues": [{
                "severity": "major",
                "category": category_str,
                "file_path": "src/a.rs",
                "line_range": "1-2",
                "description": "missing tests for the new branch",
                "suggested_fix": null
            }],
            "strengths": [],
            "feedback": null,
            "red_reconciliation": null
        })
        .to_string()
    }

    #[test]
    fn clean_run_derives_nothing() {
        assert!(derive_learnings("niki-task-x", &empty_result()).is_empty());
    }

    #[test]
    fn test_gap_rejection_derives_verification_failure() {
        let mut result = empty_result();
        result.artifacts.push((
            AgentRole::Reviewer,
            reviewer_json(Verdict::RevisionNeeded, IssueCategory::TestGap),
        ));
        let entries = derive_learnings("niki-task-x", &result);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "verification_failure");
        assert!(entries[0].details.contains("missing tests"));
    }

    #[test]
    fn non_test_rejection_derives_nothing() {
        let mut result = empty_result();
        result.artifacts.push((
            AgentRole::Reviewer,
            reviewer_json(Verdict::RevisionNeeded, IssueCategory::Logic),
        ));
        // "logic" lowercases to "logic" — parses to IssueCategory::Logic.
        assert!(derive_learnings("niki-task-x", &result).is_empty());
    }

    #[test]
    fn critic_reject_derives_review_correction() {
        let mut result = empty_result();
        result.artifacts.push((
            AgentRole::Critic,
            json!({
                "disposition": "reject",
                "summary": "cited file missing",
                "unsupported_claims": ["issue in src/ghost.rs has no diff evidence"],
                "confirmed_findings": []
            })
            .to_string(),
        ));
        let entries = derive_learnings("niki-task-x", &result);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "review_correction");
        assert!(entries[0].details.contains("ghost.rs"));
    }

    #[test]
    fn high_security_finding_derives_security_fix() {
        let mut result = empty_result();
        result.artifacts.push((
            AgentRole::SecurityAuditor,
            json!({
                "verdict": "approved",
                "overall_assessment": "patched in loop",
                "findings": [{
                    "severity": "high",
                    "category": "injection",
                    "file_path": "src/web.rs",
                    "line_range": "9-12",
                    "description": "unescaped query concat",
                    "suggested_fix": null
                }],
                "strengths": []
            })
            .to_string(),
        ));
        let entries = derive_learnings("niki-task-x", &result);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "security_fix");
    }

    #[test]
    fn record_reflections_writes_learnings_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config = NikiConfig::default();
        let mut result = empty_result();
        result.artifacts.push((
            AgentRole::Reviewer,
            reviewer_json(Verdict::RevisionNeeded, IssueCategory::TestGap),
        ));
        let task_dir = tmp.path().join("task");
        let n = record_reflections(tmp.path(), &config, &task_dir, &result);
        assert_eq!(n, 1);
        let content = std::fs::read_to_string(tmp.path().join(".niki/learnings.jsonl")).unwrap();
        assert!(content.contains("verification_failure"));
        assert!(content.contains("niki-task-unknown"));
    }

    #[test]
    fn record_reflections_disabled_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = NikiConfig::default();
        config.repo_intel.enabled = false;
        let mut result = empty_result();
        result.artifacts.push((
            AgentRole::Reviewer,
            reviewer_json(Verdict::RevisionNeeded, IssueCategory::TestGap),
        ));
        assert_eq!(
            record_reflections(tmp.path(), &config, tmp.path(), &result),
            0
        );
    }
}
