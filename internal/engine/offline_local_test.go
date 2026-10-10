package engine

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// I5 acceptance: an offline fixture run completes a task. A loopback
// server standing in for Ollama's OpenAI-compatible endpoint drives the
// real provider client through a full tool-call round trip — no external
// network, no API key.

func sseFixtureHandler(reqCount *int32) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			http.NotFound(w, r)
			return
		}
		atomic.AddInt32(reqCount, 1)
		var req struct {
			Messages []struct {
				Role    string          `json:"role"`
				Content json.RawMessage `json:"content"`
				Tool    string          `json:"tool_call_id"`
			} `json:"messages"`
		}
		_ = json.NewDecoder(r.Body).Decode(&req)

		w.Header().Set("Content-Type", "text/event-stream")
		hasToolResult := false
		for _, m := range req.Messages {
			if m.Role == "tool" || m.Tool != "" {
				hasToolResult = true
			}
		}
		if !hasToolResult {
			// Round 1: request the tool call.
			fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_offline_1","function":{"name":"read_file","arguments":""}}]}}]}`+"\n\n")
			fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_offline_1","function":{"arguments":"{\"path\":\"engine.go\"}"}}]}}]}`+"\n\n")
			fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}`+"\n\n")
			fmt.Fprint(w, "data: [DONE]\n\n")
			return
		}
		// Round 2: complete the task with the final answer.
		fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{"role":"assistant","content":"I read engine.go — offline task complete."}}]}`+"\n\n")
		fmt.Fprint(w, `data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}`+"\n\n")
		fmt.Fprint(w, "data: [DONE]\n\n")
	}
}

func TestOfflineFixtureLocalModelCompletesTask(t *testing.T) {
	var reqCount int32
	srv := httptest.NewServer(sseFixtureHandler(&reqCount))
	defer srv.Close()

	// Real Ollama client shape, pointed at the loopback fixture.
	prov := provider.NewOllamaProvider("qwen2.5-coder:7b-instruct-q4_K_M")
	prov.BaseURL = srv.URL + "/v1"
	if prov.APIKey != "" {
		t.Fatal("local model path must not require an API key")
	}

	runner := &TurnRunner{
		verifier: passVerifier(),
		Provider: prov,
		Registry: tools.DefaultRegistry(),
		Context:  NewContextAssembler(),
		Perm:     permissions.NewGuard(permissions.ModeFullAccess),
	}

	var events []protocol.EngineEvent
	var texts []string
	emit := func(evt protocol.EngineEvent) {
		events = append(events, evt)
		if evt.Type == protocol.EventAssistantTextDelta && evt.Text != "" {
			texts = append(texts, evt.Text)
		}
	}

	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	if err := runner.Run(ctx, "read engine.go", emit); err != nil {
		t.Fatalf("offline fixture turn failed: %v", err)
	}

	var sawToolStart, sawToolDone, sawTurnDone bool
	for _, ev := range events {
		switch ev.Type {
		case protocol.EventToolStarted:
			if ev.ToolName == "read_file" {
				sawToolStart = true
			}
		case protocol.EventToolCompleted:
			if ev.ToolName == "read_file" && ev.Error == "" {
				sawToolDone = true
			}
		case protocol.EventTurnCompleted:
			sawTurnDone = true
		}
	}
	if !sawToolStart || !sawToolDone {
		t.Fatalf("tool round trip incomplete (start=%v done=%v)", sawToolStart, sawToolDone)
	}
	if !sawTurnDone {
		t.Fatal("turn did not complete")
	}
	joined := strings.Join(texts, "")
	if !strings.Contains(joined, "offline task complete") {
		t.Fatalf("missing final answer, deltas: %q", joined)
	}
	if got := atomic.LoadInt32(&reqCount); got != 2 {
		t.Fatalf("expected exactly 2 loopback round trips, got %d", got)
	}
	t.Logf("[I5 offline run] task complete: %d round trips, tool read_file executed, final=%q (all loopback)", reqCount, "I read engine.go — offline task complete.")
}
