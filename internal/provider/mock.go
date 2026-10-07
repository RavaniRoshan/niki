package provider

import (
	"context"
	"strings"
	"time"
)

// MockProvider is a deterministic provider for tests and offline use.
type MockProvider struct {
	ChunkDelay time.Duration
	// Script maps a prompt substring to a scripted response.
	Scripts map[string]string
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
	for k, v := range m.Scripts {
		if strings.Contains(strings.ToLower(user), k) {
			response = v
			break
		}
	}

	go func() {
		defer close(deltas)
		defer close(errs)
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
