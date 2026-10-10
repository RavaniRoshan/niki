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

// planStepTool records which plan the model chose so the fixture can
// assert the plan actually changed.
type planStepTool struct {
	tools.Base
	mu   sync.Mutex
	seen []string
}

func (t *planStepTool) Name() string        { return "plan_step" }
func (t *planStepTool) Description() string { return "record a plan choice" }
func (t *planStepTool) Run(_ context.Context, args json.RawMessage) (tools.ToolResult, error) {
	var a struct {
		Plan string `json:"plan"`
	}
	_ = json.Unmarshal(args, &a)
	t.mu.Lock()
	t.seen = append(t.seen, a.Plan)
	t.mu.Unlock()
	return tools.ToolResult{Output: "planned " + a.Plan}, nil
}
func (t *planStepTool) plans() []string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return append([]string(nil), t.seen...)
}

// planGateTool blocks the run until the test releases it, giving the
// test a deterministic moment to push a steer mid-run.
type planGateTool struct {
	tools.Base
	entered chan struct{}
	release chan struct{}
}

func (t *planGateTool) Name() string        { return "plan_gate" }
func (t *planGateTool) Description() string { return "block until the test releases" }
func (t *planGateTool) Run(ctx context.Context, _ json.RawMessage) (tools.ToolResult, error) {
	close(t.entered)
	select {
	case <-t.release:
		return tools.ToolResult{Output: "gate released"}, nil
	case <-ctx.Done():
		return tools.ToolResult{}, ctx.Err()
	}
}

// planProvider is a message-aware fixture: without a steer it plans A,
// after the steer text appears in a user message it plans B, and it
// ends with a completion claim.
type planProvider struct {
	mu       sync.Mutex
	requests [][]provider.Message
}

func (p *planProvider) Name() string { return "plan-fixture" }

func (p *planProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	p.requests = append(p.requests, append([]provider.Message(nil), messages...))
	n := len(p.requests)
	p.mu.Unlock()

	hasSteer := false
	for _, m := range messages {
		if m.Role == "user" && strings.Contains(m.Content, "prioritize plan-B") {
			hasSteer = true
			break
		}
	}

	var calls []provider.ToolCall
	var text string
	switch n {
	case 1:
		calls = []provider.ToolCall{{ID: "c1", Tool: "plan_gate", Args: `{}`}}
	case 2:
		plan := "A"
		if hasSteer {
			plan = "B"
		}
		calls = []provider.ToolCall{{ID: "c2", Tool: "plan_step", Args: fmt.Sprintf(`{"plan":%q}`, plan)}}
	default:
		text = "all plans done"
	}

	deltas := make(chan provider.Delta, 16)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		for _, c := range calls {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: c.ID, ToolName: c.Tool, ToolArgs: c.Args}:
			}
		}
		if text != "" {
			for _, w := range strings.Split(text, " ") {
				select {
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				case deltas <- provider.Delta{Kind: provider.DeltaText, Text: w + " "}:
				}
			}
		}
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 8, CompletionTokens: 4}}:
		}
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case deltas <- provider.Delta{Kind: provider.DeltaDone}:
		}
	}()
	return deltas, errs
}

// runSteerFixture runs one turn; steer is pushed after the gate tool
// blocks (mid-run). Returns events, provider, and the plans recorded.
func runSteerFixture(t *testing.T, steer string) ([]protocol.EngineEvent, *planProvider, []string, error) {
	t.Helper()
	prov := &planProvider{}
	step := &planStepTool{Base: tools.Base{SchemaStr: `{"type":"object","properties":{"plan":{"type":"string"}},"required":["plan"]}`}}
	gate := &planGateTool{
		Base:    tools.Base{SchemaStr: `{"type":"object","properties":{}}`},
		entered: make(chan struct{}),
		release: make(chan struct{}),
	}
	reg := tools.DefaultRegistry()
	reg.Register(step)
	reg.Register(gate)

	steerChan := make(chan string, 4)
	runner := &TurnRunner{
		verifier:     passVerifier(),
		Provider:     prov,
		Registry:     reg,
		Context:      NewContextAssembler(),
		Perm:         permissions.NewGuard(permissions.ModeFullAccess),
		SteerChannel: steerChan,
	}

	var mu sync.Mutex
	var events []protocol.EngineEvent
	emit := func(evt protocol.EngineEvent) {
		mu.Lock()
		events = append(events, evt)
		mu.Unlock()
	}

	done := make(chan error, 1)
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		done <- runner.Run(ctx, "initial prompt", emit)
	}()

	<-gate.entered
	if steer != "" {
		steerChan <- steer // pushed while the run is mid-step
	}
	close(gate.release)

	err := <-done
	mu.Lock()
	defer mu.Unlock()
	return events, prov, step.plans(), err
}

// TestSteerChangesNextPlan: a steer delivered mid-run lands at the next
// safe point, is shown as an event, and changes the model's next plan.
func TestSteerChangesNextPlan(t *testing.T) {
	events, prov, plans, err := runSteerFixture(t, "prioritize plan-B for the next step")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Shown as an event.
	var steerEvents []protocol.EngineEvent
	for _, e := range events {
		if e.Type == protocol.EventSteer {
			steerEvents = append(steerEvents, e)
		}
	}
	if len(steerEvents) != 1 {
		t.Fatalf("[L4] want 1 steer event, got %d (events: %d)", len(steerEvents), len(events))
	}
	if !strings.Contains(steerEvents[0].Text, "prioritize plan-B") {
		t.Fatalf("[L4] steer event text = %q", steerEvents[0].Text)
	}

	// The next plan changed: the plan step ran with B, never A.
	if len(plans) != 1 || plans[0] != "B" {
		t.Fatalf("[L4] plan choices = %v, want [B]", plans)
	}

	// The steer is in the request that produced the changed plan.
	prov.mu.Lock()
	requests := prov.requests
	prov.mu.Unlock()
	if len(requests) < 2 {
		t.Fatalf("[L4] provider requests = %d, want >= 2", len(requests))
	}
	found := false
	for _, m := range requests[1] {
		if m.Role == "user" && strings.Contains(m.Content, "prioritize plan-B") {
			found = true
		}
	}
	if !found {
		t.Fatalf("[L4] steer missing from the provider request that chose the new plan")
	}

	// Run still completes normally through the verifier gate.
	completed := false
	for _, e := range events {
		if e.Type == protocol.EventTurnCompleted {
			completed = true
		}
		if e.Type == protocol.EventTurnNotVerified {
			t.Fatalf("[L4] run ended not-verified: %s", e.Text)
		}
	}
	if !completed {
		t.Fatalf("[L4] turn did not complete")
	}
	t.Logf("[L4 steer run] event=%s plans=%v requests=%d turn_completed", steerEvents[0].Text, plans, len(requests))
}

// TestSteerFixtureControlWithoutSteer: without a steer the same fixture
// plans A — proving the steer (not the fixture) changed the plan.
func TestSteerFixtureControlWithoutSteer(t *testing.T) {
	events, _, plans, err := runSteerFixture(t, "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	for _, e := range events {
		if e.Type == protocol.EventSteer {
			t.Fatalf("[L4 control] unexpected steer event without a steer")
		}
	}
	if len(plans) != 1 || plans[0] != "A" {
		t.Fatalf("[L4 control] plan choices = %v, want [A]", plans)
	}
	t.Logf("[L4 control] plans=%v (no steer)", plans)
}
