package engine

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/budget"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// L2 acceptance: a fixture run hitting each budget (wall time, cost,
// steps) produces the correct state (WRAP at 85%, stopped at 100%) and
// verdict, with gauges emitted along the way.

// budgetFixture streams one tool call per step after an optional delay,
// with fixed usage — so a run keeps looping until a budget fires.
type budgetFixture struct {
	delay time.Duration
	usage *provider.Usage
}

func (b *budgetFixture) Name() string { return "budget-fixture" }

func (b *budgetFixture) Stream(ctx context.Context, _ []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltas := make(chan provider.Delta, 4)
	errs := make(chan error, 1)
	send := func(d provider.Delta) bool {
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return false
		case deltas <- d:
			return true
		}
	}
	go func() {
		defer close(deltas)
		defer close(errs)
		if b.delay > 0 {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case <-time.After(b.delay):
			}
		}
		if !send(provider.Delta{Kind: provider.DeltaToolCall, ToolName: "read_file", ToolArgs: `{"path":"engine.go"}`, CallID: "budget_1"}) {
			return
		}
		if b.usage != nil {
			if !send(provider.Delta{Kind: provider.DeltaUsage, Usage: b.usage}) {
				return
			}
		}
		send(provider.Delta{Kind: provider.DeltaDone})
	}()
	return deltas, errs
}

type budgetScenario struct {
	name      string
	limits    budget.Limits
	model     string
	fixture   *budgetFixture
	wantType  protocol.EventType
	wantState budget.State
	wantHit   budget.Kind
}

func TestBudgetFixtureRunsHitEachBudget(t *testing.T) {
	// gpt-4o-mini: $0.15/M input -> 150k tokens = $0.0225 per step.
	miniUsage := &provider.Usage{PromptTokens: 150000, CompletionTokens: 0}

	scenarios := []budgetScenario{
		{
			name:      "wall_time wrap",
			limits:    budget.Limits{WallTime: 400 * time.Millisecond},
			model:     "gpt-4o-mini",
			fixture:   &budgetFixture{delay: 360 * time.Millisecond},
			wantType:  protocol.EventBudgetWrap,
			wantState: budget.StateWrap,
			wantHit:   budget.KindWallTime,
		},
		{
			name:      "wall_time stopped",
			limits:    budget.Limits{WallTime: 400 * time.Millisecond},
			model:     "gpt-4o-mini",
			fixture:   &budgetFixture{delay: 450 * time.Millisecond},
			wantType:  protocol.EventBudgetStopped,
			wantState: budget.StateStopped,
			wantHit:   budget.KindWallTime,
		},
		{
			name:      "cost wrap",
			limits:    budget.Limits{CostUSD: 0.05},
			model:     "gpt-4o-mini",
			fixture:   &budgetFixture{usage: miniUsage},
			wantType:  protocol.EventBudgetWrap,
			wantState: budget.StateWrap,
			wantHit:   budget.KindCost,
		},
		{
			name:      "cost stopped",
			limits:    budget.Limits{CostUSD: 0.04},
			model:     "gpt-4o-mini",
			fixture:   &budgetFixture{usage: miniUsage},
			wantType:  protocol.EventBudgetStopped,
			wantState: budget.StateStopped,
			wantHit:   budget.KindCost,
		},
		{
			name:      "steps wrap",
			limits:    budget.Limits{Steps: 10},
			model:     "gpt-4o-mini",
			fixture:   &budgetFixture{},
			wantType:  protocol.EventBudgetWrap,
			wantState: budget.StateWrap,
			wantHit:   budget.KindSteps,
		},
		{
			name:      "steps stopped",
			limits:    budget.Limits{Steps: 4},
			model:     "gpt-4o-mini",
			fixture:   &budgetFixture{},
			wantType:  protocol.EventBudgetStopped,
			wantState: budget.StateStopped,
			wantHit:   budget.KindSteps,
		},
	}

	for _, sc := range scenarios {
		t.Run(sc.name, func(t *testing.T) {
			eng, cmdChan, eventChan := NewEngine(100, sc.fixture, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
			eng.SetBudget(sc.limits, sc.model)
			eng.SetBudgetCheck(func(context.Context) (string, error) { return "fixture-verified", nil })
			go eng.Run()
			defer eng.Stop()

			cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "loop until budget fires"}

			var (
				gauges           int
				firstGaugeState  string
				terminal         *protocol.EngineEvent
				sawTurnCompleted bool
			)
			deadline := time.After(20 * time.Second)
			for terminal == nil {
				select {
				case evt, ok := <-eventChan:
					if !ok {
						t.Fatal("event channel closed")
					}
					switch evt.Type {
					case protocol.EventBudgetGauge:
						gauges++
						if gauges == 1 && evt.Budget != nil {
							firstGaugeState = evt.Budget.State
						}
						if evt.Budget == nil {
							t.Fatal("gauge event without budget payload")
						}
					case protocol.EventBudgetWrap, protocol.EventBudgetStopped:
						e := evt
						terminal = &e
					case protocol.EventTurnCompleted:
						sawTurnCompleted = true
					}
				case <-deadline:
					t.Fatalf("timeout waiting for budget event (gauges=%d)", gauges)
				}
			}
			time.Sleep(50 * time.Millisecond)

			if sawTurnCompleted {
				t.Error("budget must end the run before turn_completed")
			}
			if gauges == 0 {
				t.Error("no gauge events emitted")
			}
			if firstGaugeState != string(budget.StateOK) {
				t.Errorf("first gauge must be ok, got %q", firstGaugeState)
			}
			b := terminal.Budget
			if b == nil {
				t.Fatal("terminal budget event missing payload")
			}
			if terminal.Type != sc.wantType {
				t.Errorf("event type want %s got %s", sc.wantType, terminal.Type)
			}
			if b.State != string(sc.wantState) {
				t.Errorf("state want %s got %s", sc.wantState, b.State)
			}
			if b.Hit != string(sc.wantHit) {
				t.Errorf("hit want %s got %s", sc.wantHit, b.Hit)
			}
			if b.Verdict != "fixture-verified" {
				t.Errorf("verdict want fixture-verified got %q", b.Verdict)
			}
			// The run stopped at a safe point: the last completed step's
			// work is kept in the session (branch kept, L2/G1).
			var toolDone int
			for _, e := range eng.session.Events {
				if e.Type == protocol.EventToolCompleted {
					toolDone++
				}
			}
			if toolDone == 0 {
				t.Error("no completed step kept in session before budget stop")
			}
			// Gauges on the terminal event are populated.
			if !b.WallEnabled && !b.CostEnabled && !b.StepEnabled {
				t.Error("terminal event must report the enabled gauges")
			}
			t.Logf("[L2 %s] %s: state=%s hit=%s verdict=%q gauges=%d wall=%.2f cost=%.2f step=%.2f toolStepsKept=%d",
				sc.name, terminal.Type, b.State, b.Hit, b.Verdict, gauges, b.Wall, b.Cost, b.Step, toolDone)
		})
	}
}

// Budgets disabled (all zero) never fire.
func TestBudgetDisabledNeverFires(t *testing.T) {
	eng, cmdChan, eventChan := NewEngine(100, &budgetFixture{usage: &provider.Usage{PromptTokens: 999999}}, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	eng.SetBudget(budget.Limits{}, "gpt-4o-mini")
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "one step then done"}

	// The fixture always returns a tool call, so cap observation: we
	// just need no budget events. Cancel after a short window.
	var budgetEvents int
	done := false
	deadline := time.After(3 * time.Second)
	for !done {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			switch evt.Type {
			case protocol.EventBudgetGauge, protocol.EventBudgetWrap, protocol.EventBudgetStopped:
				budgetEvents++
			case protocol.EventTurnCompleted:
				done = true
			case protocol.EventToolCompleted:
				// keep looping; doom guard may end the run instead
			case protocol.EventWarning:
				if evt.Text != "" && strings.Contains(evt.Text, "doom loop") {
					done = true
				}
			}
		case <-deadline:
			done = true
		}
	}
	if budgetEvents != 0 {
		t.Errorf("disabled budgets emitted %d budget events", budgetEvents)
	}
}
