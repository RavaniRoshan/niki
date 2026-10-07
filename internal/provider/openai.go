package provider

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// OpenAIProvider talks to any OpenAI-compatible /chat/completions endpoint.
type OpenAIProvider struct {
	BaseURL string
	APIKey  string
	Model   string
	Client  *http.Client
}

func NewOpenAIProvider(baseURL, apiKey, model string) *OpenAIProvider {
	if baseURL == "" {
		baseURL = "https://api.openai.com/v1"
	}
	return &OpenAIProvider{BaseURL: baseURL, APIKey: apiKey, Model: model, Client: &http.Client{Timeout: 120 * time.Second}}
}

func (o *OpenAIProvider) Name() string { return "openai" }

type chatRequest struct {
	Model    string    `json:"model"`
	Messages []Message `json:"messages"`
	Stream   bool      `json:"stream"`
	Usage    *usageOpt `json:"stream_options,omitempty"`
}

type usageOpt struct {
	IncludeUsage bool `json:"include_usage"`
}

type chatChunk struct {
	Choices []struct {
		Delta Message `json:"delta"`
	} `json:"choices"`
	Usage *Usage `json:"usage,omitempty"`
}

func (o *OpenAIProvider) Stream(ctx context.Context, messages []Message) (<-chan Delta, <-chan error) {
	deltas := make(chan Delta, 64)
	errs := make(chan error, 1)

	go func() {
		defer close(deltas)
		defer close(errs)

		body, _ := json.Marshal(chatRequest{Model: o.Model, Messages: messages, Stream: true, Usage: &usageOpt{IncludeUsage: true}})

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
			errs <- err
			return
		}
		defer func() { _ = resp.Body.Close() }()
		if resp.StatusCode >= 400 {
			b, _ := io.ReadAll(io.LimitReader(resp.Body, 4096))
			errs <- fmt.Errorf("provider error %d: %s", resp.StatusCode, string(b))
			return
		}

		sc := bufio.NewScanner(resp.Body)
		sc.Buffer(make([]byte, 1024*1024), 1024*1024)
		for sc.Scan() {
			line := strings.TrimSpace(sc.Text())
			if !strings.HasPrefix(line, "data:") {
				continue
			}
			data := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
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
			}
		}
		if err := sc.Err(); err != nil {
			errs <- err
			return
		}
		deltas <- Delta{Kind: DeltaDone}
	}()

	return deltas, errs
}

func (o *OpenAIProvider) doWithBackoff(ctx context.Context, req *http.Request) (*http.Response, error) {
	var resp *http.Response
	var err error
	for attempt := 0; attempt < 3; attempt++ {
		if attempt > 0 {
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(time.Duration(attempt) * 200 * time.Millisecond):
			}
		}
		resp, err = o.Client.Do(req)
		if err == nil && resp.StatusCode < 500 {
			return resp, nil
		}
		if resp != nil {
			_ = resp.Body.Close()
		}
	}
	return resp, err
}
