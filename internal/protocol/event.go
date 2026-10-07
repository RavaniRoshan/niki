package protocol

import (
	"time"
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
	EventContextCompacted     EventType = "context_compacted"
	EventWarning              EventType = "warning"
	EventError                EventType = "error"
	EventBootPhase            EventType = "boot_phase"
)

type EngineEvent struct {
	Type      EventType     `json:"type"`
	Timestamp time.Time     `json:"timestamp"`
	SessionID SessionId     `json:"session_id,omitempty"`
	TurnID    TurnId        `json:"turn_id,omitempty"`
	CallID    ToolCallId    `json:"call_id,omitempty"`
	Text      string        `json:"text,omitempty"`
	ToolName  string        `json:"tool_name,omitempty"`
	Error     string        `json:"error,omitempty"`
	Duration  time.Duration `json:"duration,omitempty"`
	Plan      []PlanStep    `json:"plan,omitempty"`
}

type PlanStep struct {
	Description string `json:"description"`
	Status      string `json:"status"` // pending, active, completed, failed
}
