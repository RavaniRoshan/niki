package protocol

import (
	"time"

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
	EventError                EventType = "error"
	EventBootPhase            EventType = "boot_phase"
	EventQuestionPrompted     EventType = "question_prompted"
	EventSessionList          EventType = "session_list"
	EventSessionLoaded        EventType = "session_loaded"
)

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
}

type EngineEvent struct {
	Type      EventType         `json:"type"`
	Timestamp time.Time         `json:"timestamp"`
	SessionID SessionId         `json:"session_id,omitempty"`
	TurnID    TurnId            `json:"turn_id,omitempty"`
	CallID    ToolCallId        `json:"call_id,omitempty"`
	Text      string            `json:"text,omitempty"`
	ToolName  string            `json:"tool_name,omitempty"`
	Error     string            `json:"error,omitempty"`
	Duration  time.Duration     `json:"duration,omitempty"`
	Plan      []PlanStep        `json:"plan,omitempty"`
	Usage     *Usage            `json:"usage,omitempty"`
	Questions []UserQuestion    `json:"questions,omitempty"`
	Sessions  []SessionMetadata `json:"sessions,omitempty"`
	History   []string          `json:"history,omitempty"`
}

type Usage = provider.Usage

type PlanStep struct {
	Description string `json:"description"`
	Status      string `json:"status"` // pending, active, completed, failed
}
