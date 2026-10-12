package protocol

import (
	"time"

	"github.com/RavaniRoshan/niki/internal/tools"

	"github.com/RavaniRoshan/niki/internal/provider"
)

type EventType string

const (
	EventSessionStarted       EventType = "session_started"
	EventSessionReady         EventType = "session_ready"
	EventTurnStarted          EventType = "turn_started"
	EventTurnCompleted        EventType = "turn_completed"
	EventTurnCancelled        EventType = "turn_cancelled"
	EventTurnFailed           EventType = "turn_failed"
	EventAssistantTextDelta   EventType = "assistant_text_delta"
	EventAssistantMessageDone EventType = "assistant_message_done"
	EventPlanUpdated          EventType = "plan_updated"
	EventToolStarted          EventType = "tool_started"
	EventToolOutput           EventType = "tool_output"
	EventToolCompleted        EventType = "tool_completed"
	EventToolFailed           EventType = "tool_failed"
	EventPermissionRequested  EventType = "permission_requested"
	EventPermissionResolved   EventType = "permission_resolved"
	EventMcpServerStarting    EventType = "mcp_server_starting"
	EventMcpServerReady       EventType = "mcp_server_ready"
	EventMcpServerFailed      EventType = "mcp_server_failed"
	EventSkillDiscovered      EventType = "skill_discovered"
	EventConfigReloaded       EventType = "config_reloaded"
	EventContextCompacted     EventType = "context_compacted"
	EventSubagentStarted      EventType = "subagent_started"
	EventSubagentCompleted    EventType = "subagent_completed"
	EventSubagentFailed       EventType = "subagent_failed"
	EventWarning              EventType = "warning"
	EventSteer                EventType = "steer"
	EventError                EventType = "error"
	EventBootPhase            EventType = "boot_phase"
	EventQuestionPrompted     EventType = "question_prompted"
	EventSessionList          EventType = "session_list"
	EventSessionLoaded        EventType = "session_loaded"
	EventBudgetGauge          EventType = "budget_gauge"
	EventBudgetWrap           EventType = "budget_wrap"
	EventBudgetStopped        EventType = "budget_stopped"
	EventVerification         EventType = "verification"
	EventTurnNotVerified      EventType = "turn_not_verified"
	EventReasoningDelta       EventType = "reasoning_delta"
	// EventCheckpointCreated reports a real checkpoint (U3/E3): the
	// label is the checkpoint's own turn number, set by the engine
	// where the snapshot is taken. The right rail and the status
	// count render only from these events.
	EventCheckpointCreated EventType = "checkpoint_created"
	// EventModeChanged reports a real mode switch (U5): the engine
	// emits it when a mode command (or a config reload) changes the
	// permission mode or plan mode. The frontend applies it as the
	// authoritative mode state; the mode chip renders from it.
	EventModeChanged EventType = "mode_changed"
)

// VerificationInfo reports one verifier attempt (L3).
type VerificationInfo struct {
	Attempt int    `json:"attempt"`
	Result  string `json:"result"`
	Reason  string `json:"reason,omitempty"`

	// Tier is the graded trust tier code ("T0".."T3") (C3). The
	// word "verified" may render only at T2 or higher.
	Tier string `json:"tier,omitempty"`
	// Requirements is the per-requirement evidence in prompt
	// order (C3): the verdict card renders exactly this.
	Requirements []ReqStatus `json:"requirements,omitempty"`
	// NotVerified lists what failed or had no check (C3). A
	// verified verdict never has entries here.
	NotVerified []string `json:"not_verified,omitempty"`
	// Contract is the contract strip's per-requirement state
	// during the run (C3).
	Contract []ContractReq `json:"contract,omitempty"`
}

// ReqStatus is one requirement's measured state (C3).
type ReqStatus struct {
	ID       string `json:"id"`
	Text     string `json:"text"`
	Evidence string `json:"evidence,omitempty"`
	State    string `json:"state"` // pass, fail, queued
}

// ContractReq is one contract strip entry (C3).
type ContractReq struct {
	ID    string `json:"id"`
	State string `json:"state"` // pass, fail, running, queued, needs
}

// BudgetInfo carries L2 budget gauges and verdicts on budget events.
type BudgetInfo struct {
	State       string  `json:"state"`
	Hit         string  `json:"hit,omitempty"`
	Wall        float64 `json:"wall"`
	Cost        float64 `json:"cost"`
	Step        float64 `json:"step"`
	WallEnabled bool    `json:"wall_enabled"`
	CostEnabled bool    `json:"cost_enabled"`
	StepEnabled bool    `json:"step_enabled"`
	WallSeconds float64 `json:"wall_seconds"`
	CostUSD     float64 `json:"cost_usd"`
	StepsUsed   int     `json:"steps_used"`
	Verdict     string  `json:"verdict,omitempty"`
}

type SessionMetadata struct {
	ID        SessionId `json:"id"`
	Title     string    `json:"title"`
	CreatedAt time.Time `json:"created_at"`
	TurnCount int       `json:"turn_count"`
}

type UserQuestion struct {
	Header      string   `json:"header"`
	Question    string   `json:"question"`
	Options     []string `json:"options"`
	AllowCustom bool     `json:"allow_custom"`
	// Safe is the index of the option that keeps the constraint
	// (C3): the gate focuses it and marks it "(default)". -1 when
	// the source does not say, in which case the first option is
	// the default (the tool documents safe-first ordering).
	Safe int `json:"safe_option"`
}

type EngineEvent struct {
	Type         EventType         `json:"type"`
	Timestamp    time.Time         `json:"timestamp"`
	SessionID    SessionId         `json:"session_id,omitempty"`
	TurnID       TurnId            `json:"turn_id,omitempty"`
	CallID       ToolCallId        `json:"call_id,omitempty"`
	Text         string            `json:"text,omitempty"`
	ToolName     string            `json:"tool_name,omitempty"`
	Error        string            `json:"error,omitempty"`
	Duration     time.Duration     `json:"duration,omitempty"`
	Plan         []PlanStep        `json:"plan,omitempty"`
	Usage        *Usage            `json:"usage,omitempty"`
	Questions    []UserQuestion    `json:"questions,omitempty"`
	Sessions     []SessionMetadata `json:"sessions,omitempty"`
	History      []string          `json:"history,omitempty"`
	Budget       *BudgetInfo       `json:"budget,omitempty"`
	Verification *VerificationInfo `json:"verification,omitempty"`

	// Target is the tool row's target: the file path, command, or
	// pattern the tool acts on (C2). Set by the engine at
	// tool_started; the TUI never invents it.
	Target string `json:"target,omitempty"`
	// Result is the tool row's right-aligned result summary: line
	// counts, diff stats, exit codes (C2). Set at
	// tool_completed/tool_failed.
	Result string `json:"result,omitempty"`
	// Excerpt is up to two lines of failing output shown beneath
	// a failed tool row (C2). Set at tool_failed only.
	Excerpt string `json:"excerpt,omitempty"`

	// Contract is the turn's requirement contract (C3), extracted
	// from the prompt at turn start: one entry per requirement,
	// all queued. The contract strip renders only this.
	Contract []ContractReq `json:"contract,omitempty"`

	// Step is the plan step a tool call serves (U2/E1): the
	// in_progress step's label at emit time, set by the engine.
	// Empty means the engine does not attribute this call to a
	// step, and the TUI renders no step header for it — the
	// grouping is never invented in the frontend.
	Step string `json:"step,omitempty"`
	// Diff is the real unified diff of a file mutation, attached
	// by the engine at the patching tool's completion (U2/E2).
	// Empty when the tool changed no bytes.
	Diff string `json:"diff,omitempty"`

	// Mode is the permission mode after a mode switch (U5):
	// readonly | workspace_write | full_access | manual. Set on
	// mode_changed events only.
	Mode string `json:"mode,omitempty"`
	// PlanActive reports plan mode after a mode switch (U5);
	// together with Mode it names the workbench mode
	// (plan | act | yolo). Set on mode_changed events only.
	PlanActive bool `json:"plan_active,omitempty"`
}

type Usage = provider.Usage

// EventSchemaVersion versions sessions and JSON event output
// (U2). Breaking changes bump this number; readers must tolerate
// unknown fields (forward-compatible decode).
const EventSchemaVersion = 1

// Redacted returns a copy with credential-like text scrubbed from
// the free-text carriers (X3). Callers persist or render the copy,
// never the original.
func (e EngineEvent) Redacted() EngineEvent {
	e.Text = tools.Redact(e.Text)
	e.Error = tools.Redact(e.Error)
	return e
}

type PlanStep struct {
	Description string `json:"description"`
	Status      string `json:"status"` // pending, active, completed, failed, cancelled
}
