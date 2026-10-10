package provider

import (
	"context"
	"encoding/json"
)

// DeltaKind classifies a streamed model delta.
type DeltaKind string

const (
	DeltaText       DeltaKind = "text"
	DeltaToolCall   DeltaKind = "tool_call"
	DeltaToolResult DeltaKind = "tool_result"
	DeltaUsage      DeltaKind = "usage"
	DeltaDone       DeltaKind = "done"
)

type Delta struct {
	Kind     DeltaKind
	Text     string
	ToolName string
	ToolArgs string
	CallID   string
	Usage    *Usage
}

type Usage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
	TotalTokens      int `json:"total_tokens,omitempty"`
}

type Message struct {
	Role         string `json:"role"` // system, user, assistant, tool
	Content      string `json:"content"`
	Name         string `json:"name,omitempty"`
	CacheControl string `json:"cache_control,omitempty"`
	// ToolCalls are the tool requests an assistant message made (request-side
	// tool loop). Persisted in context so follow-up requests can rebuild the
	// provider-native conversation shape.
	ToolCalls []ToolCall `json:"tool_calls,omitempty"`
	// ToolCallID links a tool-result message to the assistant tool call it
	// answers (required by OpenAI/Anthropic tool protocols).
	ToolCallID string `json:"tool_call_id,omitempty"`
}

// ToolCall is one tool request from the model.
type ToolCall struct {
	ID   string `json:"id,omitempty"`
	Tool string `json:"tool"`
	Args string `json:"args"`
}

// ToolSpec is a tool definition sent to the model in the request.
type ToolSpec struct {
	Name        string
	Description string
	Parameters  json.RawMessage // JSON Schema object
}

// Streamer emits model deltas.
type Streamer interface {
	Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error)
}

// ToolStreamer is the request-side tool loop: providers that can receive tool
// schemas and return tool-call deltas implement this. The engine prefers it
// over Stream whenever available.
type ToolStreamer interface {
	ModelProvider
	StreamWithTools(ctx context.Context, messages []Message, tools []ToolSpec) (<-chan Delta, <-chan error)
}

// ModelProvider is the runtime contract for providers.
type ModelProvider interface {
	Name() string
	Streamer
}
