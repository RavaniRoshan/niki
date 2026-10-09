package provider

import (
	"context"
	"sort"
	"strings"
	"time"
)

// MockProvider is a deterministic provider for tests and offline use.
type MockProvider struct {
	ChunkDelay time.Duration
	// Script maps a prompt substring to a scripted response.
	Scripts map[string]string
	// ToolScripts maps a prompt substring to tool calls the mock
	// performs first (proving the tool loop without a model).
	ToolScripts map[string][]ToolCall
}

// ToolCall is one scripted model tool request.
type ToolCall struct {
	Tool string
	Args string
}

func NewMockProvider() *MockProvider {
	return &MockProvider{
		ChunkDelay: time.Millisecond,
		Scripts: map[string]string{
			"read": "I will read the file.",
			"run":  "I will run the command.",
		},
	}
}

func (m *MockProvider) Name() string { return "mock" }

// sortedKeys makes script matching deterministic (longest first).
func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool { return len(keys[i]) > len(keys[j]) })
	return keys
}

func (m *MockProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	deltas := make(chan Delta, 16)
	errs := make(chan error, 1)

	user := ""
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == "user" {
			user = messages[i].Content
			break
		}
	}

	response := "Acknowledged: " + user
	for _, k := range sortedKeys(m.Scripts) {
		if strings.Contains(strings.ToLower(user), k) {
			response = m.Scripts[k]
			break
		}
	}
	var calls []ToolCall
	hasToolResults := false
	for _, msg := range messages {
		if msg.Role == "tool" {
			hasToolResults = true
			break
		}
	}
	if !hasToolResults {
		for _, k := range sortedKeys(m.ToolScripts) {
			if strings.Contains(strings.ToLower(user), k) {
				calls = m.ToolScripts[k]
				break
			}
		}
	}

	go func() {
		defer close(deltas)
		defer close(errs)
		for _, c := range calls {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case <-time.After(m.ChunkDelay):
			}
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case deltas <- Delta{Kind: DeltaToolCall, ToolName: c.Tool, ToolArgs: c.Args}:
			}
		}
		for _, word := range strings.Split(response, " ") {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case <-time.After(m.ChunkDelay):
			}
			d := Delta{Kind: DeltaText, Text: word + " "}
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case deltas <- d:
			}
		}
		deltas <- Delta{Kind: DeltaUsage, Usage: &Usage{PromptTokens: len(user) / 4, CompletionTokens: len(response) / 4}}
		deltas <- Delta{Kind: DeltaDone}
	}()

	return deltas, errs
}
