package tui

import (
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestSpinnerStyles(t *testing.T) {
	styles := []SpinnerStyle{SpinnerBloom, SpinnerBraille, SpinnerSweep, SpinnerPulse}

	for _, s := range styles {
		for f := 0; f < 20; f++ {
			g := SpinGlyphWithStyle(s, f, false, false)
			if g == "" {
				t.Fatalf("style %d frame %d produced empty glyph", s, f)
			}
		}
	}
}

func TestThinkingVerbsAdvance(t *testing.T) {
	if len(ThinkingVerbs) < 3 {
		t.Fatal("expected at least 3 rotating thinking verbs")
	}

	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.state.Busy = true
	m.state.Activity = "thinking…"

	// Frame 0
	m.state.SpinFrame = 0
	v0 := m.activityView()

	// Frame 12 (~1.4s later)
	m.state.SpinFrame = 12
	v1 := m.activityView()

	if v0 == v1 {
		t.Fatalf("thinking verb should rotate across frames: v0=%q v1=%q", v0, v1)
	}
}
