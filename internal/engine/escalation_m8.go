package engine

// Escalation (M8): when a small model fails the same step twice, one
// retry of that step may be routed to a configured larger model. This
// is OFF unless a secondary/escalation model is configured. The
// escalation is shown in the stream so the operator can see it happen;
// it never silently swaps the model.

// Escalator decides whether a repeated step failure should escalate to
// a larger model (M8). It is disabled unless an escalation model is
// configured.
type Escalator struct {
	// EscalationModel is the larger model to retry with. Empty means
	// escalation is OFF (the default).
	EscalationModel string
	// FailThreshold is how many times the same step must fail before
	// escalation is offered. Default 2.
	FailThreshold int
	// failCounts tracks consecutive failures per step key.
	failCounts map[string]int
	// escalated marks steps already escalated (one retry only).
	escalated map[string]bool
}

// NewEscalator builds an escalator. When model is empty, escalation is
// off and ShouldEscalate always returns false.
func NewEscalator(model string) *Escalator {
	return &Escalator{
		EscalationModel: model,
		FailThreshold:   2,
		failCounts:      map[string]int{},
		escalated:       map[string]bool{},
	}
}

// Enabled reports whether escalation is configured (M8: off unless
// configured).
func (e *Escalator) Enabled() bool { return e != nil && e.EscalationModel != "" }

// RecordFailure records that a step failed and reports whether this
// failure should trigger an escalation (M8). It returns true at most
// once per step (the single allowed retry). When escalation is off, it
// always returns false.
func (e *Escalator) RecordFailure(stepKey string) bool {
	if !e.Enabled() {
		return false
	}
	if e.escalated[stepKey] {
		return false // already used the one retry for this step
	}
	e.failCounts[stepKey]++
	threshold := e.FailThreshold
	if threshold < 1 {
		threshold = 2
	}
	if e.failCounts[stepKey] >= threshold {
		e.escalated[stepKey] = true
		return true
	}
	return false
}

// Reset clears a step's failure count (called when the step succeeds),
// so escalation is about *consecutive* failures of the same step.
func (e *Escalator) Reset(stepKey string) {
	delete(e.failCounts, stepKey)
}

// Model returns the escalation model to retry with.
func (e *Escalator) Model() string {
	if e == nil {
		return ""
	}
	return e.EscalationModel
}
