package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
)

type PlanStep struct {
	Title  string `json:"title"`
	Status string `json:"status"` // "pending" | "in_progress" | "completed" | "cancelled"
}

type UpdatePlanTool struct {
	Base
	mu       sync.RWMutex
	planMode bool
	steps    []PlanStep
}

func NewUpdatePlanTool() *UpdatePlanTool {
	return &UpdatePlanTool{
		Base: Base{
			SchemaStr: `{"required":["steps"]}`,
		},
	}
}

func (t *UpdatePlanTool) Name() string { return "update_plan" }
func (t *UpdatePlanTool) Description() string {
	return "Update the execution plan with structured steps; enforces at most one in-progress step and rejects in read-only plan mode"
}

type updatePlanArgs struct {
	Steps []PlanStep `json:"steps"`
}

func (t *UpdatePlanTool) SetPlanMode(enabled bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.planMode = enabled
}

func (t *UpdatePlanTool) GetSteps() []PlanStep {
	t.mu.RLock()
	defer t.mu.RUnlock()
	res := make([]PlanStep, len(t.steps))
	copy(res, t.steps)
	return res
}

func (t *UpdatePlanTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	t.mu.RLock()
	inPlanMode := t.planMode
	t.mu.RUnlock()

	if inPlanMode {
		return ToolResult{
			Output:  "plan updates are rejected while in Plan Mode (exploration is read-only)",
			IsError: true,
		}, nil
	}

	var a updatePlanArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	if len(a.Steps) == 0 {
		return ToolResult{Output: "steps cannot be empty", IsError: true}, nil
	}

	inProgressCount := 0
	for i, s := range a.Steps {
		if strings.TrimSpace(s.Title) == "" {
			return ToolResult{Output: fmt.Sprintf("step %d missing title", i+1), IsError: true}, nil
		}
		status := strings.ToLower(strings.TrimSpace(s.Status))
		switch status {
		case "pending", "completed", "cancelled":
		case "in_progress":
			inProgressCount++
		default:
			return ToolResult{Output: fmt.Sprintf("invalid status %q for step %d: must be pending, in_progress, completed, or cancelled", s.Status, i+1), IsError: true}, nil
		}
	}

	if inProgressCount > 1 {
		return ToolResult{
			Output:  fmt.Sprintf("invalid plan: at most 1 step can be in_progress (found %d)", inProgressCount),
			IsError: true,
		}, nil
	}

	t.mu.Lock()
	t.steps = a.Steps
	t.mu.Unlock()

	var sb strings.Builder
	fmt.Fprintf(&sb, "Plan updated with %d steps:\n", len(a.Steps))
	for i, s := range a.Steps {
		icon := "[ ]"
		switch strings.ToLower(s.Status) {
		case "in_progress":
			icon = "[>]"
		case "completed":
			icon = "[x]"
		case "cancelled":
			icon = "[-]"
		}
		fmt.Fprintf(&sb, "%d. %s %s (%s)\n", i+1, icon, s.Title, s.Status)
	}

	return ToolResult{Output: strings.TrimSpace(sb.String())}, nil
}
