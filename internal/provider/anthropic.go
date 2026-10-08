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

type anthropicMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type anthropicRequest struct {
	Model     string             `json:"model"`
	Messages  []anthropicMessage `json:"messages"`
	System    string             `json:"system,omitempty"`
	MaxTokens int                `json:"max_tokens"`
	Stream    bool               `json:"stream"`
}

type anthropicDelta struct {
	Type  string `json:"type"`
	Delta struct {
		Type string `json:"type"`
		Text string `json:"text"`
	} `json:"delta"`
	Usage *struct {
		InputTokens  int `json:"input_tokens"`
		OutputTokens int `json:"output_tokens"`
	} `json:"usage"`
}

func (a *AnthropicProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	deltas := make(chan Delta, 64)
	errs := make(chan error, 1)

	go func() {
		defer close(deltas)
		defer close(errs)

		var system string
		var conv []anthropicMessage
		for _, m := range messages {
			if m.Role == "system" {
				system = m.Content
			} else {
				conv = append(conv, anthropicMessage{Role: m.Role, Content: m.Content})
			}
		}

		body, _ := json.Marshal(anthropicRequest{
			Model:     a.Model,
			Messages:  conv,
			System:    system,
			MaxTokens: 4096,
			Stream:    true,
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
				if deltaEv.Delta.Text != "" {
					select {
					case deltas <- Delta{Kind: DeltaText, Text: deltaEv.Delta.Text}:
					case <-ctx.Done():
						errs <- ctx.Err()
						return
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
	backoff := 10 * time.Millisecond
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
				return nil, err
			}
			return nil, fmt.Errorf("exhausted retries (last status %d)", resp.StatusCode)
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(backoff):
			backoff *= 2
		}
	}
	return nil, fmt.Errorf("exhausted retries")
}
