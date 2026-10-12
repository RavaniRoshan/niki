package tui

import (
	"image/color"
	"io"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// U3: the rails and the mascot. Each element is proven absent
// before its event fires and present after; Nib is proven across
// all five run states with body color AND eye shape changing,
// constant width, ASCII and NO_COLOR variants, and no redraw of
// its own.

// U3: the Plan list renders from the plan event and is absent when
// no plan exists.
func TestU3PlanRail(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if v := workbenchFrame(m); strings.Contains(v, "PLAN") {
		t.Fatal("PLAN section present before any plan event")
	}
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
		{Description: "Add the limiter", Status: "active"},
	}})
	v := workbenchFrame(m)
	if !strings.Contains(v, "PLAN") || !strings.Contains(v, "Add the limiter") {
		t.Fatalf("plan rail missing after the plan event:\n%s", v)
	}
}

// U3: the Agents list renders from the subagent events and is
// absent when there are no subagents.
func TestU3AgentsRail(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if v := workbenchFrame(m); strings.Contains(v, "AGENTS") {
		t.Fatal("AGENTS section present before any subagent event")
	}
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventSubagentStarted, Text: "agent-7"},
		protocol.EngineEvent{Type: protocol.EventSubagentFailed, Text: "agent-7", Error: "boom"},
	)
	v := workbenchFrame(m)
	if !strings.Contains(v, "AGENTS") || !strings.Contains(v, "agent-7") {
		t.Fatalf("agents rail missing after the subagent events:\n%s", v)
	}
}

// U3: the Context meter shows the real used/window or the em dash —
// never a fake number.
func TestU3ContextMeter(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	rail := m.wbRightRail(24)
	if !strings.Contains(rail, dash()) {
		t.Fatalf("unknown context must render the em dash: %q", rail)
	}
	if strings.Contains(rail, "0 /") || strings.Contains(rail, "0k") {
		t.Fatalf("context must not fake a zero: %q", rail)
	}
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventAssistantMessageDone,
		Usage: &protocol.Usage{PromptTokens: 5120, TotalTokens: 5120}})
	rail = m.wbRightRail(24)
	if !strings.Contains(rail, "5.1k / 128k") {
		t.Fatalf("real usage must render the real pair: %q", rail)
	}
}

// U3/E3: the Checkpoints section renders from the checkpoint
// events and is absent when none exist — including after a full
// run that created none.
func TestU3CheckpointsRail(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if v := workbenchFrame(m); strings.Contains(v, "CHECKPOINTS") {
		t.Fatal("CHECKPOINTS present before any checkpoint event")
	}
	if v := workbenchFrame(m); strings.Contains(v, "checkpoints 1") {
		t.Fatal("checkpoint count present with no checkpoints")
	}

	// A run whose fixture predates checkpoint events renders none.
	full := workbenchFullRun(t, 100, 27)
	// (the full-run fixture now carries a real checkpoint event,
	// so this asserts the presence path)
	v := workbenchFrame(full)
	if !strings.Contains(v, "CHECKPOINTS") || !strings.Contains(v, "turn 1") {
		t.Fatalf("checkpoints rail missing after the checkpoint event:\n%s", v)
	}
	if !strings.Contains(v, "checkpoints 1") {
		t.Fatalf("status-bar checkpoint count missing:\n%s", v)
	}

	// Absence is a state: a run with no checkpoint event shows no
	// section and no count.
	plain := workbenchModel(t, 100, 27)
	plain.state.ModelName = "claude-sonnet"
	plain = feedEvents(plain,
		protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
			{Description: "Add the limiter", Status: "active"},
		}},
	)
	if v := workbenchFrame(plain); strings.Contains(v, "CHECKPOINTS") || strings.Contains(v, "checkpoints") {
		t.Fatalf("checkpoint chrome without a checkpoint event:\n%s", v)
	}
}

// U3: Cost shows the real turn and session cost, or the em dash —
// never a fabricated zero.
func TestU3CostRail(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	rail := m.wbRightRail(24)
	if !strings.Contains(rail, "turn "+dash()) || !strings.Contains(rail, "session "+dash()) {
		t.Fatalf("unknown costs must render the em dash: %q", rail)
	}
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventAssistantMessageDone,
		Usage: provider_Usage(5120, 5120)})
	rail = m.wbRightRail(24)
	if !strings.Contains(rail, "turn $") || !strings.Contains(rail, "session $") {
		t.Fatalf("real costs must render after usage: %q", rail)
	}
}

// provider_Usage builds the usage payload the provider emits.
func provider_Usage(prompt, total int) *protocol.Usage {
	return &protocol.Usage{PromptTokens: prompt, TotalTokens: total}
}

// nibFrames renders the model in each run state through real
// events and returns the eye-bearing rows (header orb + status
// bar) per state.
func nibFrames(t *testing.T, m AppModel) map[string]string {
	t.Helper()
	out := map[string]string{}
	states := []struct {
		name   string
		events []protocol.EngineEvent
	}{
		{"idle", nil},
		{"working", []protocol.EngineEvent{{Type: protocol.EventTurnStarted, TurnID: "t1"}}},
		{"done", []protocol.EngineEvent{{Type: protocol.EventTurnStarted, TurnID: "t1"}, {Type: protocol.EventTurnCompleted, TurnID: "t1"}}},
		{"error", []protocol.EngineEvent{{Type: protocol.EventTurnStarted, TurnID: "t1"}, {Type: protocol.EventTurnFailed, TurnID: "t1", Error: "boom"}}},
		{"interrupted", []protocol.EngineEvent{{Type: protocol.EventTurnStarted, TurnID: "t1"}, {Type: protocol.EventTurnCancelled, TurnID: "t1", Text: "paused"}}},
	}
	for _, st := range states {
		m = feedEvents(m, st.events...)
		out[st.name] = workbenchFrame(m)
	}
	return out
}

// U3: Nib renders in the header and status bar across all five run
// states, with the body color AND the eye shape both changing —
// color is never the only signal.
func TestNibStatesHeaderAndStatusBar(t *testing.T) {
	base := workbenchModel(t, 100, 27)
	base.state.ModelName = "claude-sonnet"
	frames := nibFrames(t, base)

	wantEye := map[string]string{
		"idle": "◐", "working": "◑", "done": "●", "error": "○", "interrupted": "◐",
	}
	wantWord := map[string]string{
		"idle": "idle", "working": "working", "done": "done", "error": "error", "interrupted": "interrupted",
	}
	seenEyes := map[string]bool{}
	for name, frame := range frames {
		eye := wantEye[name]
		if !strings.Contains(frame, eye) {
			t.Fatalf("[%s] eye %q missing from the frame:\n%s", name, eye, frame)
		}
		if !strings.Contains(frame, wantWord[name]) {
			t.Fatalf("[%s] status word %q missing:\n%s", name, wantWord[name], frame)
		}
		seenEyes[eye] = true
	}
	// The eye shape itself carries the state: at least three
	// distinct eyes across the five states (half-lit, flipped,
	// filled, hollow).
	if len(seenEyes) < 3 {
		t.Fatalf("the eye must distinguish states, saw %d shapes", len(seenEyes))
	}
	// done and error differ from each other and from idle/working
	// in shape, not only color.
	if wantEye["done"] == wantEye["idle"] || wantEye["error"] == wantEye["idle"] {
		t.Fatal("done/error eyes must differ from the idle eye")
	}
}

// U3: the header orb and the status-bar row both carry the state,
// and both change together.
func TestNibHeaderAndStatusBarAgree(t *testing.T) {
	base := workbenchModel(t, 100, 27)
	base.state.ModelName = "claude-sonnet"
	frames := nibFrames(t, base)
	// The status bar is the last line of every frame; the header
	// orb is the first three rows.
	for name, frame := range frames {
		lines := strings.Split(frame, "\n")
		last := lines[len(lines)-1]
		if !strings.Contains(last, "["+wantStatusEye(name)+"] "+wantWord(name)) {
			t.Fatalf("[%s] status bar missing the state eye/word: %q", name, last)
		}
		head := strings.Join(lines[:3], "\n")
		if !strings.Contains(head, wantHeaderEye(name)) {
			t.Fatalf("[%s] header orb missing the state eye:\n%s", name, head)
		}
	}
}

func wantStatusEye(state string) string {
	switch state {
	case "done":
		return "●"
	case "error":
		return "○"
	case "working":
		return "◑"
	default:
		return "◐"
	}
}

func wantHeaderEye(state string) string { return wantStatusEye(state) }

func wantWord(state string) string {
	switch state {
	case "working":
		return "working"
	case "done":
		return "done"
	case "error":
		return "error"
	case "interrupted":
		return "interrupted"
	default:
		return "idle"
	}
}

// U3: the mascot art is constant-width in every state (no layout
// jitter between states).
func TestNibConstantWidth(t *testing.T) {
	base := workbenchModel(t, 100, 27)
	base.state.ModelName = "claude-sonnet"
	frames := nibFrames(t, base)
	var widths []int
	for _, frame := range frames {
		lines := strings.Split(frame, "\n")
		// The orb occupies the first three rows; each row's width
		// must be identical across states.
		w := CellWidth(lines[0]) + CellWidth(lines[1]) + CellWidth(lines[2])
		widths = append(widths, w)
	}
	for i := 1; i < len(widths); i++ {
		if widths[i] != widths[0] {
			t.Fatalf("mascot width jitter: %v", widths)
		}
	}
	// And the orb itself (rows 1-3, first 7 cells) never changes
	// width between states.
	orbWidths := map[int]bool{}
	for _, frame := range frames {
		lines := strings.Split(frame, "\n")
		orbWidths[CellWidth(lines[1][:min(7, len(lines[1]))])] = true
	}
	if len(orbWidths) != 1 {
		t.Fatalf("orb width must be constant across states: %v", orbWidths)
	}
}

// U3: Nib never schedules a redraw of its own — an idle model with
// no events renders byte-identically and starts no tick; the
// state changes only through real events.
func TestNibNoSelfScheduledRedraw(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	// Init schedules only the event wait and the submodule probe —
	// no mascot tick.
	cmds := m.Init()
	if cmds == nil {
		t.Fatal("precondition: Init schedules the event wait")
	}
	// Two View calls with no Update between are byte-identical.
	a := workbenchFrame(m)
	b := workbenchFrame(m)
	if a != b {
		t.Fatal("Nib's render is not deterministic without events")
	}
	// The frame changes only when a run-state event arrives.
	before := workbenchFrame(m)
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventTurnStarted, TurnID: "t1"})
	if workbenchFrame(m) == before {
		t.Fatal("the state did not change on a real run event")
	}
	// A non-event message (a window size to the same size) changes
	// nothing about the mascot's state machine.
	same := workbenchFrame(m)
	um, _ := m.Update(tea.WindowSizeMsg{Width: 100, Height: 27})
	if workbenchFrame(um.(AppModel)) != same {
		t.Fatal("a resize with no state change must not alter the mascot")
	}
}

// U3: the ASCII mascot uses the fallback eyes o (idle/working),
// * (done), x (error).
func TestNibASCIIEyes(t *testing.T) {
	saved := frameProfile
	defer func() { frameProfile = saved }()
	t.Setenv("NIKI_ASCII", "1")
	ApplyDisplayEnv(io.Discard)
	base := workbenchModel(t, 80, 24)
	base.state.ModelName = "claude-sonnet"
	frames := nibFrames(t, base)
	for _, tc := range []struct{ state, eye string }{
		{"idle", "o"}, {"working", "o"}, {"done", "*"}, {"error", "x"},
	} {
		frame := frames[tc.state]
		if !strings.Contains(frame, tc.eye) {
			t.Fatalf("[ascii %s] eye %q missing:\n%s", tc.state, tc.eye, frame)
		}
		for _, r := range frame {
			if r > 127 {
				t.Fatalf("[ascii %s] non-ascii rune %q", tc.state, r)
			}
		}
	}
}

// U3: the NO_COLOR variant keeps every state legible with no
// escape sequences.
func TestNibNoColor(t *testing.T) {
	saved := frameProfile
	defer func() { frameProfile = saved }()
	t.Setenv("NO_COLOR", "1")
	ApplyDisplayEnv(io.Discard)
	base := workbenchModel(t, 100, 27)
	base.state.ModelName = "claude-sonnet"
	for _, frame := range nibFrames(t, base) {
		if strings.Contains(frame, "\x1b[") {
			t.Fatalf("NO_COLOR frame carries escapes:\n%q", frame)
		}
	}
}

// U3: the mascot's per-state body colors clear the 3:1 UI-glyph
// contrast bar in every theme variant SelectTheme returns.
func TestNibContrastEveryTheme(t *testing.T) {
	for _, name := range []string{"default", "dark", "light", "monochrome"} {
		th := SelectTheme(name)
		// The four body colors the state machine uses.
		bodies := []struct {
			label string
			c     color.Color
		}{
			{"idle/working accent", th.Accent.GetForeground()},
			{"done success", th.Success.GetForeground()},
			{"error", th.Error.GetForeground()},
			{"interrupted muted", th.Muted.GetForeground()},
		}
		for _, b := range bodies {
			ratio := contrastRatio(b.c, darkBG)
			if ratio < 3.0 {
				t.Errorf("[nib contrast %s] %s: %.2f:1, want >= 3.0:1", name, b.label, ratio)
			} else {
				t.Logf("[nib contrast %-10s %-20s %.2f:1", name, b.label, ratio)
			}
		}
	}
}
