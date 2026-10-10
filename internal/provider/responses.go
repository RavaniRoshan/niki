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

// ResponsesProvider interacts with the OpenAI Responses API (/v1/responses).
type ResponsesProvider struct {
	BaseURL string
	APIKey  string
	Model   string
	Client  *http.Client

	// jitter scales retry backoff (L6); nil means rand.Float64.
	jitter func() float64
}

// NewResponsesProvider constructs a provider targeting the Responses API.
func NewResponsesProvider(baseURL, apiKey, model string) *ResponsesProvider {
	if baseURL == "" {
		baseURL = "https://api.openai.com/v1"
	}
	return &ResponsesProvider{
		BaseURL: baseURL,
		APIKey:  apiKey,
		Model:   model,
		Client:  &http.Client{Timeout: 120 * time.Second},
	}
}

func (r *ResponsesProvider) Name() string { return "responses" }

// responsesItem is a Responses-API input item: plain role messages, prior
// assistant function calls, and function call outputs.
type responsesItem struct {
	Type      string `json:"type,omitempty"`
	Role      string `json:"role,omitempty"`
	Content   string `json:"content,omitempty"`
	CallID    string `json:"call_id,omitempty"`
	Name      string `json:"name,omitempty"`
	Arguments string `json:"arguments,omitempty"`
	Output    string `json:"output,omitempty"`
}

type responsesRequest struct {
	Model  string          `json:"model"`
	Input  []responsesItem `json:"input"`
	Stream bool            `json:"stream"`
	Tools  []responsesTool `json:"tools,omitempty"`
}

type responsesTool struct {
	Type        string          `json:"type"`
	Name        string          `json:"name"`
	Description string          `json:"description,omitempty"`
	Parameters  json.RawMessage `json:"parameters,omitempty"`
}

// mapResponsesInput converts generic messages into Responses input items.
func mapResponsesInput(messages []Message) []responsesItem {
	var out []responsesItem
	for _, m := range messages {
		switch m.Role {
		case "system":
			out = append(out, responsesItem{Role: "system", Content: m.Content})
		case "user":
			out = append(out, responsesItem{Role: "user", Content: m.Content})
		case "assistant":
			if m.Content != "" {
				out = append(out, responsesItem{Role: "assistant", Content: m.Content})
			}
			for _, tc := range m.ToolCalls {
				id := tc.ID
				if id == "" {
					id = "call_" + tc.Tool
				}
				out = append(out, responsesItem{Type: "function_call", CallID: id, Name: tc.Tool, Arguments: tc.Args})
			}
		case "tool":
			id := m.ToolCallID
			if id == "" {
				id = "call_" + m.Name
			}
			out = append(out, responsesItem{Type: "function_call_output", CallID: id, Output: m.Content})
		}
	}
	return out
}

type responsesDeltaPayload struct {
	Type  string `json:"type,omitempty"`
	Delta string `json:"delta,omitempty"`
	Item  *struct {
		Type      string `json:"type"`
		ID        string `json:"id"`
		CallID    string `json:"call_id"`
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"item,omitempty"`
	Response *struct {
		Usage *struct {
			InputTokens  int `json:"input_tokens"`
			OutputTokens int `json:"output_tokens"`
			TotalTokens  int `json:"total_tokens"`
		} `json:"usage"`
	} `json:"response,omitempty"`
	Usage *struct {
		InputTokens  int `json:"input_tokens"`
		OutputTokens int `json:"output_tokens"`
		TotalTokens  int `json:"total_tokens"`
	} `json:"usage,omitempty"`
}

func (r *ResponsesProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	return r.StreamWithTools(ctx, messages, nil)
}

func (r *ResponsesProvider) StreamWithTools(ctx context.Context, messages []Message, tools []ToolSpec) (<-chan Delta, <-chan error) {
	deltas := make(chan Delta, 64)
	errs := make(chan error, 1)

	go func() {
		defer close(deltas)
		defer close(errs)

		var wireTools []responsesTool
		for _, ts := range tools {
			wireTools = append(wireTools, responsesTool{
				Type:        "function",
				Name:        ts.Name,
				Description: ts.Description,
				Parameters:  ts.Parameters,
			})
		}

		body, err := json.Marshal(responsesRequest{
			Model:  r.Model,
			Input:  mapResponsesInput(messages),
			Stream: true,
			Tools:  wireTools,
		})
		if err != nil {
			errs <- err
			return
		}

		url := strings.TrimSuffix(r.BaseURL, "/")
		if !strings.HasSuffix(url, "/responses") {
			url += "/responses"
		}

		req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
		if err != nil {
			errs <- err
			return
		}
		req.Header.Set("Content-Type", "application/json")
		if r.APIKey != "" {
			req.Header.Set("Authorization", "Bearer "+r.APIKey)
		}

		resp, err := r.doWithBackoff(ctx, req)
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
		sawCompletion := false

		for {
			ev, err := scanner.Next()
			if err != nil {
				if err == io.EOF {
					if !sawCompletion {
						errs <- fmt.Errorf("stream truncated: connection closed before completion")
						return
					}
					break
				}
				errs <- fmt.Errorf("stream truncated: %w", err)
				return
			}

			data := strings.TrimSpace(ev.Data)
			if data == "[DONE]" || ev.Event == "response.done" || ev.Event == "response.completed" {
				sawCompletion = true
			}

			if data == "[DONE]" {
				break
			}

			var payload responsesDeltaPayload
			if err := json.Unmarshal([]byte(data), &payload); err == nil {
				if payload.Type == "response.output_text.delta" && payload.Delta != "" {
					select {
					case deltas <- Delta{Kind: DeltaText, Text: payload.Delta}:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
					}
				}
				if payload.Type == "response.output_item.done" && payload.Item != nil && payload.Item.Type == "function_call" {
					args := payload.Item.Arguments
					if args == "" {
						args = "{}"
					}
					callID := payload.Item.CallID
					if callID == "" {
						callID = payload.Item.ID
					}
					select {
					case deltas <- Delta{Kind: DeltaToolCall, ToolName: payload.Item.Name, ToolArgs: args, CallID: callID}:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
					}
				}

				u := payload.Usage
				if u == nil && payload.Response != nil && payload.Response.Usage != nil {
					u = payload.Response.Usage
				}
				if u != nil {
					total := u.TotalTokens
					if total == 0 {
						total = u.InputTokens + u.OutputTokens
					}
					select {
					case deltas <- Delta{Kind: DeltaUsage, Usage: &Usage{
						PromptTokens:     u.InputTokens,
						CompletionTokens: u.OutputTokens,
						TotalTokens:      total,
					}}:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
					}
				}
			}

			if sawCompletion {
				break
			}
		}

		deltas <- Delta{Kind: DeltaDone}
	}()

	return deltas, errs
}

func (r *ResponsesProvider) doWithBackoff(ctx context.Context, req *http.Request) (*http.Response, error) {
	for attempt := 0; attempt < 3; attempt++ {
		if req.GetBody != nil {
			body, err := req.GetBody()
			if err == nil {
				req.Body = body
			}
		}

		resp, err := r.Client.Do(req)
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
		case <-time.After(JitteredBackoff(10*time.Millisecond, attempt, r.jitter)):
		}
	}
	return nil, fmt.Errorf("exhausted retries")
}
