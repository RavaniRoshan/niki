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

type responsesMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type responsesRequest struct {
	Model  string             `json:"model"`
	Input  []responsesMessage `json:"input"`
	Stream bool               `json:"stream"`
}

type responsesDeltaPayload struct {
	Type     string `json:"type,omitempty"`
	Delta    string `json:"delta,omitempty"`
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
	deltas := make(chan Delta, 64)
	errs := make(chan error, 1)

	go func() {
		defer close(deltas)
		defer close(errs)

		var input []responsesMessage
		for _, m := range messages {
			input = append(input, responsesMessage{Role: m.Role, Content: m.Content})
		}

		body, err := json.Marshal(responsesRequest{
			Model:  r.Model,
			Input:  input,
			Stream: true,
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
				if payload.Delta != "" {
					select {
					case deltas <- Delta{Kind: DeltaText, Text: payload.Delta}:
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
	backoff := 10 * time.Millisecond
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
		case <-time.After(backoff):
			backoff *= 2
		}
	}
	return nil, fmt.Errorf("exhausted retries")
}
