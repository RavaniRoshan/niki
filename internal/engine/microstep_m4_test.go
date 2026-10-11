package engine

import (
	"strings"
	"testing"
)

// M4: micro-step plans — each step has a done criterion and a check;
// a step cannot close without its check passing.
func TestMicroStepCannotCloseWithoutCheck(t *testing.T) {
	p := &MicroPlan{Steps: []MicroStep{
		{Title: "Add the parser", Criterion: "parser handles all inputs", Check: "go test ./parser"},
		{Title: "Wire it up", Criterion: "engine calls the parser", Check: "go test ./engine"},
		{Title: "Trivial step", Criterion: "no check needed"}, // no check
	}}

	// A step with a check cannot close before the check passes.
	if err := p.CloseStep(0); err == nil {
		t.Fatalf("[M4] step with an unpassed check must not close")
	} else {
		t.Logf("[M4] close refused before check: %v", err)
	}

	// A failing check still does not allow closing.
	if err := p.RecordCheck(0, false); err != nil {
		t.Fatal(err)
	}
	if err := p.CloseStep(0); err == nil {
		t.Fatalf("[M4] step with a FAILED check must not close")
	}
	t.Logf("[M4] failed check keeps the step open")

	// Once the check passes, the step closes.
	if err := p.RecordCheck(0, true); err != nil {
		t.Fatal(err)
	}
	if err := p.CloseStep(0); err != nil {
		t.Fatalf("[M4] step should close after its check passes: %v", err)
	}
	if !p.Steps[0].Closed {
		t.Fatalf("[M4] step not marked closed")
	}
	t.Logf("[M4] step closes only after its check passes")

	// A step with no check can close immediately.
	if err := p.CloseStep(2); err != nil {
		t.Fatalf("[M4] checkless step should close: %v", err)
	}
	t.Logf("[M4] checkless step closes immediately")

	// A step with no done criterion cannot close at all.
	bad := &MicroPlan{Steps: []MicroStep{{Title: "vague"}}}
	if err := bad.CloseStep(0); err == nil {
		t.Fatalf("[M4] step without a done criterion must not close")
	}
	t.Logf("[M4] step without a done criterion refused")

	// Render marks closed vs pending-check steps.
	out := p.Render()
	if !strings.Contains(out, "[x] 1. Add the parser") {
		t.Fatalf("[M4] render missing closed marker:\n%s", out)
	}
	if !strings.Contains(out, "check pending") {
		t.Fatalf("[M4] render missing pending-check marker:\n%s", out)
	}
	t.Logf("[M4] plan renders closed + pending-check state")
}
