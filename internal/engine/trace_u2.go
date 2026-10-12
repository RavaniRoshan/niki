package engine

import (
	"fmt"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// U2 run-trace emitters. The trace is built from real engine state
// only: the plan comes from the update_plan tool's own steps, the
// step attribution names the in_progress step at emit time, and the
// diff is computed from the file's pre/post images. Nothing here is
// invented for the frontend.

// planSteps reads the current plan from the registry's update_plan
// tool (the plan's single source of truth). Nil-safe: an absent
// tool means no plan, so no event and no headers.
func (t *TurnRunner) planSteps() []protocol.PlanStep {
	if t.Registry == nil {
		return nil
	}
	tool, ok := t.Registry.Get("update_plan")
	if !ok {
		return nil
	}
	upt, ok := tool.(*tools.UpdatePlanTool)
	if !ok {
		return nil
	}
	steps := upt.GetSteps()
	if len(steps) == 0 {
		return nil
	}
	out := make([]protocol.PlanStep, 0, len(steps))
	for _, s := range steps {
		out = append(out, protocol.PlanStep{
			Description: s.Title,
			Status:      planStepStatus(s.Status),
		})
	}
	return out
}

// planStepStatus maps the tool's status vocabulary to the wire's:
// pending | in_progress | completed | cancelled become
// pending | active | completed | cancelled.
func planStepStatus(status string) string {
	switch status {
	case "in_progress":
		return "active"
	default:
		return status
	}
}

// activeStepLabel names the plan step currently in progress — the
// step a tool call is attributed to. Empty when the plan has no
// in_progress step; the frontend then renders no step header.
func (t *TurnRunner) activeStepLabel() string {
	for _, s := range t.planSteps() {
		if planStepStatus(s.Status) == "active" {
			return s.Description
		}
	}
	return ""
}

// emitPlanUpdate sends the plan as a real event when the model just
// called update_plan successfully. Called after the tool's own
// completion row is emitted, so the trace reads: the plan tool
// call, then the plan it produced.
func (t *TurnRunner) emitPlanUpdate(turnID protocol.TurnId, emit func(protocol.EngineEvent)) {
	steps := t.planSteps()
	if len(steps) == 0 {
		return
	}
	emit(protocol.EngineEvent{
		Type:      protocol.EventPlanUpdated,
		Timestamp: time.Now(),
		TurnID:    turnID,
		Plan:      steps,
	})
}

// unifiedDiff renders a real unified diff between two file states:
// common prefix and suffix lines are trimmed and the changed middle
// is one hunk with correct line counts. A single hunk covering all
// changes is the honest minimal form (documented; no interleaving
// multi-hunk splitting). Unchanged content produces no diff.
func unifiedDiff(path, before, after string) string {
	if before == after {
		return ""
	}
	a := strings.Split(before, "\n")
	b := strings.Split(after, "\n")

	prefix := 0
	for prefix < len(a) && prefix < len(b) && a[prefix] == b[prefix] {
		prefix++
	}
	suffix := 0
	for suffix < len(a)-prefix && suffix < len(b)-prefix &&
		a[len(a)-1-suffix] == b[len(b)-1-suffix] {
		suffix++
	}

	oldMid := a[prefix : len(a)-suffix]
	newMid := b[prefix : len(b)-suffix]

	var sb strings.Builder
	fmt.Fprintf(&sb, "--- a/%s\n", path)
	fmt.Fprintf(&sb, "+++ b/%s\n", path)
	// hunk header: @@ -oldStart,oldCount +newStart,newCount @@
	// Following git's convention: a range with count 0 starts at
	// the prefix line (the line the change is anchored to), a
	// non-empty range starts after it.
	oldCount, newCount := len(oldMid), len(newMid)
	oldStart, newStart := prefix+1, prefix+1
	if oldCount == 0 {
		oldStart = prefix
	}
	if newCount == 0 {
		newStart = prefix
	}
	fmt.Fprintf(&sb, "@@ -%d,%d +%d,%d @@\n", oldStart, oldCount, newStart, newCount)
	for _, l := range oldMid {
		sb.WriteString("-" + l + "\n")
	}
	for _, l := range newMid {
		sb.WriteString("+" + l + "\n")
	}
	return strings.TrimRight(sb.String(), "\n")
}
