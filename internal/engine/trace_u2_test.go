package engine

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// U2: the run trace is built from real engine state. These tests
// prove each new emitter fires with real content — the plan from
// the update_plan tool's own steps, the step attribution from the
// in_progress step at emit time, and the diff from the file's real
// before/after bytes.

// passVerifier is defined in the engine's verifier tests; it lets a
// turn end as done so the event sequence completes.
func runTraceTurn(t *testing.T, prompt string, calls []provider.ToolCall) []protocol.EngineEvent {
	t.Helper()
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	prov.ToolScripts = map[string][]provider.ToolCall{prompt: calls}
	prov.Scripts[prompt] = "done"

	runner := &TurnRunner{
		verifier: passVerifier(),
		Provider: prov,
		Registry: reg,
		Context:  NewContextAssembler(),
		Perm:     permissions.NewGuard(permissions.ModeFullAccess),
	}
	var events []protocol.EngineEvent
	emit := func(evt protocol.EngineEvent) { events = append(events, evt) }
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := runner.Run(ctx, prompt, emit); err != nil {
		t.Fatalf("turn failed: %v", err)
	}
	return events
}

// U2/E1: the plan tool's effect is its own event, carrying the
// tool's real steps.
func TestPlanEventFromUpdatePlanTool(t *testing.T) {
	events := runTraceTurn(t, "plan the login work", []provider.ToolCall{
		{Tool: "update_plan", Args: `{"steps":[
			{"title":"Read the handler","status":"completed"},
			{"title":"Add the limiter","status":"in_progress"},
			{"title":"Run the auth tests","status":"pending"}]}`},
	})
	var plan *protocol.EngineEvent
	for i := range events {
		if events[i].Type == protocol.EventPlanUpdated {
			plan = &events[i]
			break
		}
	}
	if plan == nil {
		t.Fatal("update_plan must emit a plan_updated event")
	}
	if len(plan.Plan) != 3 {
		t.Fatalf("plan event carries %d steps, want 3", len(plan.Plan))
	}
	if plan.Plan[0].Description != "Read the handler" || plan.Plan[0].Status != "completed" {
		t.Fatalf("step 1 = %+v", plan.Plan[0])
	}
	if plan.Plan[1].Description != "Add the limiter" || plan.Plan[1].Status != "active" {
		t.Fatalf("in_progress must map to active: %+v", plan.Plan[1])
	}
	if plan.Plan[2].Status != "pending" {
		t.Fatalf("step 3 = %+v", plan.Plan[2])
	}
}

// U2/E1: a tool call is attributed to the plan step in progress at
// emit time; with no plan, no attribution is invented.
func TestToolEventsCarryActiveStep(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	if err := os.WriteFile(filepath.Join(dir, "token.go"), []byte("package auth\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	events := runTraceTurn(t, "harden the login handler", []provider.ToolCall{
		{Tool: "update_plan", Args: `{"steps":[
			{"title":"Read the handler","status":"completed"},
			{"title":"Add the limiter","status":"in_progress"}]}`},
		{Tool: "read_file", Args: `{"path":"token.go"}`},
	})

	var steps []string
	for _, ev := range events {
		switch ev.Type {
		case protocol.EventToolStarted:
			steps = append(steps, ev.Step)
		case protocol.EventToolCompleted, protocol.EventToolFailed:
			steps = append(steps, ev.Step)
		}
	}
	// The sequence is real: update_plan's own start fires before the
	// plan has an in_progress step (""), its completion fires after
	// ("Add the limiter" — the step the plan just activated), and the
	// read that follows is attributed to that same active step.
	want := []string{"", "Add the limiter", "Add the limiter", "Add the limiter"}
	if len(steps) != len(want) {
		t.Fatalf("tool events = %v, want %v", steps, want)
	}
	for i := range want {
		if steps[i] != want[i] {
			t.Fatalf("tool event %d step = %q, want %q (all: %v)", i, steps[i], want[i], steps)
		}
	}

	// With no plan at all, no step attribution appears anywhere.
	plain := runTraceTurn(t, "read the file", []provider.ToolCall{
		{Tool: "read_file", Args: `{"path":"token.go"}`},
	})
	for _, ev := range plain {
		if ev.Step != "" {
			t.Fatalf("step attribution %q invented without a plan", ev.Step)
		}
	}
}

// U2/E2: the patching tool's completion carries the real unified
// diff of the file's before/after bytes; a no-op edit carries none.
func TestEditCompletionCarriesRealDiff(t *testing.T) {
	dir := t.TempDir()
	t.Chdir(dir)
	original := "package auth\n\nfunc Check() bool {\n\treturn true\n}\n"
	if err := os.WriteFile(filepath.Join(dir, "limit.go"), []byte(original), 0o644); err != nil {
		t.Fatal(err)
	}
	events := runTraceTurn(t, "tighten the limiter", []provider.ToolCall{
		{Tool: "edit_file", Args: `{"path":"limit.go","old_string":"return true","new_string":"return allowed()"}`},
	})

	var diff string
	var sawDiffEvent bool
	for _, ev := range events {
		if ev.Type == protocol.EventToolCompleted && ev.ToolName == "edit_file" {
			diff = ev.Diff
			sawDiffEvent = true
		}
	}
	if !sawDiffEvent {
		t.Fatal("edit_file must emit a tool_completed event")
	}
	if diff == "" {
		t.Fatal("a real file change must attach the real diff")
	}
	for _, want := range []string{"--- a/limit.go", "+++ b/limit.go", "@@", "-\treturn true", "+\treturn allowed()"} {
		if !strings.Contains(diff, want) {
			t.Fatalf("diff missing %q:\n%s", want, diff)
		}
	}
	// The unchanged context lines are not invented as changes: the
	// hunk holds exactly one - line and one + line.
	if strings.Count(diff, "\n-\t") != 1 || strings.Count(diff, "\n+\t") != 1 {
		t.Fatalf("diff must hold exactly the changed lines:\n%s", diff)
	}

	// A no-op edit attaches no diff (no byte change).
	noop := runTraceTurn(t, "touch nothing", []provider.ToolCall{
		{Tool: "edit_file", Args: `{"path":"limit.go","old_string":"return allowed()","new_string":"return allowed()"}`},
	})
	for _, ev := range noop {
		if ev.Type == protocol.EventToolCompleted && ev.ToolName == "edit_file" && ev.Diff != "" {
			t.Fatalf("a no-op edit attached a diff:\n%s", ev.Diff)
		}
	}
}

// unifiedDiff: real hunks, real line counts, nothing for unchanged
// content.
func TestUnifiedDiff(t *testing.T) {
	if got := unifiedDiff("f.go", "same\n", "same\n"); got != "" {
		t.Fatalf("unchanged content must produce no diff, got %q", got)
	}

	got := unifiedDiff("f.go", "a\nb\nc\n", "a\nB\nc\n")
	for _, want := range []string{"--- a/f.go", "+++ b/f.go", "@@ -2,1 +2,1 @@", "-b", "+B"} {
		if !strings.Contains(got, want) {
			t.Fatalf("diff missing %q:\n%s", want, got)
		}
	}

	// A pure insertion keeps the context and counts only the new
	// lines on the + side.
	got = unifiedDiff("f.go", "a\nc\n", "a\nb\nc\n")
	if !strings.Contains(got, "@@ -1,0 +2,1 @@") {
		t.Fatalf("insertion hunk header wrong:\n%s", got)
	}
	if !strings.Contains(got, "+b") {
		t.Fatalf("insertion line missing:\n%s", got)
	}

	// A pure deletion keeps the context on the - side.
	got = unifiedDiff("f.go", "a\nb\nc\n", "a\nc\n")
	if !strings.Contains(got, "@@ -2,1 +1,0 @@") {
		t.Fatalf("deletion hunk header wrong:\n%s", got)
	}
	if !strings.Contains(got, "-b") {
		t.Fatalf("deletion line missing:\n%s", got)
	}

	// Two separated changes collapse into one honest hunk covering
	// the whole changed middle — the unchanged lines between the two
	// edits are restated on both sides (documented behavior; no
	// context trimming beyond the common prefix/suffix).
	got = unifiedDiff("f.go", "1\n2\n3\n4\n5\n6\n", "1\nX\n3\n4\n5\nY\n")
	if !strings.Contains(got, "@@ -2,5 +2,5 @@") {
		t.Fatalf("multi-change hunk header wrong:\n%s", got)
	}
	if !strings.Contains(got, "-2") || !strings.Contains(got, "+X") ||
		!strings.Contains(got, "-6") || !strings.Contains(got, "+Y") {
		t.Fatalf("multi-change lines missing:\n%s", got)
	}

	// A created file diffs from empty.
	got = unifiedDiff("new.go", "", "package new\n")
	if !strings.Contains(got, "@@ -0,0 +1,1 @@") || !strings.Contains(got, "+package new") {
		t.Fatalf("created-file diff wrong:\n%s", got)
	}
}
