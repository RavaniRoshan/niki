package provider

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestMockProviderStreamsDeltas(t *testing.T) {
	m := NewMockProvider()
	m.ChunkDelay = time.Millisecond
	deltas, errs := m.Stream(context.Background(), []Message{{Role: "user", Content: "hello world"}})
	var text strings.Builder
	sawUsage, sawDone := false, false
	for d := range deltas {
		switch d.Kind {
		case DeltaText:
			text.WriteString(d.Text)
		case DeltaUsage:
			sawUsage = true
		case DeltaDone:
			sawDone = true
		}
	}
	if err := <-errs; err != nil {
		t.Fatal(err)
	}
	if !sawUsage || !sawDone {
		t.Fatalf("usage=%v done=%v", sawUsage, sawDone)
	}
	if !strings.Contains(text.String(), "hello world") {
		t.Fatalf("unexpected response: %q", text.String())
	}
}

func TestOpenAIProviderStreamsSSE(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Errorf("bad path %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprintln(w, `data: {"choices":[{"delta":{"role":"assistant","content":"Hello"}}]}`)
		fmt.Fprintln(w, ``)
		fmt.Fprintln(w, `data: {"choices":[{"delta":{"content":" there"}}]}`)
		fmt.Fprintln(w, `data: [DONE]`)
	}))
	defer srv.Close()

	p := NewOpenAIProvider(srv.URL, "key", "gpt-test")
	deltas, errs := p.Stream(context.Background(), []Message{{Role: "user", Content: "hi"}})
	var text strings.Builder
	for d := range deltas {
		if d.Kind == DeltaText {
			text.WriteString(d.Text)
		}
	}
	if err := <-errs; err != nil {
		t.Fatal(err)
	}
	if text.String() != "Hello there" {
		t.Fatalf("got %q", text.String())
	}
}

// TestOpenAIProviderParsesUsage (P4): the final
// SSE chunk carries token usage; the provider must
// surface it as a DeltaUsage event.
func TestOpenAIProviderParsesUsage(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprintln(w, `data: {"choices":[{"delta":{"content":"hi"}}]}`)
		fmt.Fprintln(w, `data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":34}}`)
		fmt.Fprintln(w, `data: [DONE]`)
	}))
	defer srv.Close()

	p := NewOpenAIProvider(srv.URL, "key", "gpt-test")
	deltas, errs := p.Stream(context.Background(), []Message{{Role: "user", Content: "hi"}})
	var usage *Usage
	for d := range deltas {
		if d.Kind == DeltaUsage {
			usage = d.Usage
		}
	}
	if err := <-errs; err != nil {
		t.Fatal(err)
	}
	if usage == nil {
		t.Fatal("no usage delta received")
	}
	if usage.PromptTokens != 12 || usage.CompletionTokens != 34 {
		t.Errorf("usage = %+v, want prompt=12 completion=34", usage)
	}
}

// TestOpenAIProviderSendsAuthHeader (P3): the API
// key travels as a bearer token and the request
// body names the model and streams.
func TestOpenAIProviderSendsAuthHeader(t *testing.T) {
	var gotAuth, gotBody string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		b, _ := io.ReadAll(r.Body)
		gotBody = string(b)
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprintln(w, `data: [DONE]`)
	}))
	defer srv.Close()

	p := NewOpenAIProvider(srv.URL, "secret-key", "gpt-test")
	deltas, errs := p.Stream(context.Background(), []Message{{Role: "user", Content: "hi"}})
	for range deltas {
	}
	if err := <-errs; err != nil {
		t.Fatal(err)
	}
	if gotAuth != "Bearer secret-key" {
		t.Errorf("Authorization = %q", gotAuth)
	}
	if !strings.Contains(gotBody, `"model":"gpt-test"`) {
		t.Errorf("request body missing model: %s", gotBody)
	}
	if !strings.Contains(gotBody, `"stream":true`) {
		t.Errorf("request body not streaming: %s", gotBody)
	}
}

// TestOpenAIProviderBacksOffOnServerError (P4): a
// 500 is retried with backoff; the retry's stream
// is delivered, not the error.
func TestOpenAIProviderBacksOffOnServerError(t *testing.T) {
	var attempts int
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		if attempts == 1 {
			w.WriteHeader(http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprintln(w, `data: {"choices":[{"delta":{"content":"recovered"}}]}`)
		fmt.Fprintln(w, `data: [DONE]`)
	}))
	defer srv.Close()

	p := NewOpenAIProvider(srv.URL, "key", "gpt-test")
	deltas, errs := p.Stream(context.Background(), []Message{{Role: "user", Content: "hi"}})
	var text strings.Builder
	for d := range deltas {
		if d.Kind == DeltaText {
			text.WriteString(d.Text)
		}
	}
	if err := <-errs; err != nil {
		t.Fatal(err)
	}
	if attempts < 2 {
		t.Errorf("server error was not retried (attempts=%d)", attempts)
	}
	if text.String() != "recovered" {
		t.Errorf("got %q, want the retried stream", text.String())
	}
}

// TestOpenAIProviderSurfacesPersistentServerError
// asserts that a server error which never clears is
// reported after the retries are exhausted.
func TestOpenAIProviderSurfacesPersistentServerError(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()

	p := NewOpenAIProvider(srv.URL, "key", "gpt-test")
	deltas, errs := p.Stream(context.Background(), []Message{{Role: "user", Content: "hi"}})
	for range deltas {
	}
	err := <-errs
	if err == nil {
		t.Fatal("expected an error after exhausted retries")
	}
	if !strings.Contains(err.Error(), "500") {
		t.Errorf("error should mention the status: %v", err)
	}
}
