package tui

import (
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// TestCoalescedFrameBurst asserts a burst of queued events collapses into one render update (B10).
func TestCoalescedFrameBurst(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 32)
	eventChan := make(chan protocol.EngineEvent, 32)
	m := NewAppModel(cmdChan, eventChan)
	um, _ := m.Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	m = um.(AppModel)
	// Queue a burst of deltas before delivering one event.
	for i := 0; i < 8; i++ {
		eventChan <- protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Text: "x"}
	}
	um2, _ := m.Update(engineEventMsg(protocol.EngineEvent{Type: protocol.EventTurnStarted}))
	m = um2.(AppModel)
	// All burst deltas should have been drained into the same update pass.
	count := 0
	for _, c := range m.history.Cells {
		if c.Role == "assistant" {
			count += len(c.Text)
		}
	}
	if count != 8 {
		t.Fatalf("expected 8 coalesced deltas, got %d", count)
	}
	// Nothing left queued.
	select {
	case <-eventChan:
		t.Fatal("expected channel to be drained")
	case <-time.After(10 * time.Millisecond):
	}
}
