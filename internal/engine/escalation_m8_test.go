package engine

import "testing"

// M8: escalation — when a small model fails the same step twice, one
// retry may use a configured larger model, shown in the stream. Off
// unless configured.
func TestEscalationOnRepeatedStepFailure(t *testing.T) {
	// Off by default: no escalation model -> never escalates.
	off := NewEscalator("")
	if off.Enabled() {
		t.Fatalf("[M8] escalation must be off without a configured model")
	}
	for i := 0; i < 5; i++ {
		if off.RecordFailure("step-a") {
			t.Fatalf("[M8] disabled escalator must never escalate")
		}
	}
	t.Logf("[M8] escalation off by default (no model configured)")

	// Configured: escalate on the 2nd consecutive failure of a step.
	esc := NewEscalator("gpt-4o")
	if !esc.Enabled() {
		t.Fatalf("[M8] configured escalator must be enabled")
	}
	if esc.RecordFailure("step-a") {
		t.Fatalf("[M8] first failure must not escalate")
	}
	if !esc.RecordFailure("step-a") {
		t.Fatalf("[M8] second consecutive failure must escalate")
	}
	t.Logf("[M8] 2nd consecutive failure of step-a -> escalate to %q", esc.Model())

	// Only ONE retry per step: further failures do not re-escalate.
	if esc.RecordFailure("step-a") {
		t.Fatalf("[M8] step-a must not escalate twice")
	}
	t.Logf("[M8] one retry per step (no repeat escalation)")

	// A different step gets its own budget.
	if esc.RecordFailure("step-b") {
		t.Fatalf("[M8] step-b first failure must not escalate")
	}
	if !esc.RecordFailure("step-b") {
		t.Fatalf("[M8] step-b second failure must escalate")
	}
	t.Logf("[M8] per-step escalation budget (step-b escalates independently)")

	// Success resets the consecutive-failure count.
	esc.Reset("step-b")
	if esc.RecordFailure("step-b") {
		t.Fatalf("[M8] after reset, first failure must not escalate")
	}
	t.Logf("[M8] success resets the consecutive-failure count")
}
