package engine

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// truncProvider fails stream calls with "stream truncated" before
// emitting anything for the first failFirst calls, then answers with
// a completion claim. It records each call time for backoff checks.
type truncProvider struct {
	mu            sync.Mutex
	calls         int
	failFirst     int
	failAfterText bool // emit one delta, then truncate (must NOT retry)
	times         []time.Time
}

func (p *truncProvider) Name() string { return "trunc-fixture" }

func (p *truncProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	p.calls++
	n := p.calls
	p.times = append(p.times, time.Now())
	p.mu.Unlock()

	deltas := make(chan provider.Delta, 8)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		if n <= p.failFirst {
			errs <- fmt.Errorf("stream truncated: connection closed before completion")
			return
		}
		if p.failAfterText && n == 1 {
			deltas <- provider.Delta{Kind: provider.DeltaText, Text: "partial "}
			errs <- fmt.Errorf("stream truncated: connection closed before completion")
			return
		}
		for _, w := range strings.Split("retry claim done", " ") {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case deltas <- provider.Delta{Kind: provider.DeltaText, Text: w + " "}:
			}
		}
		deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 4, CompletionTokens: 4}}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

func runRetryFixture(t *testing.T, p provider.ModelProvider) []protocol.EngineEvent {
	t.Helper()
	reg := tools.DefaultRegistry()
	eng, cmdChan, eventChan := NewEngine(100, p, reg, permissions.NewGuard(permissions.ModeWorkspaceWrite))
	eng.SetVerifier(passVerifier(), 0)
	go func() { _ = eng.Run() }()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "go"}
	var events []protocol.EngineEvent
	deadline := time.After(15 * time.Second)
	for {
		select {
		case evt := <-eventChan:
			events = append(events, evt)
			if evt.Type == protocol.EventTurnCompleted ||
				evt.Type == protocol.EventTurnFailed ||
				evt.Type == protocol.EventTurnNotVerified ||
				evt.Type == protocol.EventTurnCancelled {
				return events
			}
		case <-deadline:
			t.Fatal("[L6] fixture run timed out (possible retry hang)")
		}
	}
}

func countEvents(events []protocol.EngineEvent, typ protocol.EventType) int {
	n := 0
	for _, e := range events {
		if e.Type == typ {
			n++
		}
	}
	return n
}

// A truncated stream before any output is retried with jittered
// backoff and the run completes.
func TestTruncatedStreamRetriedWithJitteredBackoff(t *testing.T) {
	p := &truncProvider{failFirst: 2}
	events := runRetryFixture(t, p)

	if countEvents(events, protocol.EventTurnCompleted) != 1 {
		t.Fatalf("[L6] want exactly 1 turn_completed, got %d", countEvents(events, protocol.EventTurnCompleted))
	}
	if countEvents(events, protocol.EventTurnFailed) != 0 {
		t.Fatalf("[L6] run failed despite retryable truncation")
	}
	p.mu.Lock()
	calls := p.calls
	times := append([]time.Time(nil), p.times...)
	p.mu.Unlock()
	if calls < 3 {
		t.Fatalf("[L6] provider calls = %d, want >= 3 (2 truncated + 1 success)", calls)
	}

	// Retry warnings were shown.
	warnings := 0
	for _, e := range events {
		if e.Type == protocol.EventWarning && strings.Contains(e.Text, "retrying") {
			warnings++
		}
	}
	if warnings < 2 {
		t.Fatalf("[L6] retry warnings = %d, want >= 2", warnings)
	}

	// Jittered backoff floors: engine base is 50ms -> equal jitter
	// gives >=25ms before retry 1 and >=50ms before retry 2.
	if len(times) >= 3 {
		gap1 := times[1].Sub(times[0])
		gap2 := times[2].Sub(times[1])
		if gap1 < 20*time.Millisecond {
			t.Fatalf("[L6] backoff before retry 1 = %v, want >= ~25ms (jittered)", gap1)
		}
		if gap2 < 45*time.Millisecond {
			t.Fatalf("[L6] backoff before retry 2 = %v, want >= ~50ms (jittered)", gap2)
		}
		t.Logf("[L6 truncated] calls=%d gap1=%v gap2=%v warnings=%d completed", calls, gap1, gap2, warnings)
	} else {
		t.Logf("[L6 truncated] calls=%d warnings=%d completed (fewer timestamps)", calls, warnings)
	}
}

// A truncation AFTER text was already emitted must not retry —
// re-streaming would duplicate output.
func TestTruncatedStreamAfterOutputNotRetried(t *testing.T) {
	p := &truncProvider{failAfterText: true}
	events := runRetryFixture(t, p)

	if countEvents(events, protocol.EventTurnFailed) != 1 {
		t.Fatalf("[L6] want turn_failed after mid-stream truncation")
	}
	if countEvents(events, protocol.EventTurnCompleted) != 0 {
		t.Fatalf("[L6] mid-stream truncation must not complete")
	}
	p.mu.Lock()
	calls := p.calls
	p.mu.Unlock()
	if calls != 1 {
		t.Fatalf("[L6] provider calls = %d, want 1 (no retry once output was emitted)", calls)
	}
	// The partial text appears exactly once — never duplicated.
	textEvents := 0
	for _, e := range events {
		if e.Type == protocol.EventAssistantTextDelta && e.Text == "partial " {
			textEvents++
		}
	}
	if textEvents != 1 {
		t.Fatalf("[L6] partial text emitted %d times, want exactly 1", textEvents)
	}
	t.Logf("[L6 mid-stream] calls=%d, no retry, partial emitted once", calls)
}

// Non-retryable provider errors fail fast — no backoff loop.
func TestNonRetryableStreamErrorFailsFast(t *testing.T) {
	failing := &failingProvider{err: fmt.Errorf("invalid api key: nope")}
	events := runRetryFixture(t, failing)
	if countEvents(events, protocol.EventTurnFailed) != 1 {
		t.Fatalf("[L6] want turn_failed")
	}
	failing.mu.Lock()
	calls := failing.calls
	failing.mu.Unlock()
	if calls != 1 {
		t.Fatalf("[L6] provider calls = %d, want 1 (fail fast)", calls)
	}
	t.Logf("[L6 fail-fast] calls=%d", calls)
}

type failingProvider struct {
	mu    sync.Mutex
	calls int
	err   error
}

func (p *failingProvider) Name() string { return "failing-fixture" }
func (p *failingProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	p.calls++
	p.mu.Unlock()
	deltas := make(chan provider.Delta)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		errs <- p.err
	}()
	return deltas, errs
}
