package provider

import (
	"context"
	"fmt"
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
