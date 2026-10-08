package provider

import (
	"context"
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
	Role    string `json:"role"` // system, user, assistant, tool
	Content string `json:"content"`
	Name    string `json:"name,omitempty"`
}

// Streamer emits model deltas.
type Streamer interface {
	Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error)
}

// ModelProvider is the runtime contract for providers.
type ModelProvider interface {
	Name() string
	Streamer
}
