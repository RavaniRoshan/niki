package provider

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// AnthropicProvider talks to Anthropic /messages streaming endpoint.
type AnthropicProvider struct {
	BaseURL string
	APIKey  string
	Model   string
	Client  *http.Client

	// jitter scales retry backoff (L6); nil means rand.Float64.
	jitter func() float64
}

func NewAnthropicProvider(baseURL, apiKey, model string) *AnthropicProvider {
	if baseURL == "" {
		baseURL = "https://api.anthropic.com/v1"
	}
	return &AnthropicProvider{
		BaseURL: baseURL,
		APIKey:  apiKey,
		Model:   model,
		Client:  &http.Client{Timeout: 120 * time.Second},
	}
}

func (a *AnthropicProvider) Name() string { return "anthropic" }

type anthropicBlock struct {
	Type      string          `json:"type"`
	Text      string          `json:"text,omitempty"`
	ID        string          `json:"id,omitempty"`
	Name      string          `json:"name,omitempty"`
	Input     json.RawMessage `json:"input,omitempty"`
	ToolUseID string          `json:"tool_use_id,omitempty"`
	Content   string          `json:"content,omitempty"`
}

type anthropicMessage struct {
	Role    string           `json:"role"`
	Content []anthropicBlock `json:"content"`
}

type anthropicTool struct {
	Name        string          `json:"name"`
	Description string          `json:"description,omitempty"`
	InputSchema json.RawMessage `json:"input_schema"`
}

type anthropicRequest struct {
	Model     string             `json:"model"`
	Messages  []anthropicMessage `json:"messages"`
	System    string             `json:"system,omitempty"`
	MaxTokens int                `json:"max_tokens"`
	Stream    bool               `json:"stream"`
	Tools     []anthropicTool    `json:"tools,omitempty"`
}

type anthropicDelta struct {
	Type  string `json:"type"`
	Index int    `json:"index"`
	Delta struct {
		Type        string `json:"type"`
		Text        string `json:"text"`
		PartialJSON string `json:"partial_json"`
	} `json:"delta"`
	ContentBlock *struct {
		Type string `json:"type"`
		ID   string `json:"id"`
		Name string `json:"name"`
	} `json:"content_block"`
	Usage *struct {
		InputTokens  int `json:"input_tokens"`
		OutputTokens int `json:"output_tokens"`
	} `json:"usage"`
}

// mapAnthropicConversation converts generic messages into Anthropic's
// content-block conversation: system prompts move to the top-level system
// field, assistant tool calls become tool_use blocks, tool results become
// tool_result blocks inside a following user message.
func mapAnthropicConversation(messages []Message) (string, []anthropicMessage) {
	var system []string
	var conv []anthropicMessage
	var pendingTool []anthropicBlock

	flushTool := func() {
		if len(pendingTool) == 0 {
			return
		}
		if n := len(conv); n > 0 && conv[n-1].Role == "user" {
			conv[n-1].Content = append(conv[n-1].Content, pendingTool...)
		} else {
			conv = append(conv, anthropicMessage{Role: "user", Content: pendingTool})
		}
		pendingTool = nil
	}
	appendBlocks := func(role string, blocks []anthropicBlock) {
		if n := len(conv); n > 0 && conv[n-1].Role == role {
			conv[n-1].Content = append(conv[n-1].Content, blocks...)
			return
		}
		conv = append(conv, anthropicMessage{Role: role, Content: blocks})
	}

	for _, m := range messages {
		switch m.Role {
		case "system":
			system = append(system, m.Content)
		case "assistant":
			flushTool()
			var blocks []anthropicBlock
			if m.Content != "" {
				blocks = append(blocks, anthropicBlock{Type: "text", Text: m.Content})
			}
			for _, tc := range m.ToolCalls {
				id := tc.ID
				if id == "" {
					id = "call_" + tc.Tool
				}
				blocks = append(blocks, anthropicBlock{
					Type:  "tool_use",
					ID:    id,
					Name:  tc.Tool,
					Input: sanitizeToolArgs(tc.Args),
				})
			}
			if len(blocks) > 0 {
				appendBlocks("assistant", blocks)
			}
		case "tool":
			id := m.ToolCallID
			if id == "" {
				id = "call_" + m.Name
			}
			pendingTool = append(pendingTool, anthropicBlock{
				Type:      "tool_result",
				ToolUseID: id,
				Content:   m.Content,
			})
		default: // user
			flushTool()
			if m.Content != "" {
				appendBlocks("user", []anthropicBlock{{Type: "text", Text: m.Content}})
			}
		}
	}
	flushTool()
	return strings.Join(system, "\n\n"), conv
}

// sanitizeToolArgs guarantees valid JSON for a tool_use input object. Malformed
// arguments (small-model failures) survive round-trips as a wrapped raw string
// instead of breaking request marshaling; the validator reports them to the model.
func sanitizeToolArgs(raw string) json.RawMessage {
	if raw == "" {
		return json.RawMessage(`{}`)
	}
	if json.Valid([]byte(raw)) {
		return json.RawMessage(raw)
	}
	b, _ := json.Marshal(raw)
	return json.RawMessage(`{"_raw":` + string(b) + `}`)
}

func (a *AnthropicProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	return a.StreamWithTools(ctx, messages, nil)
}

func (a *AnthropicProvider) StreamWithTools(ctx context.Context, messages []Message, tools []ToolSpec) (<-chan Delta, <-chan error) {
	deltas := make(chan Delta, 64)
	errs := make(chan error, 1)

	// toolUse tracks the in-flight tool_use block across stream events.
	var toolUse *struct {
		id   string
		name string
		args string
	}

	go func() {
		defer close(deltas)
		defer close(errs)

		system, conv := mapAnthropicConversation(messages)

		var wireTools []anthropicTool
		for _, ts := range tools {
			wireTools = append(wireTools, anthropicTool{
				Name:        ts.Name,
				Description: ts.Description,
				InputSchema: ts.Parameters,
			})
		}

		body, _ := json.Marshal(anthropicRequest{
			Model:     a.Model,
			Messages:  conv,
			System:    system,
			MaxTokens: 4096,
			Stream:    true,
			Tools:     wireTools,
		})

		url := strings.TrimSuffix(a.BaseURL, "/") + "/messages"
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
		if err != nil {
			errs <- err
			return
		}
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("x-api-key", a.APIKey)
		req.Header.Set("anthropic-version", "2023-06-01")

		resp, err := a.doWithBackoff(ctx, req)
		if err != nil {
			errs <- err
			return
		}
		defer func() { _ = resp.Body.Close() }()

		if resp.StatusCode >= 400 {
			b, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
			errs <- fmt.Errorf("provider error %d: %s", resp.StatusCode, string(b))
			return
		}

		scanner := NewSSEScanner(resp.Body)
		sawStop := false

		for {
			ev, err := scanner.Next()
			if err != nil {
				if err == io.EOF {
					if !sawStop {
						errs <- fmt.Errorf("stream truncated: connection closed before completion")
						return
					}
					break
				}
				errs <- fmt.Errorf("stream truncated: %w", err)
				return
			}

			data := strings.TrimSpace(ev.Data)
			if data == "[DONE]" || ev.Event == "message_stop" {
				sawStop = true
			}
			if data == "[DONE]" {
				break
			}

			var deltaEv anthropicDelta
			if err := json.Unmarshal([]byte(data), &deltaEv); err == nil {
				switch {
				case deltaEv.Type == "content_block_start" && deltaEv.ContentBlock != nil && deltaEv.ContentBlock.Type == "tool_use":
					toolUse = &struct {
						id   string
						name string
						args string
					}{id: deltaEv.ContentBlock.ID, name: deltaEv.ContentBlock.Name}
				case deltaEv.Type == "content_block_delta" && deltaEv.Delta.Type == "input_json_delta" && toolUse != nil:
					toolUse.args += deltaEv.Delta.PartialJSON
				case deltaEv.Type == "content_block_stop" && toolUse != nil:
					args := toolUse.args
					if args == "" {
						args = "{}"
					}
					call := Delta{Kind: DeltaToolCall, ToolName: toolUse.name, ToolArgs: args, CallID: toolUse.id}
					toolUse = nil
					select {
					case deltas <- call:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
					}
				default:
					if deltaEv.Delta.Text != "" {
						select {
						case deltas <- Delta{Kind: DeltaText, Text: deltaEv.Delta.Text}:
						case <-ctx.Done():
							errs <- ctx.Err()
							return
						}
					}
				}
				if deltaEv.Usage != nil {
					total := deltaEv.Usage.InputTokens + deltaEv.Usage.OutputTokens
					select {
					case deltas <- Delta{Kind: DeltaUsage, Usage: &Usage{
						PromptTokens:     deltaEv.Usage.InputTokens,
						CompletionTokens: deltaEv.Usage.OutputTokens,
						TotalTokens:      total,
					}}:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
					}
				}
			}

			if sawStop {
				break
			}
		}
		select {
		case deltas <- Delta{Kind: DeltaDone}:
		case <-ctx.Done():
		}
	}()

	return deltas, errs
}

func (a *AnthropicProvider) doWithBackoff(ctx context.Context, req *http.Request) (*http.Response, error) {
	for attempt := 0; attempt < 3; attempt++ {
		if req.GetBody != nil {
			body, err := req.GetBody()
			if err == nil {
				req.Body = body
			}
		}

		resp, err := a.Client.Do(req)
		if err == nil && resp.StatusCode < 500 && resp.StatusCode != 429 {
			return resp, nil
		}
		if resp != nil {
			_ = resp.Body.Close()
		}
		if attempt == 2 {
			if err != nil {
				return nil, fmt.Errorf("provider unreachable after retries (exhausted retries): %v. Check base_url, API key, and network connectivity (see `nikicode doctor`)", err)
			}
			return nil, fmt.Errorf("exhausted retries (last status %d). Check the provider status page and your quota/key, then retry", resp.StatusCode)
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(JitteredBackoff(10*time.Millisecond, attempt, a.jitter)):
		}
	}
	return nil, fmt.Errorf("exhausted retries")
}
