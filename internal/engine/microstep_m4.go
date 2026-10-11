package engine

import (
	"fmt"
	"strings"
)

// Micro-step plans (M4): each plan step carries an explicit done
// criterion and a check command. A step cannot be closed unless its
// check has passed. This compensates for small models declaring a step
// "done" without evidence: CloseStep refuses to mark a step complete
// until its check result is recorded as passed.

// MicroStep is one step in a micro-step plan.
type MicroStep struct {
	Title     string
	Criterion string // what "done" means for this step
	Check     string // the command/check that proves the criterion
	// CheckPassed records whether the step's check has passed. A step
	// with an empty Check is trivially satisfiable (no evidence
	// required); a step WITH a check must have CheckPassed=true before
	// it can close.
	CheckPassed bool
	Closed      bool
}

// MicroPlan is an ordered set of micro-steps.
type MicroPlan struct {
	Steps []MicroStep
}

// CanClose reports whether the step at index i may be closed (M4). A
// step with a check cannot close until the check has passed; a step
// with no check can close immediately.
func (p *MicroPlan) CanClose(i int) (bool, string) {
	if i < 0 || i >= len(p.Steps) {
		return false, "step index out of range"
	}
	s := p.Steps[i]
	if s.Closed {
		return true, ""
	}
	if s.Criterion == "" {
		return false, "step has no done criterion"
	}
	if s.Check != "" && !s.CheckPassed {
		return false, fmt.Sprintf("step %q cannot close: check %q has not passed", s.Title, s.Check)
	}
	return true, ""
}

// RecordCheck records the result of a step's check (M4). Passing=true
// marks the check satisfied so the step can close; passing=false
// leaves the step unclosable until the check is re-run and passes.
func (p *MicroPlan) RecordCheck(i int, passed bool) error {
	if i < 0 || i >= len(p.Steps) {
		return fmt.Errorf("step index out of range")
	}
	p.Steps[i].CheckPassed = passed
	return nil
}

// CloseStep closes the step if and only if it may close (M4). It
// returns an error (and does not close) when the check has not passed.
func (p *MicroPlan) CloseStep(i int) error {
	ok, why := p.CanClose(i)
	if !ok {
		return fmt.Errorf("cannot close step %d: %s", i, why)
	}
	p.Steps[i].Closed = true
	return nil
}

// AllClosed reports whether every step is closed.
func (p *MicroPlan) AllClosed() bool {
	for _, s := range p.Steps {
		if !s.Closed {
			return false
		}
	}
	return len(p.Steps) > 0
}

// NextOpen returns the index of the first open step, or -1.
func (p *MicroPlan) NextOpen() int {
	for i, s := range p.Steps {
		if !s.Closed {
			return i
		}
	}
	return -1
}

// Render formats the plan for display, marking closed steps and
// pending checks.
func (p *MicroPlan) Render() string {
	var b strings.Builder
	for i, s := range p.Steps {
		mark := " "
		if s.Closed {
			mark = "x"
		}
		fmt.Fprintf(&b, "[%s] %d. %s (done: %s)", mark, i+1, s.Title, s.Criterion)
		if s.Check != "" {
			status := "check pending"
			if s.CheckPassed {
				status = "check passed"
			}
			fmt.Fprintf(&b, " — %s: %s", s.Check, status)
		}
		b.WriteString("\n")
	}
	return b.String()
}
