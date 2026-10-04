//! Every message that may cross the seam.
//!
//! Two enums are closed by construction: [`ClientRequest`] and [`ServerNotification`]. A method
//! name is declared exactly once, as a `#[serde(rename)]` on a variant, and [`method`] returns it.
//! `tests/protocol_contract.rs` asserts that every declared variant is reachable, that its wire
//! name is the one the docs name, and that nothing else parses.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Why a tool or a stage row exists at all. Drives glyph and colour, never invented locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Idle,
    Thinking,
    Streaming,
    ToolRunning,
    AwaitingApproval,
    Error,
    Interrupted,
    Done,
}

/// The pipeline role that produced an event. Mirrors `AgentRole`; nothing is invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StageRole {
    Planner,
    Coder,
    Tester,
    Reviewer,
    Synthesizer,
    SecurityAuditor,
    Red,
    Critic,
}

impl StageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            StageRole::Planner => "planner",
            StageRole::Coder => "coder",
            StageRole::Tester => "tester",
            StageRole::Reviewer => "reviewer",
            StageRole::Synthesizer => "synthesizer",
            StageRole::SecurityAuditor => "security_auditor",
            StageRole::Red => "red",
            StageRole::Critic => "critic",
        }
    }
}

/// Whether a verdict came from a stage that could see the code or from the stage that wrote it.
/// The footer and the stage row both show this, because it changes how much a verdict is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// A stage independent of the one that produced the change.
    Independent,
    /// The producing stage checking its own work.
    SelfVerification,
}

/// Severity carried by a failure. The shell maps this to the error colour and never to anything
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Shell -> engine
// ---------------------------------------------------------------------------

/// A request from the shell to the engine. Adjacently tagged: `{"method":..,"params":..}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "params")]
#[ts(tag = "method", content = "params")]
pub enum ClientRequest {
    #[serde(rename = "initialize")]
    Initialize(InitializeParams),
    #[serde(rename = "shutdown")]
    Shutdown(ShutdownParams),
    #[serde(rename = "session.load")]
    SessionLoad(SessionLoadParams),
    #[serde(rename = "turn.start")]
    TurnStart(TurnStartParams),
    #[serde(rename = "approval.reply")]
    ApprovalReply(ApprovalReplyParams),
}

impl ClientRequest {
    /// The wire name of this method. One source of truth for the popup, the help text and the
    /// tests; nothing restates it.
    pub fn method(&self) -> &'static str {
        match self {
            ClientRequest::Initialize(_) => "initialize",
            ClientRequest::Shutdown(_) => "shutdown",
            ClientRequest::SessionLoad(_) => "session.load",
            ClientRequest::TurnStart(_) => "turn.start",
            ClientRequest::ApprovalReply(_) => "approval.reply",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InitializeParams {
    /// The protocol the shell speaks. The engine refuses a mismatch rather than guessing.
    pub protocol_version: u32,
    pub client: ClientInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
    /// How many columns and rows the shell has. The engine uses it to size diffs and reports.
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ShutdownParams {
    /// True when the user asked to quit, false when the shell is being replaced.
    pub user_initiated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SessionLoadParams {
    /// `None` starts a fresh session; `Some` resumes a checkpoint.
    pub session_id: Option<String>,
    pub project_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TurnStartParams {
    pub prompt: String,
    /// Permission posture for this turn. The engine never assumes one the shell did not send.
    pub permission_mode: PermissionMode,
}

/// The permission posture, deny-first. `Manual` is the default and the only mode in which the
/// shell focuses the safest option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Ask before anything that is not read-only. The default, and the only mode in which the
    /// shell focuses the safest option.
    #[default]
    Manual,
    /// Ask only for the risky classes.
    Auto,
    /// Never ask; deny what policy does not allow.
    DontAsk,
    /// Ask nothing and allow everything. Never a default; requires explicit confirmation.
    Bypass,
}

/// What the user decided about an `approval.request`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allow,
    Deny,
    /// Denied, with a reason the user typed. Tab-reject with a reason lands here.
    DenyWithReason,
    /// Allow this and every equivalent request for the rest of the session.
    AllowAlways,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApprovalReplyParams {
    /// The `id` from the `approval.request` this answers.
    pub id: String,
    pub decision: ApprovalDecision,
    /// The user's words when the decision carries one.
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Engine -> shell: results
// ---------------------------------------------------------------------------

/// The `result` member of a response. Untagged because the request id already says which method
/// it answers; the shell deserialises with the type it asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClientResult {
    Initialize(InitializeResult),
    Shutdown(ShutdownResult),
    SessionLoad(SessionLoadResult),
    TurnStart(TurnStartResult),
    ApprovalReply(ApprovalReplyResult),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InitializeResult {
    pub protocol_version: u32,
    pub engine_version: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct Capabilities {
    pub streaming: bool,
    pub approvals: bool,
    pub sessions: bool,
    pub diffs: bool,
    pub context_usage: bool,
    pub cost: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ShutdownResult {
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SessionLoadResult {
    pub session_id: String,
    /// What the session already knows. Empty on a fresh session; the shell says so plainly.
    pub resumed_messages: u32,
    pub project_path: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TurnStartResult {
    /// The engine accepted the turn and started it. Not a promise that it will succeed.
    pub turn_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApprovalReplyResult {
    pub id: String,
    pub decision: ApprovalDecision,
}

// ---------------------------------------------------------------------------
// Engine -> shell: notifications
// ---------------------------------------------------------------------------

/// A message the engine pushes without being asked. Adjacently tagged, like every other payload:
/// `{"method":..,"params":..}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "method", content = "params")]
#[ts(tag = "method", content = "params")]
pub enum ServerNotification {
    #[serde(rename = "session.ready")]
    SessionReady(SessionReadyParams),
    #[serde(rename = "turn.started")]
    TurnStarted(TurnStartedParams),
    #[serde(rename = "turn.delta")]
    TurnDelta(TurnDeltaParams),
    #[serde(rename = "turn.end")]
    TurnEnd(TurnEndParams),
    #[serde(rename = "stage.start")]
    StageStart(StageStartParams),
    #[serde(rename = "stage.token")]
    StageToken(StageTokenParams),
    #[serde(rename = "stage.done")]
    StageDone(StageDoneParams),
    #[serde(rename = "stage.failed")]
    StageFailed(StageFailedParams),
    #[serde(rename = "tool.call")]
    ToolCall(ToolCallParams),
    #[serde(rename = "tool.progress")]
    ToolProgress(ToolProgressParams),
    #[serde(rename = "tool.result")]
    ToolResult(ToolResultParams),
    #[serde(rename = "tool.diff")]
    ToolDiff(ToolDiffParams),
    #[serde(rename = "approval.request")]
    ApprovalRequest(ApprovalRequestParams),
    #[serde(rename = "plan.update")]
    PlanUpdate(PlanUpdateParams),
    #[serde(rename = "notice")]
    Notice(NoticeParams),
    #[serde(rename = "diff.ready")]
    DiffReady(RefParams),
    #[serde(rename = "verdict.ready")]
    VerdictReady(VerdictReadyParams),
    #[serde(rename = "branch.created")]
    BranchCreated(BranchCreatedParams),
    #[serde(rename = "context.usage")]
    ContextUsage(ContextUsageParams),
    #[serde(rename = "cost.update")]
    CostUpdate(CostUpdateParams),
    #[serde(rename = "final")]
    Final(FinalParams),
}

impl ServerNotification {
    /// The wire name of this notification. One source of truth for the shell's dispatch table.
    pub fn method(&self) -> &'static str {
        match self {
            ServerNotification::SessionReady(_) => "session.ready",
            ServerNotification::TurnStarted(_) => "turn.started",
            ServerNotification::TurnDelta(_) => "turn.delta",
            ServerNotification::TurnEnd(_) => "turn.end",
            ServerNotification::StageStart(_) => "stage.start",
            ServerNotification::StageToken(_) => "stage.token",
            ServerNotification::StageDone(_) => "stage.done",
            ServerNotification::StageFailed(_) => "stage.failed",
            ServerNotification::ToolCall(_) => "tool.call",
            ServerNotification::ToolProgress(_) => "tool.progress",
            ServerNotification::ToolResult(_) => "tool.result",
            ServerNotification::ToolDiff(_) => "tool.diff",
            ServerNotification::ApprovalRequest(_) => "approval.request",
            ServerNotification::PlanUpdate(_) => "plan.update",
            ServerNotification::Notice(_) => "notice",
            ServerNotification::DiffReady(_) => "diff.ready",
            ServerNotification::VerdictReady(_) => "verdict.ready",
            ServerNotification::BranchCreated(_) => "branch.created",
            ServerNotification::ContextUsage(_) => "context.usage",
            ServerNotification::CostUpdate(_) => "cost.update",
            ServerNotification::Final(_) => "final",
        }
    }

    /// Every declared notification, built once. Tests use it to prove the message set is closed
    /// and to regenerate the TypeScript dispatch table without hand-writing a second list.
    pub fn all() -> Vec<ServerNotification> {
        vec![
            ServerNotification::SessionReady(SessionReadyParams {
                session_id: "s".into(),
                project_path: "/tmp".into(),
                model: "model".into(),
                permission_mode: PermissionMode::Manual,
                branch: Some("main".into()),
                ahead: None,
                behind: None,
                resumed_messages: 0,
            }),
            ServerNotification::TurnStarted(TurnStartedParams {
                turn_id: "t".into(),
                prompt: "p".into(),
            }),
            ServerNotification::TurnDelta(TurnDeltaParams {
                turn_id: "t".into(),
                text: "x".into(),
            }),
            ServerNotification::TurnEnd(TurnEndParams {
                turn_id: "t".into(),
                summary: "s".into(),
                duration_ms: 0,
                tool_calls: 0,
                files_changed: 0,
            }),
            ServerNotification::StageStart(StageStartParams {
                stage_id: "s".into(),
                role: StageRole::Planner,
                attempt: 1,
            }),
            ServerNotification::StageToken(StageTokenParams {
                stage_id: "s".into(),
                role: StageRole::Planner,
                text: "x".into(),
            }),
            ServerNotification::StageDone(StageDoneParams {
                stage_id: "s".into(),
                role: StageRole::Planner,
                summary: "s".into(),
                tokens_in: 0,
                tokens_out: 0,
                cost_usd: 0.0,
                latency_ms: 0,
                retry_count: 0,
                artifact_ref: Some("a".into()),
                provenance: Provenance::Independent,
            }),
            ServerNotification::StageFailed(StageFailedParams {
                stage_id: "s".into(),
                role: StageRole::Coder,
                error: "e".into(),
                severity: Severity::Error,
                recovery: None,
            }),
            ServerNotification::ToolCall(ToolCallParams {
                tool_id: "tool".into(),
                name: "read".into(),
                args: "path".into(),
            }),
            ServerNotification::ToolProgress(ToolProgressParams {
                tool_id: "tool".into(),
                note: "n".into(),
            }),
            ServerNotification::ToolResult(ToolResultParams {
                tool_id: "tool".into(),
                ok: true,
                summary: "read 46 lines".into(),
                full_ref: None,
                duration_ms: 0,
            }),
            ServerNotification::ToolDiff(ToolDiffParams {
                tool_id: "tool".into(),
                path: "src/lib.rs".into(),
                hunks: Vec::new(),
            }),
            ServerNotification::ApprovalRequest(ApprovalRequestParams {
                id: "a".into(),
                tool: "bash".into(),
                command: "npm test".into(),
                options: vec![ApprovalOption {
                    id: "deny".into(),
                    label: "Deny".into(),
                }],
                // The engine says which option is safest, so the shell cannot get this wrong.
                safest_option_id: "deny".into(),
            }),
            ServerNotification::PlanUpdate(PlanUpdateParams { items: Vec::new() }),
            ServerNotification::Notice(NoticeParams {
                text: "n".into(),
                level: Severity::Info,
            }),
            ServerNotification::DiffReady(RefParams { ref_: "a".into() }),
            ServerNotification::VerdictReady(VerdictReadyParams {
                ref_: "a".into(),
                verdict: "approved".into(),
                provenance: Provenance::Independent,
            }),
            ServerNotification::BranchCreated(BranchCreatedParams {
                name: "niki/1".into(),
            }),
            ServerNotification::ContextUsage(ContextUsageParams { used: 0, limit: 0 }),
            ServerNotification::CostUpdate(CostUpdateParams { usd: 0.0 }),
            ServerNotification::Final(FinalParams {
                verdict: Some("approved".into()),
                error: None,
            }),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SessionReadyParams {
    pub session_id: String,
    pub project_path: String,
    pub model: String,
    pub permission_mode: PermissionMode,
    pub branch: Option<String>,
    /// Ahead/behind counts. Absent when git has not reported them; the footer then shows no arrow
    /// rather than a fabricated one.
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub resumed_messages: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TurnStartedParams {
    pub turn_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TurnDeltaParams {
    pub turn_id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TurnEndParams {
    pub turn_id: String,
    pub summary: String,
    #[ts(type = "number")] // u64 counts a terminal never reaches 2^53
    pub duration_ms: u64,
    pub tool_calls: u32,
    pub files_changed: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StageStartParams {
    pub stage_id: String,
    pub role: StageRole,
    /// 1 on the first pass. A revision loop re-runs a stage with 2, 3, ... so the UI can show a
    /// retry without inventing one.
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StageTokenParams {
    pub stage_id: String,
    pub role: StageRole,
    /// A provider-supplied summary only. Raw private chain-of-thought is never sent.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StageDoneParams {
    pub stage_id: String,
    pub role: StageRole,
    pub summary: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f64,
    #[ts(type = "number")] // u64 counts a terminal never reaches 2^53
    pub latency_ms: u64,
    pub retry_count: u32,
    pub artifact_ref: Option<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StageFailedParams {
    pub stage_id: String,
    pub role: StageRole,
    pub error: String,
    pub severity: Severity,
    /// What the user can do about it. `None` when there is nothing useful to offer, and the shell
    /// then renders no action rather than a fake one.
    pub recovery: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ToolCallParams {
    pub tool_id: String,
    pub name: String,
    /// Pre-rendered, single-line arguments. The shell does not parse tool arguments.
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ToolProgressParams {
    pub tool_id: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ToolResultParams {
    pub tool_id: String,
    pub ok: bool,
    /// The one-line result shown under the tool row.
    pub summary: String,
    /// Where the full output lives, when there is more than the summary.
    pub full_ref: Option<String>,
    #[ts(type = "number")] // u64 counts a terminal never reaches 2^53
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ToolDiffParams {
    pub tool_id: String,
    pub path: String,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApprovalRequestParams {
    pub id: String,
    pub tool: String,
    pub command: String,
    pub options: Vec<ApprovalOption>,
    /// Which option is safest. The shell focuses this one, never the first and never Approve.
    pub safest_option_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApprovalOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PlanUpdateParams {
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PlanItem {
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoticeParams {
    pub text: String,
    pub level: Severity,
}

/// A pointer to something the engine produced on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RefParams {
    /// `ref` is a Rust keyword, so the field is `ref_` and serialises as `ref`.
    #[serde(rename = "ref")]
    pub ref_: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct VerdictReadyParams {
    #[serde(rename = "ref")]
    pub ref_: String,
    pub verdict: String,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BranchCreatedParams {
    pub name: String,
}

/// Sent only when the engine has real numbers. An absent message means the footer shows no meter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ContextUsageParams {
    #[ts(type = "number")] // u64 counts a terminal never reaches 2^53
    pub used: u64,
    #[ts(type = "number")] // u64 counts a terminal never reaches 2^53
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CostUpdateParams {
    pub usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct FinalParams {
    pub verdict: Option<String>,
    /// Present only when the run failed. A run that succeeded never carries an empty error.
    pub error: Option<String>,
}
