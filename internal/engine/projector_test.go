package engine

import (
	"testing"

	"github.com/RavaniRoshan/niki/internal/provider"
)

func TestProjectContextHealsDanglingToolCalls(t *testing.T) {
	messages := []provider.Message{
		{Role: "system", Content: "System prompt"},
		{Role: "user", Content: "Run tests"},
		{Role: "assistant", Content: "Running tests now [ToolCall: bash]"},
		{Role: "user", Content: "Actually stop and check git status"},
	}

	projected := ProjectContext(messages)
	if len(projected) != 5 {
		t.Fatalf("expected 5 messages (with synthesized tool result), got %d: %+v", len(projected), projected)
	}

	if projected[3].Role != "tool" {
		t.Fatalf("expected synthesized tool message at index 3, got: %+v", projected[3])
	}
}

func TestProjectContextMergesConsecutiveAssistants(t *testing.T) {
	messages := []provider.Message{
		{Role: "system", Content: "System prompt"},
		{Role: "user", Content: "Hi"},
		{Role: "assistant", Content: "Hello!"},
		{Role: "assistant", Content: "How can I help you today?"},
	}

	projected := ProjectContext(messages)
	if len(projected) != 3 {
		t.Fatalf("expected 3 messages after merging assistants, got %d: %+v", len(projected), projected)
	}

	expected := "Hello!\n\nHow can I help you today?"
	if projected[2].Content != expected {
		t.Fatalf("expected merged content %q, got %q", expected, projected[2].Content)
	}
}

func TestProjectContextInjectsPromptCacheHints(t *testing.T) {
	messages := []provider.Message{
		{Role: "system", Content: "System prompt"},
		{Role: "user", Content: "First prompt"},
		{Role: "assistant", Content: "First answer"},
		{Role: "tool", Content: "Tool output"},
		{Role: "user", Content: "Second prompt"},
	}

	projected := ProjectContext(messages)
	if projected[0].CacheControl != "ephemeral" {
		t.Errorf("system prompt must have CacheControl=ephemeral")
	}
	if projected[3].CacheControl != "ephemeral" {
		t.Errorf("last tool message must have CacheControl=ephemeral")
	}
	if projected[4].CacheControl != "ephemeral" {
		t.Errorf("latest user message must have CacheControl=ephemeral")
	}
}
