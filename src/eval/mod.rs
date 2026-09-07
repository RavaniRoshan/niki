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
use crate::orchestrator::pipeline::{PipelineResult, Task, TopologyMode, execute_pipeline};
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

/// Difficulty level for an evaluation case.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Difficulty {
    Easy,
    #[default]
    Medium,
    Hard,
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
    /// Difficulty level for filtering.
    #[serde(default)]
    pub difficulty: Difficulty,
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
    pub difficulty: Difficulty,
    pub expected_caught: bool,
    pub niki: RunOutcome,
    pub baseline: RunOutcome,
    /// Combined NIKI + baseline measured spend for this case (USD). `0.0` in
    /// replay mode (fixtures predate metering) — cost discipline applies to
    /// live runs, where every stage reports provider-measured usage.
    pub cost_usd: f64,
}

/// A maintainer's merge-worthiness judgment on one case (METR-style:
/// would this diff merge into main, regardless of what the grader said?).
/// Stored as `<dataset-dir>/grades/<case-id>.json` by `niki eval grade`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintainerGrade {
    pub case_id: String,
    pub reviewer: String,
    pub merge_worthy: bool,
    #[serde(default)]
    pub note: String,
    pub date: String,
}

/// Load all maintainer grades from `<dataset-dir>/grades/*.json`.
/// Missing dir (or unreadable files) yields an empty map — grading is opt-in.
pub fn load_grades(
    dataset_dir: &std::path::Path,
) -> std::collections::HashMap<String, MaintainerGrade> {
    let mut grades = std::collections::HashMap::new();
    let dir = dataset_dir.join("grades");
    let entries = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).collect::<Vec<_>>());
    for entry in entries.into_iter().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(grade) = serde_json::from_str::<MaintainerGrade>(&content)
        {
            grades.insert(grade.case_id.clone(), grade);
        }
    }
    grades
}

/// Fraction of graded expected-caught cases where the maintainer's
/// merge-worthiness agrees with NIKI's caught flag. `None` when nothing is
/// graded — an ungraded eval makes no merge-worthiness claim at all.
pub fn grader_agreement(
    cases: &[CaseResult],
    grades: &std::collections::HashMap<String, MaintainerGrade>,
) -> Option<f64> {
    let graded: Vec<&CaseResult> = cases
        .iter()
        .filter(|c| c.expected_caught && grades.contains_key(&c.case_id))
        .collect();
    if graded.is_empty() {
        return None;
    }
    let agree = graded
        .iter()
        .filter(|c| grades[&c.case_id].merge_worthy == c.niki.caught)
        .count();
    Some(agree as f64 / graded.len() as f64)
}

/// Per-category metrics.
#[derive(Debug, Clone, Serialize)]
pub struct CategoryMetrics {
    pub category: IssueCategory,
    pub total: u32,
    pub niki_caught: u32,
    pub baseline_caught: u32,
    pub niki_catch_rate: f64,
    pub baseline_catch_rate: f64,
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
    /// Per-category breakdown.
    pub categories: Vec<CategoryMetrics>,
    /// ISO-8601 date of the run (UTC).
    pub run_date: String,
    /// NIKI crate version that produced this report.
    pub niki_version: String,
    /// `true` when real pipelines ran against live models; `false` for
    /// deterministic fixture replay (zero keys, zero cost).
    pub live: bool,
    /// Best-effort harness commit (`git rev-parse HEAD` in cwd). `None` when
    /// the eval did not run from a git checkout — reproducibility's weakest
    /// link, stated plainly instead of omitted.
    pub harness_commit: Option<String>,
    /// Whether that checkout was dirty when the eval ran.
    pub harness_dirty: bool,
    /// Sum of per-case measured spend (USD). `0.0` for replay runs.
    pub total_cost_usd: f64,
    /// Mean spend per NIKI-caught expected defect. `None` when nothing was
    /// caught or spend is unmeasured — the 2026 cost-disclosure norm is
    /// cost-per-accepted-result, not price-per-call.
    pub cost_per_niki_caught: Option<f64>,
    /// Maintainer merge-worthiness judgments by case id (opt-in human layer).
    pub grades: std::collections::HashMap<String, MaintainerGrade>,
    /// Cases with both expected_caught and a maintainer grade.
    pub graded_cases: u32,
    /// Grader-vs-harness agreement (see [`grader_agreement`]). `None` when
    /// nothing is graded.
    pub grader_agreement: Option<f64>,
}

impl EvalReport {
    /// Total number of cases in the dataset.
    pub fn total_cases(&self) -> u32 {
        self.n_cases.max(self.cases.len() as u32)
    }

    /// NIKI's pass rate: the fraction of expected-caught defects NIKI's
    /// reviewer surfaced (i.e. `1 - false_approval_rate` for NIKI). This is
    /// the headline eval metric surfaced in the UI.
    pub fn pass_rate(&self) -> f64 {
        self.niki_catch_rate
    }

    /// Convenience alias for the same metric.
    pub fn niki_pass_rate(&self) -> f64 {
        self.niki_catch_rate
    }

    /// Whether the eval is healthy enough to declare parity (both runs had
    /// a baseline to compare against and NIKI caught at least the expected
    /// fraction).
    pub fn is_healthy(&self) -> bool {
        self.n_cases > 0 && self.niki_catch_rate > 0.0
    }
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
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading eval dataset {}", path.display()))?;
    let ds: EvalDataset = toml::from_str(&content)
        .with_context(|| format!("parsing eval dataset TOML {}", path.display()))?;
    Ok(ds)
}

// ── Scoring ───────────────────────────────────────────────────────

fn find_artifact(result: &PipelineResult, role: AgentRole) -> Option<&str> {
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
    if let Some(j) = reviewer_json
        && let Ok(rv) = serde_json::from_str::<ReviewVerdict>(j)
    {
        reviewer_issues = rv.issues.len();
        let issue_hit = rv
            .issues
            .iter()
            .any(|i| i.category == cat && kw_match(&i.description, kw))
            || rv.feedback.as_ref().is_some_and(|f| {
                f.critical_issues
                    .iter()
                    .any(|i| i.category == cat && kw_match(&i.description, kw))
            });
        // Fuzzy recall: keyword appearing in the overall assessment still counts.
        let assess_hit = kw.as_ref().is_some_and(|k| {
            rv.overall_assessment
                .to_lowercase()
                .contains(&k.to_lowercase())
        });
        caught_by_reviewer = issue_hit || assess_hit;
    }

    let mut caught_by_red = false;
    let mut red_challenges = 0usize;
    let mut red_upheld = 0usize;
    if let (Some(rj), Some(vj)) = (red_json, reviewer_json)
        && let (Ok(rc), Ok(rv)) = (
            serde_json::from_str::<RedChallenge>(rj),
            serde_json::from_str::<ReviewVerdict>(vj),
        )
    {
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
        caught_by_red = rc
            .challenges
            .iter()
            .any(|c| c.category == cat && kw_match(&c.claim, kw) && upheld.contains(&c.id));
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
        diff_guardwarn: None,
        task_id: id,
        context_budget: PipelineState::new(id).context_budget,
        state: PipelineState::new(id),
        final_diff: String::new(),
        verdict: Verdict::Approved,
        revision_rounds: 0,
        artifacts: Vec::new(),
        metrics: Vec::new(),
        safety_proof: None,
        isolation: Vec::new(),
        topology: TopologyMode::MultiAgent,
        topology_reason: String::new(),
        test_execution: None,
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
        let f = art_dir.join(format!(
            "{}.json",
            crate::artifacts::types::artifact_json_name(role)
        ));
        if f.exists() {
            let json =
                std::fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?;
            artifacts.push((role, json));
        }
    }
    let id = Uuid::new_v4();
    let verdict = find_artifact(
        &PipelineResult {
            diff_guardwarn: None,
            task_id: id,
            context_budget: PipelineState::new(id).context_budget,
            state: PipelineState::new(id),
            final_diff: String::new(),
            verdict: Verdict::Approved,
            revision_rounds: 1,
            artifacts: artifacts.clone(),
            metrics: Vec::new(),
            safety_proof: None,
            isolation: Vec::new(),
            topology: TopologyMode::MultiAgent,
            topology_reason: String::new(),
            test_execution: None,
        },
        AgentRole::Reviewer,
    )
    .and_then(|j| serde_json::from_str::<ReviewVerdict>(j).ok())
    .map(|v| v.verdict)
    .unwrap_or(Verdict::Approved);
    Ok(PipelineResult {
        diff_guardwarn: None,
        task_id: id,
        context_budget: PipelineState::new(id).context_budget,
        state: PipelineState::new(id),
        final_diff: String::new(),
        verdict,
        revision_rounds: 1,
        artifacts,
        metrics: Vec::new(),
        safety_proof: None,
        isolation: Vec::new(),
        topology: TopologyMode::MultiAgent,
        topology_reason: String::new(),
        test_execution: None,
    })
}

/// Replay a case from its recorded NIKI and baseline artifact sets.
/// Returns None if neither NIKI nor baseline fixtures exist (case not yet populated).
pub fn replay_case(case: &EvalCase, dataset_dir: &Path) -> Result<Option<CaseResult>> {
    let base = dataset_dir.join(case.replay_dir.as_deref().unwrap_or("."));
    let niki_dir = base.join("niki");
    let baseline_dir = base.join("baseline");
    // Skip cases where no fixtures exist yet
    if !niki_dir.exists() && !baseline_dir.exists() {
        return Ok(None);
    }
    let niki = replay_result(&niki_dir).unwrap_or_else(|_| empty_result());
    let baseline = replay_result(&baseline_dir).unwrap_or_else(|_| empty_result());
    Ok(Some(CaseResult {
        case_id: case.id.clone(),
        defect_category: case.seeded_defect.category,
        difficulty: case.difficulty,
        expected_caught: case.seeded_defect.expected_caught,
        niki: score_result(&niki, &case.seeded_defect),
        baseline: score_result(&baseline, &case.seeded_defect),
        // Replay fixtures predate per-stage metering: spend is unmeasured (0.0),
        // never zero-cost. Only live runs populate cost_usd.
        cost_usd: 0.0,
    }))
}

// ── Live (real pipeline) ──────────────────────────────────────────

/// Drive the real pipeline for both NIKI and baseline configs on one case.
pub async fn run_case_live(
    case: &EvalCase,
    base: &NikiConfig,
    project_dir: &Path,
) -> Result<CaseResult> {
    let niki_cfg = niki_config(base);
    let base_cfg = baseline_config(base);

    let mut display = AgenticDisplay::new();
    let containers: ActiveContainers = Arc::new(Mutex::new(Vec::new()));

    let niki_task = Task {
        id: Uuid::new_v4(),
        description: case.description.clone(),
        project_path: project_dir.to_path_buf(),
    };
    let niki_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let niki_res = execute_pipeline(
        &niki_task,
        &niki_cfg,
        None,
        &mut display,
        containers.clone(),
        false,
        niki_cancel.clone(),
        &niki_task
            .project_path
            .join(&niki_cfg.general.output_dir)
            .join("tasks")
            .join(niki_task.id.to_string()),
        None,
        false,
    )
    .await?;

    let base_task = Task {
        id: Uuid::new_v4(),
        description: case.description.clone(),
        project_path: project_dir.to_path_buf(),
    };
    let base_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let base_res = execute_pipeline(
        &base_task,
        &base_cfg,
        None,
        &mut display,
        containers.clone(),
        false,
        base_cancel.clone(),
        &base_task
            .project_path
            .join(&base_cfg.general.output_dir)
            .join("tasks")
            .join(base_task.id.to_string()),
        None,
        false,
    )
    .await?;

    Ok(CaseResult {
        case_id: case.id.clone(),
        defect_category: case.seeded_defect.category,
        difficulty: case.difficulty,
        expected_caught: case.seeded_defect.expected_caught,
        niki: score_result(&niki_res, &case.seeded_defect),
        baseline: score_result(&base_res, &case.seeded_defect),
        cost_usd: niki_res.metrics.iter().map(|m| m.cost_usd).sum::<f64>()
            + base_res.metrics.iter().map(|m| m.cost_usd).sum::<f64>(),
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
    let base_cfg = if live {
        Some(NikiConfig::load(project_dir)?)
    } else {
        None
    };

    let mut cases = Vec::new();
    for case in &ds.cases {
        let cr = if live {
            let cfg = base_cfg
                .clone()
                .context("could not load config for live eval")?;
            Some(run_case_live(case, &cfg, project_dir).await?)
        } else {
            replay_case(case, &dataset_dir)?
        };
        if let Some(cr) = cr {
            cases.push(cr);
        }
    }
    let grades = load_grades(&dataset_dir);
    Ok(build_report(&ds, &cases, live, &grades))
}

/// Best-effort harness provenance for the disclosure manifest: current UTC
/// date, crate version, and (when run from a git checkout) commit + dirtiness.
pub fn harness_provenance() -> (String, String, Option<String>, bool) {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let version = env!("CARGO_PKG_VERSION").to_string();
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let dirty = commit
        .as_ref()
        .map(|_| {
            std::process::Command::new("git")
                .args(["status", "--porcelain", "--untracked-files=no"])
                .output()
                .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    (date, version, commit, dirty)
}

/// Aggregate per-case outcomes into the publishable delta.
pub fn build_report(
    ds: &EvalDataset,
    cases: &[CaseResult],
    live: bool,
    grades: &std::collections::HashMap<String, MaintainerGrade>,
) -> EvalReport {
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

    // Per-category metrics
    let mut category_map: std::collections::HashMap<IssueCategory, Vec<&CaseResult>> =
        std::collections::HashMap::new();
    for c in &expected {
        category_map.entry(c.defect_category).or_default().push(c);
    }
    let mut categories: Vec<CategoryMetrics> = category_map
        .into_iter()
        .map(|(cat, cs)| {
            let total = cs.len() as u32;
            let niki_caught = cs.iter().filter(|c| c.niki.caught).count() as u32;
            let baseline_caught = cs.iter().filter(|c| c.baseline.caught).count() as u32;
            CategoryMetrics {
                category: cat,
                total,
                niki_caught,
                baseline_caught,
                niki_catch_rate: if total > 0 {
                    niki_caught as f64 / total as f64
                } else {
                    0.0
                },
                baseline_catch_rate: if total > 0 {
                    baseline_caught as f64 / total as f64
                } else {
                    0.0
                },
            }
        })
        .collect();
    categories.sort_by(|a, b| format!("{:?}", a.category).cmp(&format!("{:?}", b.category)));

    let total_cost_usd: f64 = cases.iter().map(|c| c.cost_usd).sum();
    let niki_caught_n = expected.iter().filter(|c| c.niki.caught).count();
    let cost_per_niki_caught = if niki_caught_n > 0 && total_cost_usd > 0.0 {
        Some(total_cost_usd / niki_caught_n as f64)
    } else {
        None
    };
    let (run_date, niki_version, harness_commit, harness_dirty) = harness_provenance();

    let graded_cases = cases
        .iter()
        .filter(|c| c.expected_caught && grades.contains_key(&c.case_id))
        .count() as u32;

    EvalReport {
        dataset: ds.name.clone().unwrap_or_else(|| "eval".to_string()),
        n_cases: cases.len() as u32,
        cases: cases.to_vec(),
        niki_catch_rate: niki_caught / n,
        baseline_catch_rate: baseline_caught / n,
        niki_false_approvals: niki_fa,
        baseline_false_approvals: baseline_fa,
        false_approval_reduction_pct,
        categories,
        run_date,
        niki_version,
        live,
        harness_commit,
        harness_dirty,
        total_cost_usd,
        cost_per_niki_caught,
        grades: grades.clone(),
        graded_cases,
        grader_agreement: grader_agreement(cases, grades),
    }
}

/// Render the report as Markdown for human reading / publishing.
pub fn render_report_md(report: &EvalReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# NIKI Evaluation Report — {}\n\n",
        report.dataset
    ));
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
    if report.live {
        s.push_str(&format!(
            "| Measured spend (both configs) | ${:.4} |  |\n",
            report.total_cost_usd
        ));
    } else {
        s.push_str(
            "| Measured spend (both configs) | $0.0000 (replay: unmeasured, not free) |  |\n",
        );
    }
    if let Some(c) = report.cost_per_niki_caught {
        s.push_str(&format!("| Cost per NIKI-caught defect | ${:.4} |  |\n", c));
    }
    match report.grader_agreement {
        Some(a) => s.push_str(&format!(
            "| Grader agreement ({} graded) | {:.0}% |  |\n",
            report.graded_cases,
            a * 100.0
        )),
        None => s.push_str("| Grader agreement | ungraded (no maintainer judgments) |  |\n"),
    }

    s.push_str("\n## Disclosure manifest\n\n");
    s.push_str(&format!(
        "- Date (UTC): {}\n- NIKI version: {}\n- Mode: {}\n",
        report.run_date,
        report.niki_version,
        if report.live {
            "live (real pipelines, API keys, sandbox)"
        } else {
            "replay (recorded fixtures; deterministic, zero cost)"
        },
    ));
    match &report.harness_commit {
        Some(c) => s.push_str(&format!(
            "- Harness commit: {}{}\n",
            c,
            if report.harness_dirty {
                " (DIRTY tree — reproduce with caution)"
            } else {
                ""
            }
        )),
        None => s.push_str("- Harness commit: unknown (eval did not run from a git checkout)\n"),
    }
    s.push_str(
        "- Success definition: seeded defect surfaced by reviewer issues or an \
         upheld Red challenge (test-passing only, not maintainer-merge grading — \
         see research report VG-12).\n",
    );

    // Per-category breakdown
    if !report.categories.is_empty() {
        s.push_str("\n## Per-category\n\n");
        s.push_str(
            "| Category | Total | NIKI caught | Baseline caught | NIKI rate | Baseline rate |\n",
        );
        s.push_str("|---|---|---|---|---|---|\n");
        for cat in &report.categories {
            s.push_str(&format!(
                "| {:?} | {} | {} | {} | {:.0}% | {:.0}% |\n",
                cat.category,
                cat.total,
                cat.niki_caught,
                cat.baseline_caught,
                cat.niki_catch_rate * 100.0,
                cat.baseline_catch_rate * 100.0
            ));
        }
    }

    s.push_str("\n## Per-case\n\n");
    s.push_str(
        "| Case | Defect | Expected | NIKI caught | by Red | by Reviewer | Baseline caught |\n",
    );
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
            diff_guardwarn: None,
            task_id: id,
            context_budget: PipelineState::new(id).context_budget,
            state: PipelineState::new(id),
            final_diff: String::new(),
            verdict: Verdict::Approved,
            revision_rounds: 1,
            artifacts: parts.iter().map(|(r, j)| (*r, j.to_string())).collect(),
            metrics: Vec::new(),
            safety_proof: None,
            isolation: vec![],
            topology: TopologyMode::MultiAgent,
            topology_reason: String::new(),
            test_execution: None,
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
            difficulty: Difficulty::Medium,
            expected_caught: true,
            niki: caught(true, true, true),
            baseline: caught(false, false, false),
            cost_usd: 0.0,
        };
        let c2 = CaseResult {
            case_id: "b".into(),
            defect_category: IssueCategory::Logic,
            difficulty: Difficulty::Medium,
            expected_caught: true,
            niki: caught(true, false, true),
            baseline: caught(true, false, true),
            cost_usd: 0.0,
        };
        let ds = EvalDataset {
            name: Some("t".into()),
            cases: vec![],
        };
        let rep = build_report(&ds, &[c1, c2], false, &std::collections::HashMap::new());
        assert_eq!(rep.niki_catch_rate, 1.0);
        assert_eq!(rep.baseline_catch_rate, 0.5);
        assert_eq!(rep.baseline_false_approvals, 1);
        assert_eq!(rep.niki_false_approvals, 0);
        assert_eq!(rep.false_approval_reduction_pct, 100.0);
        assert_eq!(rep.categories.len(), 2);
    }

    #[test]
    fn replay_fixtures_show_niki_delta() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let dataset_dir = dir.join("evals");
        let ds = load_dataset(&dataset_dir.join("dataset.toml")).unwrap();
        let mut cases = Vec::new();
        for c in &ds.cases {
            if let Ok(Some(cr)) = replay_case(c, &dataset_dir) {
                cases.push(cr);
            }
        }
        let rep = build_report(&ds, &cases, false, &std::collections::HashMap::new());
        // All 23 fixture cases should be loaded
        assert_eq!(rep.n_cases, 23, "expected all 23 fixture cases");
        // NIKI catches every defect (reviewer or red surfaces the seeded defect)
        assert_eq!(rep.niki_catch_rate, 1.0);
        // Baselines miss all defects (approved with empty issues = false approvals)
        assert_eq!(rep.baseline_catch_rate, 0.0);
        // Since baselines miss everything NIKI catches, false-approval reduction is 100%
        assert_eq!(rep.false_approval_reduction_pct, 100.0);
        assert!(!rep.categories.is_empty());
        let md = render_report_md(&rep);
        assert!(md.contains("reducing reviewer"));
        assert!(md.contains("Per-category"));
        // The SQL case must be a baseline false-approval that NIKI caught via Red.
        let sql = cases.iter().find(|c| c.case_id == "defect-sql").unwrap();
        assert!(!sql.baseline.caught);
        assert!(sql.niki.caught_by_red);
    }

    #[test]
    fn grader_agreement_counts_only_graded_expected() {
        use std::collections::HashMap;
        let mk = |id: &str, hit: bool| CaseResult {
            case_id: id.into(),
            defect_category: IssueCategory::Security,
            difficulty: Difficulty::Medium,
            expected_caught: true,
            niki: caught(true, hit, true),
            baseline: caught(false, false, false),
            cost_usd: 0.0,
        };
        let grade = |id: &str, merge: bool| MaintainerGrade {
            case_id: id.into(),
            reviewer: "r".into(),
            merge_worthy: merge,
            note: String::new(),
            date: "2026-09-07".into(),
        };
        let cases = vec![mk("a", true), mk("b", true), mk("c", true)];
        // Nothing graded: no claim.
        assert_eq!(grader_agreement(&cases, &HashMap::new()), None);
        let mut grades = HashMap::new();
        grades.insert("a".into(), grade("a", true)); // agree
        grades.insert("b".into(), grade("b", false)); // disagree
        assert_eq!(grader_agreement(&cases, &grades), Some(0.5));
        let rep = build_report(
            &EvalDataset {
                name: Some("t".into()),
                cases: vec![],
            },
            &cases,
            false,
            &grades,
        );
        assert_eq!(rep.graded_cases, 2);
        assert_eq!(rep.grader_agreement, Some(0.5));
    }

    #[test]
    fn report_carries_provenance_and_cost_discipline() {
        let c1 = CaseResult {
            case_id: "a".into(),
            defect_category: IssueCategory::Security,
            difficulty: Difficulty::Medium,
            expected_caught: true,
            niki: caught(true, true, true),
            baseline: caught(false, false, false),
            cost_usd: 0.5,
        };
        let ds = EvalDataset {
            name: Some("t".into()),
            cases: vec![],
        };
        let rep = build_report(&ds, &[c1], true, &std::collections::HashMap::new());
        assert!(rep.live);
        assert_eq!(rep.niki_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(rep.run_date.len(), 10);
        assert_eq!(rep.total_cost_usd, 0.5);
        assert_eq!(rep.cost_per_niki_caught, Some(0.5));
        // Replay semantics: unmeasured spend means no per-caught figure.
        let rep_replay = build_report(&ds, &[], false, &std::collections::HashMap::new());
        assert!(!rep_replay.live);
        assert_eq!(rep_replay.total_cost_usd, 0.0);
        assert_eq!(rep_replay.cost_per_niki_caught, None);
        let md = render_report_md(&rep);
        assert!(md.contains("Disclosure manifest"));
        assert!(md.contains("Cost per NIKI-caught defect"));
    }
}
