package provider

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// I5: with no local model available, the error says exactly what to
// start — the Ollama commands, verbatim.

// closedPort returns a loopback URL nothing listens on.
func closedPort(t *testing.T) string {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := l.Addr().String()
	_ = l.Close()
	return "http://" + addr + "/v1"
}

// Connection refused: nothing listening.
func TestOllamaNoModelExactStartErrorConnectionRefused(t *testing.T) {
	p := NewOllamaProvider("qwen2.5-coder:7b-instruct-q4_K_M")
	p.BaseURL = closedPort(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_, errs := p.Stream(ctx, []Message{{Role: "user", Content: "hi"}})
	err := <-errs
	if err == nil {
		t.Fatal("expected an error when no local model is running")
	}
	msg := err.Error()
	for _, want := range []string{"ollama serve", "ollama pull", "qwen2.5-coder:7b-instruct-q4_K_M"} {
		if !strings.Contains(msg, want) {
			t.Errorf("error must say exactly what to start (missing %q):\n%s", want, msg)
		}
	}
	if !strings.Contains(strings.ToLower(msg), "no local model") {
		t.Errorf("error must name the condition (no local model):\n%s", msg)
	}
	t.Logf("[I5 start error — connection refused] %s", msg)
}

// Server up but the model was never pulled (Ollama 404): same exact fix.
func TestOllamaNoModelExactStartErrorModelMissing(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusNotFound)
		_, _ = w.Write([]byte(`{"error":"model 'qwen2.5-coder:7b-instruct-q4_K_M' not found, try pulling it first"}`))
	}))
	defer srv.Close()

	p := NewOllamaProvider("qwen2.5-coder:7b-instruct-q4_K_M")
	p.BaseURL = srv.URL + "/v1"

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_, errs := p.Stream(ctx, []Message{{Role: "user", Content: "hi"}})
	err := <-errs
	if err == nil {
		t.Fatal("expected an error when the model is missing")
	}
	msg := err.Error()
	for _, want := range []string{"ollama serve", "ollama pull", "qwen2.5-coder:7b-instruct-q4_K_M"} {
		if !strings.Contains(msg, want) {
			t.Errorf("error must say exactly what to start (missing %q):\n%s", want, msg)
		}
	}
	t.Logf("[I5 start error — model missing] %s", msg)
}

// The local provider is fully offline: loopback base URL, no key, and it
// streams a tool call from an Ollama-compatible endpoint.
func TestOllamaProviderOfflineLoopbackStream(t *testing.T) {
	var sawAuth string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			http.NotFound(w, r)
			return
		}
		sawAuth = r.Header.Get("Authorization")
		w.Header().Set("Content-Type", "text/event-stream")
		fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":""}}]}}]}`+"\n\n")
		fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"arguments":"{\"path\":\"engine.go\"}"}}]}}]}`+"\n\n")
		fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}`+"\n\n")
		fmt.Fprint(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	p := NewOllamaProvider("qwen2.5-coder:7b-instruct-q4_K_M")
	p.BaseURL = srv.URL + "/v1"

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	deltas, errs := p.StreamWithTools(ctx, []Message{{Role: "user", Content: "read engine"}}, []ToolSpec{{Name: "read_file"}})

	var toolCall *Delta
	for d := range deltas {
		if d.Kind == DeltaToolCall {
			dd := d
			toolCall = &dd
		}
	}
	select {
	case err := <-errs:
		if err != nil {
			t.Fatalf("stream error: %v", err)
		}
	default:
	}
	if toolCall == nil {
		t.Fatal("expected a tool call from the loopback Ollama endpoint")
	}
	if toolCall.ToolName != "read_file" || !strings.Contains(toolCall.ToolArgs, "engine.go") {
		t.Fatalf("bad tool call: %+v", toolCall)
	}
	if sawAuth != "" {
		t.Fatalf("local model must not send an Authorization header, got %q", sawAuth)
	}
	if strings.Contains(srv.URL, "openai.com") || strings.Contains(srv.URL, "anthropic.com") {
		t.Fatal("offline fixture must be loopback only")
	}
	t.Logf("[I5 offline stream] tool=%s args=%s (loopback %s, no auth header)", toolCall.ToolName, toolCall.ToolArgs, srv.URL)
}
