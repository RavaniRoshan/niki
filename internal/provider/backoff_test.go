package provider

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestJitteredBackoffBounds(t *testing.T) {
	base := 10 * time.Millisecond
	cases := []struct {
		name    string
		attempt int
		rnd     float64
		wantMin time.Duration
		wantMax time.Duration
	}{
		{"attempt0 low", 0, 0, 5 * time.Millisecond, 10 * time.Millisecond},
		{"attempt0 high", 0, 0.999999, 5 * time.Millisecond, 15 * time.Millisecond},
		{"attempt1 low", 1, 0, 10 * time.Millisecond, 30 * time.Millisecond},
		{"attempt1 high", 1, 0.999999, 10 * time.Millisecond, 30 * time.Millisecond},
		{"attempt2 mid", 2, 0.5, 20 * time.Millisecond, 60 * time.Millisecond},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := JitteredBackoff(base, c.attempt, func() float64 { return c.rnd })
			if got < c.wantMin || got > c.wantMax {
				t.Fatalf("[L6] JitteredBackoff(%d, rnd=%v) = %v, want [%v, %v)", c.attempt, c.rnd, got, c.wantMin, c.wantMax)
			}
		})
	}
	// Two different random draws for the same attempt must differ —
	// proving the backoff is jittered, not a fixed ladder.
	a := JitteredBackoff(base, 0, func() float64 { return 0.1 })
	b := JitteredBackoff(base, 0, func() float64 { return 0.9 })
	if a == b {
		t.Fatalf("[L6] backoff not jittered: %v == %v", a, b)
	}
	t.Logf("[L6 jitter] attempt0 rnd=0.1 -> %v, rnd=0.9 -> %v (base %v)", a, b, base)
}

func TestIsRetryable(t *testing.T) {
	retryable := []string{
		"stream truncated: connection closed before completion",
		"stream truncated: unexpected EOF",
		"write tcp: connection reset by peer",
	}
	for _, m := range retryable {
		if !IsRetryable(fmt.Errorf("%s", m)) {
			t.Fatalf("expected retryable: %q", m)
		}
	}
	notRetryable := []string{
		"exhausted retries (last status 429)",
		"invalid api key",
		"no local model available — start it with `ollama serve`",
	}
	for _, m := range notRetryable {
		if IsRetryable(fmt.Errorf("%s", m)) {
			t.Fatalf("expected NOT retryable: %q", m)
		}
	}
}

// TestOpenAIRetryUsesJitter: 429-then-200 recovers AND the jitter
// source is consulted once per retry (wired, not decorative).
func TestOpenAIRetryUsesJitter(t *testing.T) {
	var attempts atomic.Int32
	var jitterCalls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if attempts.Add(1) == 1 {
			w.WriteHeader(http.StatusTooManyRequests)
			_, _ = fmt.Fprintln(w, `{"error":"rate limit"}`)
			return
		}
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = fmt.Fprint(w, "data: {\"choices\":[{\"delta\":{\"content\":\"recovered\"}}]}\n\n")
		_, _ = fmt.Fprint(w, "data: [DONE]\n\n")
	}))
	defer srv.Close()

	prov := NewOpenAIProvider(srv.URL, "k", "m")
	prov.jitter = func() float64 {
		jitterCalls.Add(1)
		return 0.5
	}

	deltas, errs := prov.Stream(context.Background(), []Message{{Role: "user", Content: "hi"}})
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
		t.Fatalf("[L6] expected >=2 attempts, got %d", attempts.Load())
	}
	if jitterCalls.Load() < 1 {
		t.Fatalf("[L6] jitter source never consulted on retry (%d calls)", jitterCalls.Load())
	}
	if !strings.Contains(text.String(), "recovered") {
		t.Fatalf("text = %q", text.String())
	}
	t.Logf("[L6 429] attempts=%d jitterCalls=%d text=%q", attempts.Load(), jitterCalls.Load(), text.String())
}
