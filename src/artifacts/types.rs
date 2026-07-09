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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Coder,
    Tester,
    Reviewer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    TaskSpec,
    CodeDiff,
    TestReport,
    ReviewVerdict,
    ReviewFeedback,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
