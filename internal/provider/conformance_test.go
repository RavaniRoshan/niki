package provider

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
)

type providerTestCase struct {
	name           string
	createProvider func(baseURL string) ModelProvider
	successPayload func() string
	truncatedData  func() string
}

func getConformanceTestCases() []providerTestCase {
	return []providerTestCase{
		{
			name: "AnthropicProvider",
			createProvider: func(baseURL string) ModelProvider {
				return NewAnthropicProvider(baseURL, "test-anthropic-key", "claude-3-5-sonnet")
			},
			successPayload: func() string {
				return strings.Join([]string{
					`data: {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hello "}}`,
					`data: {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "from Claude!"}}`,
					`data: {"type": "message_delta", "usage": {"input_tokens": 10, "output_tokens": 20}}`,
					`data: {"type": "message_stop"}`,
					`data: [DONE]`,
					"",
				}, "\n")
			},
			truncatedData: func() string {
				return `data: {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Incomplete"}}` + "\n"
			},
		},
		{
			name: "ResponsesProvider",
			createProvider: func(baseURL string) ModelProvider {
				return NewResponsesProvider(baseURL, "test-openai-key", "gpt-4o")
			},
			successPayload: func() string {
				return strings.Join([]string{
					`data: {"type": "response.output_text.delta", "delta": "Hello "}`,
					`data: {"type": "response.output_text.delta", "delta": "from Responses!"}`,
					`data: {"type": "response.completed", "response": {"usage": {"input_tokens": 12, "output_tokens": 24, "total_tokens": 36}}}`,
					`data: [DONE]`,
					"",
				}, "\n")
			},
			truncatedData: func() string {
				return `data: {"type": "response.output_text.delta", "delta": "Incomplete"}` + "\n"
			},
		},
	}
}

// TestProviderConformanceStreaming verifies both Anthropic and Responses providers
// stream deltas and token usage cleanly.
func TestProviderConformanceStreaming(t *testing.T) {
	for _, tc := range getConformanceTestCases() {
		t.Run(tc.name, func(t *testing.T) {
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "text/event-stream")
				w.WriteHeader(http.StatusOK)
				_, _ = fmt.Fprint(w, tc.successPayload())
			}))
			defer srv.Close()

			prov := tc.createProvider(srv.URL)
			deltas, errs := prov.Stream(context.Background(), []Message{{Role: "user", Content: "Hello"}})

			var text strings.Builder
			var usage *Usage
			sawDone := false

			for d := range deltas {
				switch d.Kind {
				case DeltaText:
					text.WriteString(d.Text)
				case DeltaUsage:
					usage = d.Usage
				case DeltaDone:
					sawDone = true
				}
			}

			if err := <-errs; err != nil {
				t.Fatalf("unexpected stream error: %v", err)
			}
			if !sawDone {
				t.Fatal("expected DeltaDone event")
			}
			if !strings.Contains(text.String(), "Hello from") {
				t.Fatalf("unexpected text output: %q", text.String())
			}
			if usage == nil {
				t.Fatal("expected DeltaUsage event with token stats")
			}
			if usage.CompletionTokens == 0 {
				t.Fatalf("expected non-zero completion tokens: %+v", usage)
			}
		})
	}
}

// TestProviderConformanceRetry429 verifies providers automatically retry HTTP 429
// rate limits with backoff and succeed on subsequent attempt.
func TestProviderConformanceRetry429(t *testing.T) {
	for _, tc := range getConformanceTestCases() {
		t.Run(tc.name, func(t *testing.T) {
			var attempts atomic.Int32
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				count := attempts.Add(1)
				if count == 1 {
					w.WriteHeader(http.StatusTooManyRequests)
					_, _ = fmt.Fprintln(w, `{"error": "rate limit exceeded"}`)
					return
				}
				w.Header().Set("Content-Type", "text/event-stream")
				w.WriteHeader(http.StatusOK)
				_, _ = fmt.Fprint(w, tc.successPayload())
			}))
			defer srv.Close()

			prov := tc.createProvider(srv.URL)
			deltas, errs := prov.Stream(context.Background(), []Message{{Role: "user", Content: "Hello"}})

			var text strings.Builder
			for d := range deltas {
				if d.Kind == DeltaText {
					text.WriteString(d.Text)
				}
			}

			if err := <-errs; err != nil {
				t.Fatalf("unexpected error after 429 retry: %v", err)
			}
			if attempts.Load() < 2 {
				t.Fatalf("expected at least 2 attempts, got %d", attempts.Load())
			}
			if !strings.Contains(text.String(), "Hello from") {
				t.Fatalf("text stream recovered unexpectedly: %q", text.String())
			}
		})
	}
}

// TestProviderConformanceRetry5xx verifies providers automatically retry HTTP 500/503
// server errors with backoff and succeed on subsequent attempt.
func TestProviderConformanceRetry5xx(t *testing.T) {
	for _, tc := range getConformanceTestCases() {
		t.Run(tc.name, func(t *testing.T) {
			var attempts atomic.Int32
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				count := attempts.Add(1)
				if count == 1 {
					w.WriteHeader(http.StatusBadGateway)
					_, _ = fmt.Fprintln(w, `{"error": "bad gateway"}`)
					return
				}
				w.Header().Set("Content-Type", "text/event-stream")
				w.WriteHeader(http.StatusOK)
				_, _ = fmt.Fprint(w, tc.successPayload())
			}))
			defer srv.Close()

			prov := tc.createProvider(srv.URL)
			deltas, errs := prov.Stream(context.Background(), []Message{{Role: "user", Content: "Hello"}})

			var text strings.Builder
			for d := range deltas {
				if d.Kind == DeltaText {
					text.WriteString(d.Text)
				}
			}

			if err := <-errs; err != nil {
				t.Fatalf("unexpected error after 5xx retry: %v", err)
			}
			if attempts.Load() < 2 {
				t.Fatalf("expected at least 2 attempts, got %d", attempts.Load())
			}
			if !strings.Contains(text.String(), "Hello from") {
				t.Fatalf("text stream recovered unexpectedly: %q", text.String())
			}
		})
	}
}

// TestProviderConformanceExhaustedRetries verifies that after max retries are exceeded,
// providers return a clear error indicating exhausted retries.
func TestProviderConformanceExhaustedRetries(t *testing.T) {
	for _, tc := range getConformanceTestCases() {
		t.Run(tc.name, func(t *testing.T) {
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(http.StatusServiceUnavailable)
				_, _ = fmt.Fprintln(w, `{"error": "overloaded"}`)
			}))
			defer srv.Close()

			prov := tc.createProvider(srv.URL)
			deltas, errs := prov.Stream(context.Background(), []Message{{Role: "user", Content: "Hello"}})

			for range deltas {
			}

			err := <-errs
			if err == nil {
				t.Fatal("expected error after exhausted retries, got nil")
			}
			if !strings.Contains(err.Error(), "exhausted retries") && !strings.Contains(err.Error(), "503") {
				t.Fatalf("expected clear exhausted retries error, got: %v", err)
			}
		})
	}
}

// TestProviderConformanceTruncatedStream verifies providers detect premature connection
// termination and return a clear "stream truncated" error.
func TestProviderConformanceTruncatedStream(t *testing.T) {
	for _, tc := range getConformanceTestCases() {
		t.Run(tc.name, func(t *testing.T) {
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "text/event-stream")
				w.WriteHeader(http.StatusOK)
				// Write partial chunk and immediately close connection without [DONE] or stop
				_, _ = fmt.Fprint(w, tc.truncatedData())
			}))
			defer srv.Close()

			prov := tc.createProvider(srv.URL)
			deltas, errs := prov.Stream(context.Background(), []Message{{Role: "user", Content: "Hello"}})

			for range deltas {
			}

			err := <-errs
			if err == nil {
				t.Fatal("expected stream truncated error, got nil")
			}
			if !strings.Contains(err.Error(), "stream truncated") {
				t.Fatalf("expected error mentioning stream truncated, got: %v", err)
			}
		})
	}
}
