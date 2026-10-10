package engine

import (
	"context"
	"encoding/json"
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

// sideEffectTool performs a (fake) side effect and can be scripted to
// fail; it counts how many times it actually ran.
type sideEffectTool struct {
	tools.Base
	mu      sync.Mutex
	runs    int
	failAll bool
}

func (t *sideEffectTool) Name() string        { return "side_effect_tool" }
func (t *sideEffectTool) Description() string { return "irreversible side effect fixture" }
func (t *sideEffectTool) Run(_ context.Context, _ json.RawMessage) (tools.ToolResult, error) {
	t.mu.Lock()
	t.runs++
	fail := t.failAll
	t.mu.Unlock()
	if fail {
		return tools.ToolResult{}, fmt.Errorf("side effect failed (as scripted)")
	}
	return tools.ToolResult{Output: "side effect applied"}, nil
}
func (t *sideEffectTool) count() int {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.runs
}

// approvalGate is the "fresh approval" hook: it counts every dispatch
// gate evaluation and can deny.
type approvalGate struct {
	mu    sync.Mutex
	fires int
	deny  bool
}

// approvalProvider asks for the side-effect tool `wants` times, then
// claims done. Every ask happens only after the previous tool result
// came back — so each dispatch must have passed a fresh gate.
type approvalProvider struct {
	mu       sync.Mutex
	calls    int
	emitted  int
	wants    int
	askTimes []time.Time
}

func (p *approvalProvider) Name() string { return "approval-fixture" }

func (p *approvalProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	p.calls++
	emitTool := p.emitted < p.wants
	if emitTool {
		p.emitted++
		p.askTimes = append(p.askTimes, time.Now())
	}
	p.mu.Unlock()

	deltas := make(chan provider.Delta, 8)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		if emitTool {
			deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: "se1", ToolName: "side_effect_tool", ToolArgs: `{}`}
		} else {
			for _, w := range strings.Split("gate work done", " ") {
				select {
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				case deltas <- provider.Delta{Kind: provider.DeltaText, Text: w + " "}:
				}
			}
		}
		deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 5, CompletionTokens: 5}}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

// runApprovalScenario drives one engine run over the fixture.
func runApprovalScenario(t *testing.T, wants int, deny bool, failTool bool) ([]protocol.EngineEvent, *sideEffectTool, *approvalGate, int) {
	t.Helper()
	tool := &sideEffectTool{Base: tools.Base{SchemaStr: `{"type":"object","properties":{}}`}, failAll: failTool}
	gate := &approvalGate{deny: deny}

	reg := tools.DefaultRegistry()
	reg.Register(tool)

	prov := &approvalProvider{wants: wants}
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeWorkspaceWrite))
	eng.SetVerifier(passVerifier(), 0)
	eng.runner.Hooks = NewHookRunner()
	eng.runner.Hooks.OnBlocking(HookPreToolUse, func(ctx HookContext) error {
		gate.mu.Lock()
		gate.fires++
		deny := gate.deny
		gate.mu.Unlock()
		if deny {
			return fmt.Errorf("approval gate denied")
		}
		return nil
	})
	go func() { _ = eng.Run() }()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "apply the side effect"}
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
				return events, tool, gate, prov.calls
			}
		case <-deadline:
			t.Fatal("[L6] approval fixture timed out")
		}
	}
}

// A denied side-effect tool never runs, and the engine never
// re-dispatches it: every attempt is a fresh model request through a
// fresh approval-gate evaluation.
func TestDeniedSideEffectNeverRunsAndIsNeverAutoRetried(t *testing.T) {
	events, tool, gate, calls := runApprovalScenario(t, 2, true, false)

	if tool.count() != 0 {
		t.Fatalf("[L6] denied tool ran %d times, want 0", tool.count())
	}
	gate.mu.Lock()
	fires := gate.fires
	gate.mu.Unlock()
	if fires < 2 {
		t.Fatalf("[L6] approval gate fired %d times, want >= 2 (one per attempt)", fires)
	}
	// Each dispatch followed its own model response: no two
	// tool_started events without a model turn in between.
	toolStarted, modelTurns := 0, 0
	prevToolStarted := false
	for _, e := range events {
		switch e.Type {
		case protocol.EventAssistantMessageDone:
			modelTurns++
			prevToolStarted = false
		case protocol.EventToolStarted:
			toolStarted++
			if prevToolStarted {
				t.Fatalf("[L6] engine re-dispatched a tool without a fresh model request (auto-retry)")
			}
			prevToolStarted = true
		}
	}
	if countEvents(events, protocol.EventTurnCompleted) != 1 {
		t.Fatalf("[L6] want completed fixture run, got %d completed", countEvents(events, protocol.EventTurnCompleted))
	}
	t.Logf("[L6 denied] toolRuns=0 gateFires=%d toolStarted=%d modelTurns=%d providerCalls=%d",
		fires, toolStarted, modelTurns, calls)
}

// An approved side effect runs exactly once — the engine does not
// replay it after success.
func TestApprovedSideEffectRunsExactlyOnce(t *testing.T) {
	events, tool, gate, _ := runApprovalScenario(t, 1, false, false)
	gate.mu.Lock()
	fires := gate.fires
	gate.mu.Unlock()
	if tool.count() != 1 {
		t.Fatalf("[L6] approved tool ran %d times, want exactly 1", tool.count())
	}
	if fires != 1 {
		t.Fatalf("[L6] gate fired %d times, want 1 (fresh approval per dispatch)", fires)
	}
	if countEvents(events, protocol.EventTurnCompleted) != 1 {
		t.Fatalf("[L6] want completed run")
	}
	t.Logf("[L6 approved] toolRuns=1 gateFires=1 completed")
}

// A side effect that FAILS is not replayed by the engine either —
// each attempt is a fresh model request through the gate (3 attempts
// = the doom-loop ceiling).
func TestFailedSideEffectNotReplayedWithoutFreshRequest(t *testing.T) {
	events, tool, gate, _ := runApprovalScenario(t, 3, false, true)
	gate.mu.Lock()
	fires := gate.fires
	gate.mu.Unlock()
	if tool.count() != 3 {
		t.Fatalf("[L6] failing tool ran %d times, want 3 (one per fresh request)", tool.count())
	}
	if fires != tool.count() {
		t.Fatalf("[L6] gate fires %d != runs %d — every dispatch must pass the gate fresh", fires, tool.count())
	}
	// Strict alternation: model response -> dispatch, never dispatch -> dispatch.
	prevToolStarted := false
	for _, e := range events {
		switch e.Type {
		case protocol.EventAssistantMessageDone:
			prevToolStarted = false
		case protocol.EventToolStarted:
			if prevToolStarted {
				t.Fatalf("[L6] consecutive tool dispatch without a fresh model request")
			}
			prevToolStarted = true
		}
	}
	t.Logf("[L6 failed-side-effect] toolRuns=3 gateFires=3 (no engine auto-retry, doom loop ends the run)")
}
