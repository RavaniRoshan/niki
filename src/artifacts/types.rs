use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Envelope wrapping every artifact with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEnvelope<T: Serialize> {
    pub id: Uuid,
    pub task_id: Uuid,
    pub agent: AgentRole,
    pub artifact_type: ArtifactType,
    pub created_at: DateTime<Utc>,
    pub revision_round: u32,          // 0 for first pass, increments on feedback loops
    pub payload: T,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Coder,
    Tester,
    Reviewer,
    /// Merges N parallel coder diffs into a single coherent change (#3).
    Synthesizer,
    /// Independent security review pass (#4).
    SecurityAuditor,
    /// Adversarial "Red" agent (#1.2): independently probes the Coder's diff for
    /// defects and assumptions so the Reviewer must genuinely challenge code
    /// instead of ratifying it (guards sycophantic convergence).
    Red,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    TaskSpec,
    CodeDiff,
    TestReport,
    ReviewVerdict,
    ReviewFeedback,
    Synthesis,
    SecurityVerdict,
    RedChallenge,
}

// ── Artifact 1: TaskSpec (Planner → Coder) ──────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub summary: String,                      // One-line task description
    pub approach: String,                     // Detailed implementation approach
    pub files_to_modify: Vec<FileChange>,     // Files to create/modify/delete
    pub acceptance_criteria: Vec<String>,      // Specific, testable criteria
    pub constraints: Vec<String>,             // Things to avoid or be careful about
    pub estimated_complexity: Complexity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub action: FileAction,
    pub description: String,                  // What changes in this file
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
    #[default]
    Low,
    Medium,
    High,
}

// ── Artifact 2: CodeDiff (Coder → Tester) ───────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeDiff {
    pub unified_diff: String,                 // Full unified diff of all changes
    pub files_changed: Vec<ChangedFile>,
    pub implementation_notes: String,         // Coder's explanation of decisions made
    pub spec_adherence: String,               // How the implementation maps to the spec
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub action: FileAction,
    pub diff: String,                         // Per-file unified diff
    pub language: Option<String>,             // Detected programming language
}

// ── Artifact 3: TestReport (Tester → Reviewer) ──────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub tests_written: Vec<TestCase>,
    pub test_results: TestResults,
    pub coverage_summary: Option<CoverageSummary>,
    pub edge_cases_found: Vec<String>,        // Edge cases the Tester identified
    pub tester_notes: String,                 // Tester's overall assessment
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub file_path: String,
    pub description: String,
    pub status: TestStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageSummary {
    pub line_coverage_percent: f64,
    pub branch_coverage_percent: Option<f64>,
    pub uncovered_files: Vec<String>,
}

// ── Artifact 4: ReviewVerdict (Reviewer → Output OR Feedback) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewVerdict {
    pub verdict: Verdict,
    pub overall_assessment: String,           // Reviewer's summary judgment
    pub quality_scores: QualityScores,
    pub issues: Vec<ReviewIssue>,             // All issues found
    pub strengths: Vec<String>,               // What was done well
    pub feedback: Option<ReviewFeedback>,     // Present if verdict is RevisionNeeded
    /// Per-challenge reconciliation against the independent Red agent's critique
    /// (#1.2). Each entry records whether the Reviewer UPHeld or REFUTED a Red
    /// challenge and why. Proves the Reviewer engaged with the adversarial
    /// critique instead of rubber-stamping the Coder. Optional; absent when the
    /// Red/Blue pass is disabled.
    pub red_reconciliation: Option<Vec<RedReconciliation>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approved,
    RevisionNeeded,
    Rejected,                                 // Unrepairable — escalate to human
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScores {
    pub correctness: u8,                      // 1-10
    pub code_quality: u8,                     // 1-10
    pub test_coverage: u8,                    // 1-10
    pub spec_adherence: u8,                   // 1-10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub file_path: Option<String>,
    pub line_range: Option<String>,           // e.g., "42-58"
    pub description: String,
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Critical,                                 // Must fix before approval
    Major,                                    // Should fix
    Minor,                                    // Nice to fix
    Nit,                                      // Style/preference
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueCategory {
    Bug,
    Security,
    Performance,
    Style,
    Logic,
    TestGap,
    SpecDeviation,
}

// ── Artifact 5: ReviewFeedback (Reviewer → Coder, on revision) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFeedback {
    pub critical_issues: Vec<ReviewIssue>,     // Only critical/major issues to fix
    pub guidance: String,                      // Reviewer's specific guidance for revision
    pub keep_unchanged: Vec<String>,           // Files/aspects that are fine — don't touch
    pub revision_round: u32,                   // Which round of revision this is
}

// ── Artifact 6: Synthesis (Synthesizer → merged CodeDiff) ─────────
//
// Produced when multiple coders run in parallel (#3). Each coder emits its own
// CodeDiff in an isolated worktree; the Synthesizer reconciles them into the
// single `merged` CodeDiff the rest of the pipeline (Tester → Reviewer) consumes.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synthesis {
    /// The reconciled change, in the same shape as a single coder's output, so
    /// downstream stages treat it identically to a `CodeDiff`.
    pub merged: CodeDiff,
    /// How conflicts between the parallel coders were resolved.
    pub reconciliation_notes: String,
    /// Number of distinct coder branches that fed into this synthesis.
    pub sources_merged: u32,
}

// ── Artifact 7: SecurityVerdict (SecurityAuditor → Output) ────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityVerdict {
    pub verdict: Verdict,                      // Approved / RevisionNeeded / Rejected
    pub overall_assessment: String,
    pub findings: Vec<SecurityFinding>,
    pub strengths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub severity: SecuritySeverity,
    pub category: SecurityCategory,
    pub file_path: Option<String>,
    pub line_range: Option<String>,            // e.g., "42-58"
    pub description: String,
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecuritySeverity {
    Critical,                                  // Exploitable now — block
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityCategory {
    Injection,
    Authentication,
    Authorization,
    Cryptography,
    SecretsExposure,
    Dependency,
    InputValidation,
    SandboxEscape,
    Other,
}

// ── Artifact 8: RedChallenge (Red → Reviewer) ────────────────────
//
// Produced by the independent "Red" agent (#1.2) before the Reviewer runs.
// The Red agent has never seen the Coder's reasoning — only the spec, the diff,
// and the test report — so it probes the change adversarially. The Reviewer is
// then forced to reconcile every challenge, which guards against the
// sycophantic convergence that would otherwise make "independent review" theater.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedChallenge {
    /// Red's overall adversarial thesis: is this change trustworthy as written?
    pub overall_red_assessment: String,
    /// Discrete adversarial points the Reviewer must each address.
    pub challenges: Vec<RedPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedPoint {
    /// Stable id (e.g. "R1") so the Reviewer can cross-reference it.
    pub id: String,
    /// How serious Red believes the issue is.
    pub severity: IssueSeverity,
    /// What kind of problem Red is asserting.
    pub category: IssueCategory,
    /// The concrete claim: what Red asserts is wrong, risky, or unproven.
    pub claim: String,
    /// How confident Red is, 1-10.
    pub confidence: u8,
    /// Optional line references / reasoning behind the claim.
    pub evidence: Option<String>,
    /// Optional concrete way to verify or disprove the claim.
    pub suggested_check: Option<String>,
}

/// The Reviewer's explicit disposition on a single Red challenge (#1.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedReconciliation {
    /// Matches a `RedPoint.id` from the Red agent's challenge.
    pub challenge_id: String,
    /// Whether the Reviewer agreed with (upheld) or disagreed with (refuted) Red.
    pub disposition: RedDisposition,
    /// Why the Reviewer took that position.
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RedDisposition {
    Upheld,
    Refuted,
}

/// Per-agent proof of *context isolation* (BUILD_PLAN 2.1, P1.2).
///
/// NIKI's moat is not that agents run in separate containers (the sequential
/// stages intentionally share one *execution* sandbox so the diff persists from
/// Coder → Tester → Reviewer) — it is that each agent is an **independent LLM
/// session** which receives only the *published output artifacts* of earlier
/// roles, never another agent's chain-of-thought or the parent conversation.
/// That is what stops the sycophantic convergence that context-sharing subagents
/// (which inherit the parent's running context) suffer from.
///
/// `IsolationRecord` makes that property inspectable: it records exactly which
/// roles' artifacts an agent saw, the sandbox backend it executed in, and that it
/// never saw another agent's reasoning. The run report renders one row per agent
/// so the claim is visible, not asserted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationRecord {
    pub role: AgentRole,
    /// The sandbox backend the agent executed in (Docker container / git worktree / cloud).
    pub backend: crate::sandbox::SandboxBackend,
    /// The roles whose *published artifacts* this agent received as context.
    /// This is the complete set of prior agents it could have "seen" — and it is
    /// artifacts only, never reasoning.
    pub context_sources: Vec<AgentRole>,
    /// Always `false` by construction: an agent is never handed another agent's
    /// intermediate reasoning, scratchpad, or the host conversation.
    pub saw_other_reasoning: bool,
}

/// Stable on-disk filename (without extension) for a role's raw artifact, as
/// written by the CLI (`artifacts/<name>.json`) and consumed by the eval harness
/// replay path. Single source of truth shared with `cli/run.rs::role_filename`.
pub fn artifact_json_name(role: AgentRole) -> &'static str {
    match role {
        AgentRole::Planner => "planner",
        AgentRole::Coder => "coder",
        AgentRole::Tester => "tester",
        AgentRole::Reviewer => "reviewer",
        AgentRole::Synthesizer => "synthesizer",
        AgentRole::SecurityAuditor => "security_auditor",
        AgentRole::Red => "red",
    }
}
