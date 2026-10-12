package tui

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// U2: the run trace. Each element is proven absent before its event
// fires and present after: step headers, the diff block, the
// reasoning line, and the activity line (re-asserted here in trace
// context). The request line and result lines are re-asserted
// against the same fixture that drives the goldens.

// fullRunTrace is the recorded full run the frame6 fixture replays:
// plan, attributed tools, a real edit diff, a reasoning line, and
// the failing test. Every row corresponds to a real engine event
// shape — the same replay drives the trace tests, the frame6
// goldens, and the review dumps.
func fullRunTrace() []protocol.EngineEvent {
	return []protocol.EngineEvent{
		{Type: protocol.EventTurnStarted, TurnID: "t1", Text: "harden POST /login: reject expired tokens and rate-limit failed logins",
			Contract: []protocol.ContractReq{
				{ID: "R1", State: "queued"}, {ID: "R2", State: "queued"}, {ID: "R3", State: "queued"},
				{ID: "R4", State: "queued"}, {ID: "R5", State: "queued"},
			}},
		{Type: protocol.EventAssistantTextDelta, TurnID: "t1",
			Text:  "Reading the handler and its tests first, then adding a limiter for failed logins.",
			Usage: &protocol.Usage{PromptTokens: 5120, TotalTokens: 5120}},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c0", ToolName: "update_plan", Target: "3 steps"},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c0", ToolName: "update_plan", Result: "3 steps", Step: "Read the handler and its tests"},
		{Type: protocol.EventPlanUpdated, TurnID: "t1", Plan: []protocol.PlanStep{
			{Description: "Read the handler and its tests", Status: "completed"},
			{Description: "Add the limiter", Status: "active"},
			{Description: "run go test ./internal/auth", Status: "pending"},
		}},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c1", ToolName: "read_file", Target: "internal/auth/token.go", Step: "Read the handler and its tests"},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c1", ToolName: "read_file", Result: "112 lines", Step: "Read the handler and its tests"},
		{Type: protocol.EventReasoningDelta, TurnID: "t1", Text: "The token check must compare expiry against the clock, and the limiter counts per client IP."},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c2", ToolName: "edit_file", Target: "internal/auth/limit.go", Step: "Add the limiter"},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c2", ToolName: "edit_file", Result: "+62 −4", Step: "Add the limiter",
			Diff: "--- a/internal/auth/limit.go\n+++ b/internal/auth/limit.go\n@@ -4,1 +4,1 @@\n-\treturn true\n+\treturn allowed()"},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c3", ToolName: "shell", Target: "go test ./internal/auth", Step: "run go test ./internal/auth"},
		{Type: protocol.EventToolFailed, TurnID: "t1", CallID: "c3", ToolName: "shell",
			Error: "exit status 1", Result: "exit 1 · 2 failing",
			Excerpt: "--- FAIL: TestRefreshKeepsSession (0.00s)\n" +
				"token_test.go:81: session id changed after refresh",
			Step: "run go test ./internal/auth"},
	}
}

// workbenchFullRun submits the request through the real composer
// path and replays the full run fixture in the workbench layout.
func workbenchFullRun(t *testing.T, w, h int) AppModel {
	t.Helper()
	t.Setenv("TERM", "xterm-256color")
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.SetLayout("workbench")
	um, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	m = um.(AppModel)
	m.state.Directory = "/home/dev/projects/api"
	m.state.GitBranch = "main"
	m.state.Version = "0.1.0"
	m.state.ModelName = "claude-sonnet"
	m.composer.Input.SetValue("harden POST /login: reject expired tokens and rate-limit failed logins")
	ue, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = ue.(AppModel)
	for _, evt := range fullRunTrace() {
		e2, _ := m.Update(engineEventMsg(evt))
		m = e2.(AppModel)
	}
	m.renderLive()
	return m
}

// traceText renders the full trace content (what the viewport
// renders, unclipped) so element assertions are independent of
// scroll position.
func traceText(m AppModel) string {
	return Downsample(m.classicTranscript(m.history.Cells))
}

// U2: the request line, the tool rows and the result lines render
// from the real events.
func TestTraceRequestAndResultLines(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	if strings.Contains(traceText(m), "harden POST /login") {
		t.Fatal("request line present before the submit")
	}

	m = workbenchFullRun(t, 100, 27)
	got := traceText(m)
	for _, want := range []string{
		"harden POST /login",                  // the request line
		"internal/auth/token.go", "112 lines", // tool row + result
		"go test ./internal/auth", "exit 1",   // failed row + result
		"token_test.go:81",                    // failure excerpt
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("trace element %q missing:\n%s", want, got)
		}
	}
	// Same elements reach the rendered view at 100 columns.
	view := workbenchFrame(m)
	if !strings.Contains(view, "112 lines") || !strings.Contains(view, "exit 1") {
		t.Fatalf("trace rows missing from the view:\n%s", view)
	}
}

// U2/E1: step headers group the tool rows; they are absent before
// any attributed tool runs, numbered in trace order, and a call
// with no step attribution renders without a header.
func TestTraceStepHeaders(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	if strings.Contains(traceText(m), "step 1") {
		t.Fatal("step header present before any attributed tool")
	}

	m = workbenchFullRun(t, 100, 27)
	got := traceText(m)
	for _, want := range []string{"step 1", "Read the handler and its tests", "step 2", "Add the limiter", "step 3", "run go test ./internal/auth"} {
		if !strings.Contains(got, want) {
			t.Fatalf("step header %q missing:\n%s", want, got)
		}
	}
	// The unattributed update_plan row precedes step 1: no grouping
	// is invented for it.
	if strings.Index(got, "update_plan") > strings.Index(got, "step 1") {
		t.Fatal("the unattributed plan call must precede the first step header")
	}
	// Header order follows trace order.
	if strings.Index(got, "step 1") > strings.Index(got, "step 2") ||
		strings.Index(got, "step 2") > strings.Index(got, "step 3") {
		t.Fatal("step headers out of trace order")
	}
}

// U2/E1: grouping follows the engine's attribution — two tools
// under one step share a header; a new step opens a new one.
func TestTraceToolGroupingUnderSteps(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "a1", ToolName: "read_file", Target: "one.go", Step: "first step"},
		protocol.EngineEvent{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "a1", ToolName: "read_file", Target: "one.go", Result: "10 lines", Step: "first step"},
		protocol.EngineEvent{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "a2", ToolName: "read_file", Target: "two.go", Step: "first step"},
		protocol.EngineEvent{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "a2", ToolName: "read_file", Target: "two.go", Result: "20 lines", Step: "first step"},
		protocol.EngineEvent{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "b1", ToolName: "shell", Target: "go test", Step: "second step"},
		protocol.EngineEvent{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "b1", ToolName: "shell", Target: "go test", Result: "ok", Step: "second step"},
	)
	got := traceText(m)
	if strings.Count(got, "first step") != 1 {
		t.Fatalf("the shared step must render exactly one header:\n%s", got)
	}
	if !strings.Contains(got, "second step") {
		t.Fatalf("the new step must open its own header:\n%s", got)
	}
	if !strings.Contains(got, "one.go") || !strings.Contains(got, "two.go") {
		t.Fatalf("both grouped tool rows must render:\n%s", got)
	}
	if strings.Index(got, "first step") > strings.Index(got, "second step") {
		t.Fatal("step headers out of trace order")
	}
}

// U2/E2: the diff block renders only from the patch event's real
// diff — absent before, present after, with the real lines, under
// its tool row.
func TestTraceDiffBlock(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(traceText(m), "@@") {
		t.Fatal("diff block present before any patch event")
	}

	m = workbenchFullRun(t, 100, 27)
	got := traceText(m)
	// Tabs expand to four spaces in the rendered block (Lip Gloss's
	// rendering rule, applied explicitly by the block renderer).
	for _, want := range []string{"--- a/internal/auth/limit.go", "+++ b/internal/auth/limit.go", "@@ -4,1 +4,1 @@", "-    return true", "+    return allowed()"} {
		if !strings.Contains(got, want) {
			t.Fatalf("diff line %q missing:\n%s", want, got)
		}
	}
	if strings.Index(got, "edit  internal/auth/limit.go") > strings.Index(got, "--- a/internal/auth/limit.go") {
		t.Fatal("the diff block must render beneath its tool row")
	}

	// A run with no patch event renders no diff block at all.
	nopatch := workbenchModel(t, 100, 27)
	nopatch.state.ModelName = "claude-sonnet"
	nopatch = feedEvents(nopatch,
		protocol.EngineEvent{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "r1", ToolName: "read_file", Target: "x.go"},
		protocol.EngineEvent{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "r1", ToolName: "read_file", Target: "x.go", Result: "5 lines"},
	)
	if strings.Contains(traceText(nopatch), "@@") {
		t.Fatal("a run without a patch event must not render a diff block")
	}
}

// U2: the reasoning line is absent when no reasoning event fired,
// collapsed to one row when it did, and expands under ctrl+o.
func TestTraceReasoningLine(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(traceText(m), "thought") {
		t.Fatal("reasoning line present before any reasoning event")
	}

	m = workbenchFullRun(t, 100, 27)
	got := traceText(m)
	if !strings.Contains(got, "thought") {
		t.Fatalf("reasoning line missing after its event:\n%s", got)
	}
	// Collapsed by default: the hint shows, the reasoning text does not.
	if strings.Contains(got, "compare expiry") {
		t.Fatalf("reasoning text must stay collapsed by default:\n%s", got)
	}
	// ctrl+o expands the reasoning cell.
	um, _ := m.Update(tea.KeyPressMsg{Code: 'o', Mod: tea.ModCtrl})
	m = um.(AppModel)
	if !strings.Contains(traceText(m), "compare expiry") {
		t.Fatalf("ctrl+o must expand the reasoning line:\n%s", traceText(m))
	}
}

// U2: the activity line is absent when idle, present while a run is
// in flight, and gone again once nothing is in flight.
func TestTraceActivityLine(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(workbenchFrame(m), "esc pause") {
		t.Fatal("activity line present while idle")
	}
	// In flight: the last tool is running, the line shows its real
	// action and the interrupt hint.
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventTurnStarted, TurnID: "t1"},
		protocol.EngineEvent{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "s1", ToolName: "shell", Target: "go test ./internal/auth"},
	)
	v := workbenchFrame(m)
	if !strings.Contains(v, "running go test") || !strings.Contains(v, "esc pause") {
		t.Fatalf("activity line missing during flight:\n%s", v)
	}
	// Once the run ends, the activity line is absent (P21) — and so
	// is the running state's status-bar hint.
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "s1", ToolName: "shell", Target: "go test ./internal/auth", Result: "31 passed"},
		protocol.EngineEvent{Type: protocol.EventTurnCompleted, TurnID: "t1"},
	)
	if v := workbenchFrame(m); strings.Contains(v, "esc pause") {
		t.Fatalf("activity line must be absent when nothing is in flight:\n%s", v)
	}
}

// U2: the full-run frame fits three widths without overflow, the
// trace's tail stays visible, and the Stream view renders the same
// trace.
func TestTraceFullRunWidthsAndStreamParity(t *testing.T) {
	for _, size := range []struct{ w, h int }{{100, 27}, {80, 24}, {60, 20}} {
		m := workbenchFullRun(t, size.w, size.h)
		view := workbenchFrame(m)
		lines := strings.Split(view, "\n")
		if len(lines) > size.h {
			t.Fatalf("[%dx%d] full-run frame overflows: %d lines", size.w, size.h, len(lines))
		}
		for i, l := range lines {
			if CellWidth(l) > size.w {
				t.Fatalf("[%dx%d] line %d overflows: %q", size.w, size.h, i+1, l)
			}
		}
		// Tail-robust markers: the last step header, the failure
		// excerpt, and the contract strip all stay in view.
		for _, want := range []string{"step 3", "token_test.go:81", "contract"} {
			if !strings.Contains(view, want) {
				t.Fatalf("[%dx%d] trace element %q missing from the view:\n%s", size.w, size.h, want, view)
			}
		}
	}

	// The classic (Stream) view renders the same events the same
	// way: the trace elements carry across layouts. The classic
	// viewport scrolls, so the content assertion reads the trace
	// renderer directly; the view assertion checks the trace's
	// presence through the shared cells.
	st := classicModel(100, 27, false)
	st.state.ModelName = "claude-sonnet"
	for _, evt := range fullRunTrace() {
		e2, _ := st.Update(engineEventMsg(evt))
		st = e2.(AppModel)
	}
	stTrace := Downsample(st.classicTranscript(st.history.Cells))
	for _, want := range []string{"step 1", "Add the limiter", "--- a/internal/auth/limit.go", "thought"} {
		if !strings.Contains(stTrace, want) {
			t.Fatalf("stream view missing trace element %q:\n%s", want, stTrace)
		}
	}
}

// U2 golden frames: the full run (frame6) at three widths, normal
// and plain. Generated with NIKI_WRITE_GOLDEN=1 after printing the
// diff.
func TestTraceGoldenFrames(t *testing.T) {
	sizes := []struct {
		name string
		w, h int
	}{
		{"100x27", 100, 27},
		{"80x24", 80, 24},
		{"60x20", 60, 20},
	}
	modes := []struct {
		name  string
		setup func()
	}{
		{"normal", func() {
			t.Setenv("TERM", "xterm-256color")
			os.Unsetenv("NIKI_PLAIN")
			os.Unsetenv("NIKI_ASCII")
			os.Unsetenv("NO_COLOR")
			ApplyDisplayEnv(io.Discard)
		}},
		{"plain", func() {
			t.Setenv("TERM", "xterm-256color")
			t.Setenv("NIKI_PLAIN", "1")
			os.Unsetenv("NIKI_ASCII")
			ApplyDisplayEnv(io.Discard)
		}},
	}
	for _, mode := range modes {
		for _, size := range sizes {
			mode.setup()
			m := workbenchFullRun(t, size.w, size.h)
			got := Downsample(m.View().Content)
			golden := filepath.Join("testdata", "workbench_frame6_"+size.name+"_"+mode.name+".golden")
			if os.Getenv("NIKI_WRITE_GOLDEN") == "1" {
				if err := os.WriteFile(golden, []byte(got), 0o644); err != nil {
					t.Fatal(err)
				}
				continue
			}
			want, err := os.ReadFile(golden)
			if err != nil {
				if os.IsNotExist(err) {
					t.Fatalf("missing golden %s (generate with NIKI_WRITE_GOLDEN=1)", golden)
				}
				t.Fatal(err)
			}
			if string(want) != got {
				t.Fatalf("[trace %s %s] drifted from golden; regenerate with NIKI_WRITE_GOLDEN=1 after printing the diff", size.name, mode.name)
			}
			t.Logf("[trace %s %s] golden matches (%d bytes)", size.name, mode.name, len(got))
		}
	}
}

// The full-run frame stays pure-ASCII in plain mode.
func TestTraceFullRunPlainMode(t *testing.T) {
	saved := frameProfile
	defer func() { frameProfile = saved }()
	t.Setenv("NIKI_PLAIN", "1")
	ApplyDisplayEnv(io.Discard)
	m := workbenchFullRun(t, 100, 27)
	out := Downsample(m.View().Content)
	if strings.Contains(out, "\x1b[") {
		t.Fatal("plain mode must strip escapes")
	}
	for _, r := range out {
		if r > 127 {
			t.Fatalf("plain mode must be ASCII-only, found %q", r)
		}
	}
	// The glyph states degrade to their words in plain mode.
	for _, want := range []string{"thought", "done", "failed", "running"} {
		if !strings.Contains(out, want) {
			t.Fatalf("plain trace missing word %q:\n%s", want, out)
		}
	}
}
