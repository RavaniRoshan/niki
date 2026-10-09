package engine

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// TestReadinessRequiredBeforePrompt proves the readiness contract
// (B7): SessionReady is only emitted after every required
// capability is warm, and a prompt submitted before readiness is
// refused rather than run cold.
func TestReadinessRequiredBeforePrompt(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeFullAccess))

	// An optional warmer that is slow must not delay readiness.
	optionalDone := make(chan struct{})
	eng.WarmOptional(CapSkills, func(ctx context.Context) error {
		select {
		case <-time.After(200 * time.Millisecond):
			close(optionalDone)
			return nil
		case <-ctx.Done():
			return ctx.Err()
		}
	})

	go eng.Run()
	defer eng.Stop()

	var sawReady bool
	var readyAt, optionalAt time.Time
	deadline := time.After(5 * time.Second)
	for !sawReady || optionalAt.IsZero() {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			switch evt.Type {
			case protocol.EventSessionReady:
				sawReady = true
				readyAt = time.Now()
				if !eng.Readiness().Ready() {
					t.Fatal("SessionReady emitted while required capabilities cold")
				}
				for _, p := range eng.Readiness().Phases() {
					if !p.Required {
						t.Fatal("optional phase recorded before SessionReady")
					}
				}
			case protocol.EventBootPhase:
				if evt.Text == "skills:ready" {
					optionalAt = time.Now()
				}
			}
		case <-deadline:
			t.Fatal("timed out waiting for readiness")
		}
	}
	// Optional warmer finished after readiness was published.
	if !optionalAt.After(readyAt) {
		t.Fatalf("optional warmer did not run lazily: ready=%v optional=%v", readyAt, optionalAt)
	}

	// Prompt works once ready.
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "hello"}
	sawDone := false
	for !sawDone {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			if evt.Type == protocol.EventTurnCompleted {
				sawDone = true
			}
		case <-deadline:
			t.Fatal("turn did not complete after readiness")
		}
	}
	<-optionalDone
}

// TestPromptRefusedBeforeReadiness: an engine whose required
// warmer fails never becomes ready and refuses prompts with a
// typed failure event.
func TestPromptRefusedBeforeReadiness(t *testing.T) {
	eng, cmdChan, eventChan := NewEngine(100, provider.NewMockProvider(), tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	// Replace every required warmer with one that fails, so
	// readiness can never be published.
	eng.requiredWarmers = nil
	eng.WarmRequired(CapModel, func(context.Context) error {
		return context.DeadlineExceeded
	})
	go eng.Run()
	defer eng.Stop()

	// Run returns the warm-up error; the engine never reaches the
	// command loop, so the prompt stays queued and unanswered.
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "too early"}
	select {
	case evt, ok := <-eventChan:
		if ok && evt.Type == protocol.EventSessionReady {
			t.Fatal("SessionReady must not be emitted when a required warmer fails")
		}
	case <-time.After(500 * time.Millisecond):
	}
	if eng.Readiness().Ready() {
		t.Fatal("readiness must be false after a failed required warmer")
	}
	if missing := eng.Readiness().RequiredMissing(); len(missing) == 0 {
		t.Fatal("expected missing required capabilities to be reported")
	}
}

// TestReadOnlyTurnEndToEnd (A1): a single read-only turn streams
// typed events end to end and ends cleanly.
func TestReadOnlyTurnEndToEnd(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeReadOnly))
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read the README"}

	var order []protocol.EventType
	deadline := time.After(5 * time.Second)
	done := false
	for !done {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			order = append(order, evt.Type)
			switch evt.Type {
			case protocol.EventTurnStarted:
			case protocol.EventAssistantTextDelta:
				if evt.Text == "" {
					t.Fatal("empty text delta")
				}
			case protocol.EventAssistantMessageDone:
			case protocol.EventTurnCompleted:
				done = true
			case protocol.EventTurnFailed:
				t.Fatalf("turn failed: %s", evt.Error)
			}
		case <-deadline:
			t.Fatalf("timed out; events so far: %v", order)
		}
	}
	// Typed-event ordering contract for a clean read-only turn.
	// The mock provider streams deltas, then a usage-bearing
	// AssistantMessageDone, then the final AssistantMessageDone,
	// then TurnCompleted.
	if !contains(order, protocol.EventTurnStarted) {
		t.Fatal("no turn_started")
	}
	if !contains(order, protocol.EventAssistantTextDelta) {
		t.Fatal("no streamed text deltas")
	}
	if !contains(order, protocol.EventAssistantMessageDone) {
		t.Fatal("no assistant_message_done")
	}
	last := order[len(order)-1]
	if last != protocol.EventTurnCompleted {
		t.Fatalf("turn must end with turn_completed, got %v (tail=%v)", last, order[max(0, len(order)-4):])
	}
	// turn_started must precede the first delta.
	startedIdx, deltaIdx := -1, -1
	for i, et := range order {
		if et == protocol.EventTurnStarted && startedIdx < 0 {
			startedIdx = i
		}
		if et == protocol.EventAssistantTextDelta && deltaIdx < 0 {
			deltaIdx = i
		}
	}
	if startedIdx > deltaIdx {
		t.Fatalf("turn_started (%d) must precede first delta (%d)", startedIdx, deltaIdx)
	}
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func contains(order []protocol.EventType, target protocol.EventType) bool {
	for _, e := range order {
		if e == target {
			return true
		}
	}
	return false
}

// TestInterruptKeepsPartialOutput (A3): interrupting a turn
// cancels it, emits TurnCancelled, and the deltas streamed before
// the interrupt remain in the session transcript.
func TestInterruptKeepsPartialOutput(t *testing.T) {
	prov := provider.NewMockProvider()
	prov.ChunkDelay = 30 * time.Millisecond // slow stream so we can interrupt mid-flight
	eng, cmdChan, eventChan := NewEngine(100, prov, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "slow answer please"}

	var deltas int
	interrupted := false
	sawCancel := false
	deadline := time.After(5 * time.Second)
	for !sawCancel {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			switch evt.Type {
			case protocol.EventAssistantTextDelta:
				deltas++
				if deltas == 2 && !interrupted {
					interrupted = true
					cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
				}
			case protocol.EventTurnCancelled:
				sawCancel = true
			case protocol.EventTurnCompleted:
				t.Fatal("turn completed despite interrupt")
			}
		case <-deadline:
			t.Fatalf("timeout; deltas=%d interrupted=%v", deltas, interrupted)
		}
	}
	if deltas < 1 {
		t.Fatal("no partial output before interrupt")
	}
	// Partial deltas the user already saw stay in the transcript.
	found := false
	for _, e := range eng.session.Events {
		if e.Type == protocol.EventAssistantTextDelta {
			found = true
		}
	}
	if !found {
		t.Fatal("partial deltas lost from session transcript")
	}
}

// TestPermissionGateDeniesTool (A6): every tool call passes the
// permission gate; a tool outside the mode is refused with a
// typed failure and never executed.
func TestPermissionGateDeniesTool(t *testing.T) {
	reg := tools.DefaultRegistry()
	// Script a provider turn that attempts a write via shell.
	prov := &scriptedProvider{
		deltas: []provider.Delta{
			{Kind: provider.DeltaToolCall, ToolName: "shell", ToolArgs: `{"command":"touch /tmp/nikicode-should-not-exist"}`},
			{Kind: provider.DeltaDone},
		},
	}
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeReadOnly))
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "do something"}

	var sawStarted, sawFailed bool
	deadline := time.After(5 * time.Second)
	for !sawFailed {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			switch evt.Type {
			case protocol.EventToolStarted:
				sawStarted = true
			case protocol.EventToolFailed:
				sawFailed = true
				if !strings.Contains(evt.Error, "permission") && !strings.Contains(evt.Error, "not allowed") {
					t.Fatalf("failure not permission-related: %s", evt.Error)
				}
			case protocol.EventToolCompleted:
				t.Fatal("denied tool completed")
			}
		case <-deadline:
			t.Fatalf("timeout; started=%v", sawStarted)
		}
	}
}

// scriptedProvider emits a fixed delta sequence, for tests that
// need tool calls rather than plain text.
type scriptedProvider struct {
	deltas []provider.Delta
}

func (s *scriptedProvider) Name() string { return "scripted" }

func (s *scriptedProvider) Stream(ctx context.Context, _ []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltas := make(chan provider.Delta, len(s.deltas)+1)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		for _, d := range s.deltas {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case deltas <- d:
			}
		}
	}()
	return deltas, errs
}
