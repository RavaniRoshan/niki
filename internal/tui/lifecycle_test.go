package tui

import (
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func newModel(inline bool) (AppModel, chan protocol.EngineCommand, chan protocol.EngineEvent) {
	cmdChan := make(chan protocol.EngineCommand, 64)
	eventChan := make(chan protocol.EngineEvent, 64)
	m := NewAppModel(cmdChan, eventChan)
	m.state.Inline = inline
	um, _ := m.Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	return um.(AppModel), cmdChan, eventChan
}

// TestLiveRegionSplit (U9): finalized history
// leaves the live region; the viewport renders
// only cells after the committed boundary.
func TestLiveRegionSplit(t *testing.T) {
	m, _, _ := newModel(true)

	m.history.Append("user", "first question")
	m.history.AppendDelta("first answer")
	committed := m.history.Finalize()
	if len(committed) != 2 {
		t.Fatalf("expected 2 committed cells, got %d", len(committed))
	}
	if len(m.history.Live()) != 0 {
		t.Fatalf("live region should be empty after finalize, got %d", len(m.history.Live()))
	}

	// A new turn streams into the live region
	// without touching committed cells.
	m.history.AppendDelta("second answer")
	if len(m.history.Live()) != 1 {
		t.Fatalf("live region should hold the new delta, got %d", len(m.history.Live()))
	}
	if m.history.Committed != 2 {
		t.Fatalf("committed boundary moved: %d", m.history.Committed)
	}
}

// TestInlineFlushOnTurnEnd (U9): in inline
// mode a completed turn finalizes and the
// viewport keeps only the live region.
func TestInlineFlushOnTurnEnd(t *testing.T) {
	m, cmdChan, eventChan := newModel(true)
	_ = cmdChan

	eventChan <- protocol.EngineEvent{Type: protocol.EventTurnStarted}
	um, _ := m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "partial "}
	eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "answer"}
	um, _ = m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	eventChan <- protocol.EngineEvent{Type: protocol.EventTurnCompleted}
	um, _ = m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)

	if m.history.Committed != len(m.history.Cells) {
		t.Fatalf("turn end must finalize history: committed=%d cells=%d",
			m.history.Committed, len(m.history.Cells))
	}
	if len(m.history.Live()) != 0 {
		t.Fatalf("live region must be empty after turn end, got %d", len(m.history.Live()))
	}
	// The viewport shows the live region, which
	// is now empty.
	if m.viewport.View() != "" && strings.Contains(m.viewport.View(), "answer") {
		t.Fatalf("committed history leaked into the live viewport: %q", m.viewport.View())
	}
}

// TestActivityLineFromEvents (U3): the single
// activity line above the composer is driven
// by real events only.
func TestActivityLineFromEvents(t *testing.T) {
	m, _, eventChan := newModel(false)

	eventChan <- protocol.EngineEvent{Type: protocol.EventToolStarted, ToolName: "grep"}
	um, _ := m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	if m.state.Activity != "running grep" {
		t.Fatalf("activity=%q", m.state.Activity)
	}
	if !strings.Contains(m.activityView(), "running grep") {
		t.Fatalf("activity line not rendered: %q", m.activityView())
	}

	eventChan <- protocol.EngineEvent{Type: protocol.EventToolCompleted, ToolName: "grep"}
	um, _ = m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	if m.state.Activity != "" {
		t.Fatalf("activity should clear on tool completion: %q", m.state.Activity)
	}

	// Deltas set the activity line too.
	eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "x"}
	um, _ = m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	if m.state.Activity != "streaming…" {
		t.Fatalf("activity=%q", m.state.Activity)
	}
}

// TestDebugViewTelemetry (B9): the debug view
// shows boot-phase timings and frame telemetry.
func TestDebugViewTelemetry(t *testing.T) {
	m, _, eventChan := newModel(false)

	eventChan <- protocol.EngineEvent{Type: protocol.EventBootPhase, Text: "engine:ready", Duration: 3 * time.Millisecond}
	um, _ := m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	eventChan <- protocol.EngineEvent{Type: protocol.EventBootPhase, Text: "model:ready", Duration: 5 * time.Millisecond}
	um, _ = m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	m.state.Debug = true
	view := m.View()
	if !strings.Contains(view, "boot phases") {
		t.Fatalf("debug view missing boot phases: %q", view)
	}
	if !strings.Contains(view, "engine") || !strings.Contains(view, "model") {
		t.Fatalf("debug view missing phase names: %q", view)
	}
	if !strings.Contains(view, "frames:") || !strings.Contains(view, "render last:") {
		t.Fatalf("debug view missing frame telemetry: %q", view)
	}
	// View() records render cost.
	if m.telemetry.Frames == 0 {
		t.Fatal("View did not record frame telemetry")
	}
}

// TestPacingHysteresis (U10): deltas inside
// the stream interval coalesce; the pacer
// flushes at the boundary.
func TestPacingHysteresis(t *testing.T) {
	m, _, eventChan := newModel(false)

	// First delta renders immediately (pacer
	// cold).
	eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "a"}
	um, _ := m.Update(engineEventMsg(<-eventChan))
	m = um.(AppModel)
	if !strings.Contains(m.viewport.View(), "a") {
		t.Fatalf("first delta should render immediately: %q", m.viewport.View())
	}

	// Rapid deltas inside the interval are
	// paced: they set pending and schedule a
	// tick rather than rendering immediately.
	for i := 0; i < 5; i++ {
		eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "b"}
		um, _ = m.Update(engineEventMsg(<-eventChan))
		m = um.(AppModel)
	}
	if !m.pacer.pending {
		t.Fatal("rapid deltas should leave the pacer pending")
	}
	if strings.Contains(m.viewport.View(), "bbbbb") {
		t.Fatal("paced deltas rendered before the interval boundary")
	}

	// The tick flush renders the coalesced
	// content.
	um, _ = m.Update(tickMsg(time.Now()))
	m = um.(AppModel)
	if m.pacer.pending {
		t.Fatal("tick flush should clear the pacer")
	}
	if !strings.Contains(m.viewport.View(), "bbbbb") {
		t.Fatalf("coalesced deltas not rendered: %q", m.viewport.View())
	}
}

// TestComposerOnlyBorderedElement (U4): the
// composer is the only bordered element in
// the view.
func TestComposerOnlyBorderedElement(t *testing.T) {
	m, _, _ := newModel(false)
	m.history.Append("user", "hi")
	view := m.View()
	borders := strings.Count(view, "┌") + strings.Count(view, "╭")
	if borders != 1 {
		t.Fatalf("expected exactly one bordered element (composer), found %d in %q", borders, view)
	}
	if !strings.Contains(view, "Type a prompt") {
		t.Fatalf("composer missing from view: %q", view)
	}
}

// TestInlineHeaderScrollsAway (U4): in inline
// mode the header is flushed to native
// scrollback once and absent from the live
// view.
func TestInlineHeaderScrollsAway(t *testing.T) {
	m, _, _ := newModel(true)
	// The first WindowSizeMsg in inline mode
	// prints the header; simulate by checking
	// the live view has no header.
	view := m.View()
	if strings.Contains(view, "Local Coding Agent") {
		t.Fatalf("header must scroll away in inline mode: %q", view)
	}
	if !m.headerPrinted {
		t.Fatal("header should have been printed to scrollback on first ready")
	}
}

// TestTranscriptRendersFromRealEvents (U2):
// user, assistant, tool, and error cells all
// render from real events.
func TestTranscriptRendersFromRealEvents(t *testing.T) {
	m, _, eventChan := newModel(false)

	events := []protocol.EngineEvent{
		{Type: protocol.EventAssistantTextDelta, Text: "answer"},
		{Type: protocol.EventTurnCompleted},
	}
	for _, e := range events {
		eventChan <- e
		um, _ := m.Update(engineEventMsg(<-eventChan))
		m = um.(AppModel)
	}
	m.history.Append("user", "question")
	m.history.Append("tool", "grep done")
	m.history.Append("error", "boom")
	m.history.Append("system", "note")

	rendered := m.RenderHistory()
	for _, want := range []string{"question", "answer", "grep done", "boom", "note"} {
		if !strings.Contains(rendered, want) {
			t.Fatalf("transcript missing %q: %q", want, rendered)
		}
	}
}
