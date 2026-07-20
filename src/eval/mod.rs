//! Evaluation harness (BUILD_PLAN 1.3, P0.1).
//!
//! The thesis NIKI makes — that *isolated* agents which genuinely challenge each
//! other beat a single reviewer — needs to be **proven with data**, not asserted.
//! This harness runs every case in a dataset two ways:
//!
//!   • **NIKI**      — the full pipeline with the adversarial Red/Blue review (#1.2)
//!                     enabled (Red agent probes the diff, Reviewer must reconcile).
//!   • **Baseline**  — the same pipeline with Red/Blue disabled: a single reviewer
//!                     with no independent adversarial critique, the thing NIKI
//!                     competes against.
//!
//! For each case it knows the seeded defect (category + optional keyword + whether
//! a correct reviewer *should* catch it), scores both runs, and publishes the
//! delta — most importantly the **reviewer false-approval reduction**: defects the
//! baseline rubber-stamped that NIKI's Red/Blue loop caught.
//!
//! The harness is *repeatable* two ways:
//!   • `replay` mode (default): consumes pre-recorded agent artifacts from each
//!     case's fixture directory, so `niki eval` runs deterministically in CI with
//!     no API keys and zero LLM cost. This is the acceptance-critical path.
//!   • `live` mode (`--live`): drives the real `execute_pipeline` for both the NIKI
//!     and baseline configs against live models (needs API keys + a sandbox).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::artifacts::types::{
    AgentRole, IssueCategory, RedChallenge, RedDisposition, ReviewVerdict, Verdict,
};
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::orchestrator::pipeline::{execute_pipeline, PipelineResult, Task, TopologyMode};
use crate::orchestrator::state::PipelineState;
use crate::sandbox::{ActiveContainers, SandboxBackend};

// ── Dataset types ─────────────────────────────────────────────────

/// A defect we deliberately injected into a task so we can measure whether the
/// review process surfaces it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeededDefect {
    /// Human-readable description of what was seeded.
    pub label: String,
    /// The issue category the seeded defect maps to (e.g. `security`, `logic`).
    pub category: IssueCategory,
    /// Optional token that must appear in the catching challenge/issue text.
    /// Lets us disambiguate a generic "security" flag from *this* defect.
    #[serde(default)]
    pub keyword: Option<String>,
    /// Whether a correct reviewer should catch it (true for real defects).
    /// Drives the catch-rate / false-approval math.
    pub expected_caught: bool,
}

/// One evaluation case: a task + its known seeded defect + where to find the
/// recorded artifacts for replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub description: String,
    pub seeded_defect: SeededDefect,
    /// Directory (relative to the dataset file) holding `niki/artifacts/*.json`
    /// and `baseline/artifacts/*.json` for replay mode.
    #[serde(default)]
    pub replay_dir: Option<String>,
}

/// An evaluation dataset: a named collection of cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDataset {
    #[serde(default)]
    pub name: Option<String>,
    pub cases: Vec<EvalCase>,
}

// ── Outcome / report types ────────────────────────────────────────

/// How one configuration (NIKI or baseline) performed on one case.
#[derive(Debug, Clone, Serialize)]
pub struct RunOutcome {
    /// The seeded defect was surfaced by this configuration.
    pub caught: bool,
    /// Surfaced specifically via an *upheld* Red challenge.
    pub caught_by_red: bool,
    /// Surfaced via the Reviewer's own issues/feedback.
    pub caught_by_reviewer: bool,
    /// The Reviewer's final verdict.
    pub verdict: Verdict,
    pub reviewer_issues: usize,
    pub red_challenges: usize,
    pub red_upheld: usize,
}

/// The paired result for one case: NIKI vs baseline.
#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub case_id: String,
    pub defect_category: IssueCategory,
    pub expected_caught: bool,
    pub niki: RunOutcome,
    pub baseline: RunOutcome,
}

/// The aggregate, publishable delta.
#[derive(Debug, Clone, Serialize)]
pub struct EvalReport {
    pub dataset: String,
    pub n_cases: u32,
    pub cases: Vec<CaseResult>,
    /// Fraction of expected-caught defects NIKI surfaced.
    pub niki_catch_rate: f64,
    /// Fraction of expected-caught defects the baseline surfaced.
    pub baseline_catch_rate: f64,
    /// Count of expected-caught defects the NIKI run failed to surface.
    pub niki_false_approvals: u32,
    /// Count of expected-caught defects the baseline run failed to surface
    /// (i.e. the baseline's reviewer false-approvals).
    pub baseline_false_approvals: u32,
    /// `(baseline_fa - niki_fa) / baseline_fa * 100` — the headline metric.
    pub false_approval_reduction_pct: f64,
}

// ── Config builders ───────────────────────────────────────────────

/// NIKI configuration: adversarial Red/Blue review on, other optional passes off
/// (we isolate the Red/Blue contribution), worktree backend so no Docker needed.
pub fn niki_config(base: &NikiConfig) -> NikiConfig {
    let mut c = base.clone();
    c.red_blue.enabled = true;
    c.parallel.enabled = false;
    c.security.enabled = false;
    c.docker.backend = SandboxBackend::Worktree;
    c
}

/// Baseline configuration: same pipeline, Red/Blue disabled — a lone reviewer
/// with no independent adversarial critique. This is the thing NIKI competes with.
pub fn baseline_config(base: &NikiConfig) -> NikiConfig {
    let mut c = base.clone();
    c.red_blue.enabled = false;
    c.parallel.enabled = false;
    c.security.enabled = false;
    c.docker.backend = SandboxBackend::Worktree;
    c
}

// ── Dataset loading ───────────────────────────────────────────────

pub fn load_dataset(path: &Path) -> Result<EvalDataset> {
    let content = std::fs::read_to_string(path).with_context(|| format!("reading eval dataset {}", path.display()))?;
    let ds: EvalDataset = toml::from_str(&content).with_context(|| format!("parsing eval dataset TOML {}", path.display()))?;
    Ok(ds)
}

// ── Scoring ───────────────────────────────────────────────────────

fn find_artifact<'a>(result: &'a PipelineResult, role: AgentRole) -> Option<&'a str> {
    result
        .artifacts
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, j)| j.as_str())
}

fn kw_match(haystack: &str, keyword: &Option<String>) -> bool {
    match keyword {
        Some(k) => haystack.to_lowercase().contains(&k.to_lowercase()),
        None => true,
    }
}

/// Score one pipeline run against a seeded defect.
///
/// A defect is "caught" if either:
///   • a Red challenge of the matching category (and keyword) was **upheld** by
///     the Reviewer (`caught_by_red`), or
///   • the Reviewer's own issues/feedback cite the matching category (and keyword)
///     (`caught_by_reviewer`).
pub fn score_result(result: &PipelineResult, defect: &SeededDefect) -> RunOutcome {
    let reviewer_json = find_artifact(result, AgentRole::Reviewer);
    let red_json = find_artifact(result, AgentRole::Red);
    let cat = defect.category;
    let kw = &defect.keyword;

    let verdict = reviewer_json
        .and_then(|j| serde_json::from_str::<ReviewVerdict>(j).ok())
        .map(|v| v.verdict)
        .unwrap_or(Verdict::Approved);

    let mut caught_by_reviewer = false;
    let mut reviewer_issues = 0usize;
    if let Some(j) = reviewer_json {
        if let Ok(rv) = serde_json::from_str::<ReviewVerdict>(j) {
            reviewer_issues = rv.issues.len();
            let issue_hit = rv
                .issues
                .iter()
                .any(|i| i.category == cat && kw_match(&i.description, kw))
                || rv.feedback.as_ref().map_or(false, |f| {
                    f.critical_issues
                        .iter()
                        .any(|i| i.category == cat && kw_match(&i.description, kw))
                });
            // Fuzzy recall: keyword appearing in the overall assessment still counts.
            let assess_hit = kw.as_ref().map_or(false, |k| rv.overall_assessment.to_lowercase().contains(&k.to_lowercase()));
            caught_by_reviewer = issue_hit || assess_hit;
        }
    }

    let mut caught_by_red = false;
    let mut red_challenges = 0usize;
    let mut red_upheld = 0usize;
    if let (Some(rj), Some(vj)) = (red_json, reviewer_json) {
        if let (Ok(rc), Ok(rv)) = (
            serde_json::from_str::<RedChallenge>(rj),
            serde_json::from_str::<ReviewVerdict>(vj),
        ) {
            red_challenges = rc.challenges.len();
            let upheld: HashSet<String> = rv
                .red_reconciliation
                .as_ref()
                .map(|rs| {
                    rs.iter()
                        .filter(|r| r.disposition == RedDisposition::Upheld)
                        .map(|r| r.challenge_id.clone())
                        .collect()
                })
                .unwrap_or_default();
            red_upheld = upheld.len();
            caught_by_red = rc.challenges.iter().any(|c| {
                c.category == cat && kw_match(&c.claim, kw) && upheld.contains(&c.id)
            });
        }
    }

    RunOutcome {
        caught: caught_by_red || caught_by_reviewer,
        caught_by_red,
        caught_by_reviewer,
        verdict,
        reviewer_issues,
        red_challenges,
        red_upheld,
    }
}

// ── Replay (offline, deterministic) ──────────────────────────────

fn empty_result() -> PipelineResult {
    let id = Uuid::new_v4();
    PipelineResult {
        task_id: id,
        state: PipelineState::new(id),
        final_diff: String::new(),
        verdict: Verdict::Approved,
        revision_rounds: 0,
        artifacts: Vec::new(),
        metrics: Vec::new(),
        safety_proof: None,
        isolation: Vec::new(),
        topology: TopologyMode::MultiAgent,
    }
}

fn replay_result(dir: &Path) -> Result<PipelineResult> {
    let art_dir = dir.join("artifacts");
    if !art_dir.exists() {
        anyhow::bail!("no artifacts dir at {}", art_dir.display());
    }
    let mut artifacts = Vec::new();
    for role in [
        AgentRole::Planner,
        AgentRole::Coder,
        AgentRole::Tester,
        AgentRole::Red,
        AgentRole::Reviewer,
    ] {
        let f = art_dir.join(format!("{}.json", crate::artifacts::types::artifact_json_name(role)));
        if f.exists() {
            let json = std::fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?;
            artifacts.push((role, json));
        }
    }
    let id = Uuid::new_v4();
    let verdict = find_artifact(&PipelineResult {
        task_id: id,
        state: PipelineState::new(id),
        final_diff: String::new(),
        verdict: Verdict::Approved,
        revision_rounds: 1,
        artifacts: artifacts.clone(),
        metrics: Vec::new(),
        safety_proof: None,
        isolation: Vec::new(),
        topology: TopologyMode::MultiAgent,
    }, AgentRole::Reviewer)
    .and_then(|j| serde_json::from_str::<ReviewVerdict>(j).ok())
    .map(|v| v.verdict)
    .unwrap_or(Verdict::Approved);
    Ok(PipelineResult {
        task_id: id,
        state: PipelineState::new(id),
        final_diff: String::new(),
        verdict,
        revision_rounds: 1,
        artifacts,
        metrics: Vec::new(),
        safety_proof: None,
        isolation: Vec::new(),
        topology: TopologyMode::MultiAgent,
    })
}

/// Replay a case from its recorded NIKI and baseline artifact sets.
pub fn replay_case(case: &EvalCase, dataset_dir: &Path) -> Result<CaseResult> {
    let base = dataset_dir.join(case.replay_dir.as_deref().unwrap_or("."));
    let niki = replay_result(&base.join("niki")).unwrap_or_else(|_| empty_result());
    let baseline = replay_result(&base.join("baseline")).unwrap_or_else(|_| empty_result());
    Ok(CaseResult {
        case_id: case.id.clone(),
        defect_category: case.seeded_defect.category,
        expected_caught: case.seeded_defect.expected_caught,
        niki: score_result(&niki, &case.seeded_defect),
        baseline: score_result(&baseline, &case.seeded_defect),
    })
}

// ── Live (real pipeline) ──────────────────────────────────────────

/// Drive the real pipeline for both NIKI and baseline configs on one case.
pub async fn run_case_live(case: &EvalCase, base: &NikiConfig, project_dir: &Path) -> Result<CaseResult> {
    let niki_cfg = niki_config(base);
    let base_cfg = baseline_config(base);

    let mut display = AgenticDisplay::new();
    let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));

    let niki_task = Task {
        id: Uuid::new_v4(),
        description: case.description.clone(),
        project_path: project_dir.to_path_buf(),
    };
    let niki_res = execute_pipeline(&niki_task, &niki_cfg, None, &mut display, containers.clone(), false).await?;

    let base_task = Task {
        id: Uuid::new_v4(),
        description: case.description.clone(),
        project_path: project_dir.to_path_buf(),
    };
    let base_res = execute_pipeline(&base_task, &base_cfg, None, &mut display, containers.clone(), false).await?;

    Ok(CaseResult {
        case_id: case.id.clone(),
        defect_category: case.seeded_defect.category,
        expected_caught: case.seeded_defect.expected_caught,
        niki: score_result(&niki_res, &case.seeded_defect),
        baseline: score_result(&base_res, &case.seeded_defect),
    })
}

// ── Top-level run + reporting ─────────────────────────────────────

/// Run the whole dataset. `live` drives real pipelines (needs keys); otherwise it
/// replays recorded fixtures deterministically.
pub async fn run_eval(dataset_path: &Path, live: bool, project_dir: &Path) -> Result<EvalReport> {
    let ds = load_dataset(dataset_path)?;
    let dataset_dir = dataset_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let base_cfg = if live { Some(NikiConfig::load(project_dir)?) } else { None };

    let mut cases = Vec::new();
    for case in &ds.cases {
        let cr = if live {
            let cfg = base_cfg.clone().context("could not load config for live eval")?;
            run_case_live(case, &cfg, project_dir).await?
        } else {
            replay_case(case, &dataset_dir)?
        };
        cases.push(cr);
    }
    Ok(build_report(&ds, &cases))
}

/// Aggregate per-case outcomes into the publishable delta.
pub fn build_report(ds: &EvalDataset, cases: &[CaseResult]) -> EvalReport {
    let expected: Vec<&CaseResult> = cases.iter().filter(|c| c.expected_caught).collect();
    let n = expected.len().max(1) as f64;
    let niki_caught = expected.iter().filter(|c| c.niki.caught).count() as f64;
    let baseline_caught = expected.iter().filter(|c| c.baseline.caught).count() as f64;
    let niki_fa = expected.iter().filter(|c| !c.niki.caught).count() as u32;
    let baseline_fa = expected.iter().filter(|c| !c.baseline.caught).count() as u32;
    let false_approval_reduction_pct = if baseline_fa > 0 {
        ((baseline_fa - niki_fa) as f64 / baseline_fa as f64) * 100.0
    } else {
        0.0
    };

    EvalReport {
        dataset: ds.name.clone().unwrap_or_else(|| "eval".to_string()),
        n_cases: cases.len() as u32,
        cases: cases.to_vec(),
        niki_catch_rate: niki_caught / n,
        baseline_catch_rate: baseline_caught / n,
        niki_false_approvals: niki_fa,
        baseline_false_approvals: baseline_fa,
        false_approval_reduction_pct,
    }
}

/// Render the report as Markdown for human reading / publishing.
pub fn render_report_md(report: &EvalReport) -> String {
    let mut s = String::new();
    s.push_str(&format!("# NIKI Evaluation Report — {}\n\n", report.dataset));
    s.push_str(&format!("Cases evaluated: {}\n\n", report.n_cases));

    s.push_str("## Summary\n\n");
    s.push_str("| Metric | NIKI (Red/Blue) | Baseline (single reviewer) |\n");
    s.push_str("|---|---|---|\n");
    s.push_str(&format!(
        "| Catch rate (seeded defects) | {:.0}% | {:.0}% |\n",
        report.niki_catch_rate * 100.0,
        report.baseline_catch_rate * 100.0
    ));
    s.push_str(&format!(
        "| Reviewer false-approvals | {} | {} |\n",
        report.niki_false_approvals, report.baseline_false_approvals
    ));
    s.push_str(&format!(
        "| False-approval reduction | — | {:.0}% |\n",
        report.false_approval_reduction_pct
    ));

    s.push_str("\n## Per-case\n\n");
    s.push_str("| Case | Defect | Expected | NIKI caught | by Red | by Reviewer | Baseline caught |\n");
    s.push_str("|---|---|---|---|---|---|---|\n");
    for c in &report.cases {
        s.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} |\n",
            c.case_id,
            c.defect_category,
            c.expected_caught,
            c.niki.caught,
            c.niki.caught_by_red,
            c.niki.caught_by_reviewer,
            c.baseline.caught
        ));
    }

    s.push_str("\n## Headline\n\n");
    s.push_str(&format!(
        "On a dataset of {} seeded-defect tasks, NIKI's adversarial Red/Blue review caught {:.0}% \
         of defects versus {:.0}% for the single-reviewer baseline, reducing reviewer \
         false-approvals by {:.0}%.\n",
        report.n_cases,
        report.niki_catch_rate * 100.0,
        report.baseline_catch_rate * 100.0,
        report.false_approval_reduction_pct
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn result_from(parts: &[(AgentRole, &str)]) -> PipelineResult {
        let id = Uuid::new_v4();
        PipelineResult {
            task_id: id,
            state: PipelineState::new(id),
            final_diff: String::new(),
            verdict: Verdict::Approved,
            revision_rounds: 1,
            artifacts: parts.iter().map(|(r, j)| (*r, j.to_string())).collect(),
            metrics: Vec::new(),
            safety_proof: None,
            isolation: vec![],
            topology: TopologyMode::MultiAgent,
        }
    }

    fn caught(c: bool, by_red: bool, by_rev: bool) -> RunOutcome {
        RunOutcome {
            caught: c,
            caught_by_red: by_red,
            caught_by_reviewer: by_rev,
            verdict: Verdict::RevisionNeeded,
            reviewer_issues: 1,
            red_challenges: 1,
            red_upheld: if by_red { 1 } else { 0 },
        }
    }

    fn security_defect() -> SeededDefect {
        SeededDefect {
            label: "x".into(),
            category: IssueCategory::Security,
            keyword: Some("injection".into()),
            expected_caught: true,
        }
    }

    #[test]
    fn niki_red_catch_detected() {
        let defect = security_defect();
        let red = r#"{"overall_red_assessment":"","challenges":[{"id":"R1","severity":"critical","category":"security","claim":"SQL injection risk","confidence":9,"evidence":null,"suggested_check":null}]}"#;
        let rev = r#"{"verdict":"revision_needed","overall_assessment":"","quality_scores":{"correctness":1,"code_quality":1,"test_coverage":1,"spec_adherence":1},"issues":[{"severity":"critical","category":"security","file_path":null,"line_range":null,"description":"SQL injection here","suggested_fix":null}],"strengths":[],"feedback":null,"red_reconciliation":[{"challenge_id":"R1","disposition":"upheld","rationale":"yes"}]}"#;
        let r = result_from(&[(AgentRole::Red, red), (AgentRole::Reviewer, rev)]);
        let o = score_result(&r, &defect);
        assert!(o.caught_by_red);
        assert!(o.caught_by_reviewer);
        assert!(o.caught);
    }

    #[test]
    fn baseline_misses_without_red_and_issue() {
        let defect = security_defect();
        let rev = r#"{"verdict":"approved","overall_assessment":"looks good","quality_scores":{"correctness":5,"code_quality":5,"test_coverage":5,"spec_adherence":5},"issues":[],"strengths":[],"feedback":null,"red_reconciliation":null}"#;
        let r = result_from(&[(AgentRole::Reviewer, rev)]);
        let o = score_result(&r, &defect);
        assert!(!o.caught_by_red);
        assert!(!o.caught_by_reviewer);
        assert!(!o.caught);
        assert_eq!(o.verdict, Verdict::Approved);
    }

    #[test]
    fn refuted_red_does_not_count_as_caught() {
        let defect = security_defect();
        let red = r#"{"overall_red_assessment":"","challenges":[{"id":"R1","severity":"critical","category":"security","claim":"SQL injection risk","confidence":9,"evidence":null,"suggested_check":null}]}"#;
        let rev = r#"{"verdict":"approved","overall_assessment":"","quality_scores":{"correctness":5,"code_quality":5,"test_coverage":5,"spec_adherence":5},"issues":[],"strengths":[],"feedback":null,"red_reconciliation":[{"challenge_id":"R1","disposition":"refuted","rationale":"already parameterized"}]}"#;
        let r = result_from(&[(AgentRole::Red, red), (AgentRole::Reviewer, rev)]);
        let o = score_result(&r, &defect);
        assert!(!o.caught_by_red);
        assert!(!o.caught);
    }

    #[test]
    fn config_builders_toggle_red_blue() {
        let base = NikiConfig::default();
        assert!(niki_config(&base).red_blue.enabled);
        assert!(!baseline_config(&base).red_blue.enabled);
        assert_eq!(niki_config(&base).docker.backend, SandboxBackend::Worktree);
    }

    #[test]
    fn report_math_reduction() {
        let c1 = CaseResult {
            case_id: "a".into(),
            defect_category: IssueCategory::Security,
            expected_caught: true,
            niki: caught(true, true, true),
            baseline: caught(false, false, false),
        };
        let c2 = CaseResult {
            case_id: "b".into(),
            defect_category: IssueCategory::Logic,
            expected_caught: true,
            niki: caught(true, false, true),
            baseline: caught(true, false, true),
        };
        let ds = EvalDataset {
            name: Some("t".into()),
            cases: vec![],
        };
        let rep = build_report(&ds, &[c1, c2]);
        assert_eq!(rep.niki_catch_rate, 1.0);
        assert_eq!(rep.baseline_catch_rate, 0.5);
        assert_eq!(rep.baseline_false_approvals, 1);
        assert_eq!(rep.niki_false_approvals, 0);
        assert_eq!(rep.false_approval_reduction_pct, 100.0);
    }

    #[test]
    fn replay_fixtures_show_niki_delta() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dataset_dir = dir.join("evals");
        let ds = load_dataset(&dataset_dir.join("dataset.toml")).unwrap();
        let mut cases = Vec::new();
        for c in &ds.cases {
            cases.push(replay_case(c, &dataset_dir).unwrap());
        }
        let rep = build_report(&ds, &cases);
        assert_eq!(rep.n_cases, 2);
        assert_eq!(rep.niki_catch_rate, 1.0);
        assert_eq!(rep.baseline_catch_rate, 0.5);
        assert_eq!(rep.false_approval_reduction_pct, 100.0);
        let md = render_report_md(&rep);
        assert!(md.contains("reducing reviewer"));
        // The SQL case must be a baseline false-approval that NIKI caught via Red.
        let sql = cases.iter().find(|c| c.case_id == "defect-sql").unwrap();
        assert!(!sql.baseline.caught);
        assert!(sql.niki.caught_by_red);
    }
}
