package provider

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"sort"
	"strings"
	"time"
)

// OpenAIProvider talks to any OpenAI-compatible /chat/completions endpoint.
type OpenAIProvider struct {
	BaseURL string
	APIKey  string
	Model   string
	Client  *http.Client
	// StartHint, when set (local model paths), replaces connection-level
	// and model-missing errors with the exact commands to start the model
	// (I5). Empty for cloud providers.
	StartHint string
	name      string

	// jitter scales retry backoff (L6); nil means rand.Float64.
	jitter func() float64
}

func NewOpenAIProvider(baseURL, apiKey, model string) *OpenAIProvider {
	if baseURL == "" {
		baseURL = "https://api.openai.com/v1"
	}
	return &OpenAIProvider{BaseURL: baseURL, APIKey: apiKey, Model: model, Client: &http.Client{Timeout: 120 * time.Second}}
}

func (o *OpenAIProvider) Name() string {
	if o.name != "" {
		return o.name
	}
	return "openai"
}

type chatRequest struct {
	Model    string          `json:"model"`
	Messages []openaiMessage `json:"messages"`
	Stream   bool            `json:"stream"`
	Usage    *usageOpt       `json:"stream_options,omitempty"`
	Tools    []openaiTool    `json:"tools,omitempty"`
}

type usageOpt struct {
	IncludeUsage bool `json:"include_usage"`
}

// openaiMessage is the wire shape for chat-completions messages, including
// the tool-loop fields (tool_calls / tool_call_id) that the generic Message
// cannot express directly.
type openaiMessage struct {
	Role         string           `json:"role"`
	Content      string           `json:"content"`
	Name         string           `json:"name,omitempty"`
	ToolCallID   string           `json:"tool_call_id,omitempty"`
	ToolCalls    []openaiToolCall `json:"tool_calls,omitempty"`
	CacheControl string           `json:"cache_control,omitempty"`
}

type openaiToolCall struct {
	Type     string `json:"type"` // "function"
	ID       string `json:"id"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

type openaiFunction struct {
	Name        string          `json:"name"`
	Description string          `json:"description,omitempty"`
	Parameters  json.RawMessage `json:"parameters,omitempty"`
}

type openaiTool struct {
	Type     string         `json:"type"`
	Function openaiFunction `json:"function"`
}

// toOpenAIMessage maps the generic Message to the chat-completions wire shape.
func toOpenAIMessage(m Message) openaiMessage {
	out := openaiMessage{
		Role:         m.Role,
		Content:      m.Content,
		Name:         m.Name,
		ToolCallID:   m.ToolCallID,
		CacheControl: m.CacheControl,
	}
	for _, tc := range m.ToolCalls {
		c := openaiToolCall{Type: "function", ID: tc.ID}
		c.Function.Name = tc.Tool
		c.Function.Arguments = tc.Args
		out.ToolCalls = append(out.ToolCalls, c)
	}
	return out
}

type openaiToolCallDelta struct {
	Index    int    `json:"index"`
	ID       string `json:"id,omitempty"`
	Function struct {
		Name      string `json:"name,omitempty"`
		Arguments string `json:"arguments,omitempty"`
	} `json:"function"`
}

type openaiDelta struct {
	Content   string                `json:"content"`
	ToolCalls []openaiToolCallDelta `json:"tool_calls"`
}

type chatChunk struct {
	Choices []struct {
		Delta        openaiDelta `json:"delta"`
		FinishReason string      `json:"finish_reason"`
	} `json:"choices"`
	Usage *Usage `json:"usage,omitempty"`
}

func (o *OpenAIProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	return o.StreamWithTools(ctx, messages, nil)
}

func (o *OpenAIProvider) StreamWithTools(ctx context.Context, messages []Message, tools []ToolSpec) (<-chan Delta, <-chan error) {
	deltas := make(chan Delta, 64)
	errs := make(chan error, 1)

	wireMessages := make([]openaiMessage, 0, len(messages))
	for _, m := range messages {
		wireMessages = append(wireMessages, toOpenAIMessage(m))
	}
	var wireTools []openaiTool
	for _, ts := range tools {
		wireTools = append(wireTools, openaiTool{
			Type:     "function",
			Function: openaiFunction(ts),
		})
	}

	go func() {
		defer close(deltas)
		defer close(errs)

		body, _ := json.Marshal(chatRequest{
			Model:    o.Model,
			Messages: wireMessages,
			Stream:   true,
			Usage:    &usageOpt{IncludeUsage: true},
			Tools:    wireTools,
		})

		req, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimSuffix(o.BaseURL, "/")+"/chat/completions", bytes.NewReader(body))
		if err != nil {
			errs <- err
			return
		}
		req.Header.Set("Content-Type", "application/json")
		if o.APIKey != "" {
			req.Header.Set("Authorization", "Bearer "+o.APIKey)
		}

		resp, err := o.doWithBackoff(ctx, req)
		if err != nil {
			errs <- o.startHintError(err)
			return
		}
		defer func() { _ = resp.Body.Close() }()
		if resp.StatusCode >= 400 {
			b, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
			if o.StartHint != "" && resp.StatusCode == http.StatusNotFound {
				errs <- fmt.Errorf("%s (server reachable at %s, but the model is missing: %s)", o.StartHint, o.BaseURL, strings.TrimSpace(string(b)))
				return
			}
			errs <- fmt.Errorf("provider error %d: %s", resp.StatusCode, string(b))
			return
		}

		scanner := NewSSEScanner(resp.Body)
		// Accumulated tool-call deltas keyed by choice index; flushed when the
		// provider signals completion (finish_reason=tool_calls) or at stream end.
		pending := map[int]*struct {
			id   string
			name string
			args string
		}{}
		flushTools := func() {
			if len(pending) == 0 {
				return
			}
			idxs := make([]int, 0, len(pending))
			for i := range pending {
				idxs = append(idxs, i)
			}
			sort.Ints(idxs)
			for _, i := range idxs {
				p := pending[i]
				if p.name == "" {
					continue
				}
				args := p.args
				if args == "" {
					args = "{}"
				}
				select {
				case deltas <- Delta{Kind: DeltaToolCall, ToolName: p.name, ToolArgs: args, CallID: p.id}:
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				}
			}
			pending = map[int]*struct {
				id   string
				name string
				args string
			}{}
		}
		for {
			ev, err := scanner.Next()
			if err != nil {
				if err == io.EOF {
					errs <- fmt.Errorf("stream truncated: connection closed before completion")
					return
				}
				errs <- fmt.Errorf("stream truncated: %w", err)
				return
			}
			data := strings.TrimSpace(ev.Data)
			if data == "[DONE]" {
				break
			}
			var chunk chatChunk
			if err := json.Unmarshal([]byte(data), &chunk); err != nil {
				continue
			}
			if chunk.Usage != nil {
				select {
				case deltas <- Delta{Kind: DeltaUsage, Usage: chunk.Usage}:
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				}
			}
			for _, c := range chunk.Choices {
				if c.Delta.Content != "" {
					select {
					case deltas <- Delta{Kind: DeltaText, Text: c.Delta.Content}:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
					}
				}
				for _, tc := range c.Delta.ToolCalls {
					p, ok := pending[tc.Index]
					if !ok {
						p = &struct {
							id   string
							name string
							args string
						}{}
						pending[tc.Index] = p
					}
					if tc.ID != "" {
						p.id = tc.ID
					}
					if tc.Function.Name != "" {
						p.name = tc.Function.Name
					}
					p.args += tc.Function.Arguments
				}
				switch c.FinishReason {
				case "tool_calls":
					flushTools()
					if err := ctx.Err(); err != nil {
						errs <- err
						return
					}
				case "stop", "length", "content_filter":
					if len(pending) > 0 {
						flushTools()
						if err := ctx.Err(); err != nil {
							errs <- err
							return
						}
					}
				}
			}
		}
		flushTools()
		if err := ctx.Err(); err != nil {
			errs <- err
			return
		}
		deltas <- Delta{Kind: DeltaDone}
	}()

	return deltas, errs
}

// startHintError rewrites connection-level failures on a local model path
// into the exact "what to start" instructions (I5).
func (o *OpenAIProvider) startHintError(err error) error {
	if o.StartHint == "" || err == nil {
		return err
	}
	msg := err.Error()
	if strings.Contains(msg, "connection refused") || strings.Contains(msg, "no such host") ||
		strings.Contains(msg, "i/o timeout") || strings.Contains(msg, "context deadline exceeded") {
		return errors.New(o.StartHint)
	}
	return err
}

func (o *OpenAIProvider) doWithBackoff(ctx context.Context, req *http.Request) (*http.Response, error) {
	for attempt := 0; attempt < 3; attempt++ {
		if req.GetBody != nil {
			body, err := req.GetBody()
			if err == nil {
				req.Body = body
			}
		}
		resp, err := o.Client.Do(req)
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
		case <-time.After(JitteredBackoff(10*time.Millisecond, attempt, o.jitter)):
		}
	}
	return nil, fmt.Errorf("exhausted retries")
}
