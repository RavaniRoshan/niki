package tui

import (
	"io"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// C4 widths: the classic layout never panics at any size and keeps
// its invariants at 100/80/60/49.
func TestClassicWidths(t *testing.T) {
	for _, w := range []int{10, 20, 49, 50, 60, 79, 80, 99, 100, 120, 200} {
		m := classicFixtureSetup(w, 27, "frame3")
		view := Downsample(m.View().Content)
		if view == "" {
			t.Fatalf("width %d rendered nothing", w)
		}
		for _, line := range strings.Split(view, "\n") {
			if CellWidth(line) > w && w >= 20 {
				t.Fatalf("width %d: line exceeds the terminal (%d cells): %q", w, CellWidth(line), line)
			}
		}
		if w < 50 {
			if w >= 42 && !strings.Contains(view, "resize to 50 or wider") {
				t.Fatalf("width %d must show the resize notice: %q", w, view)
			}
			if w < 42 && !strings.Contains(view, "resize") {
				t.Fatalf("width %d must show the truncated notice: %q", w, view)
			}
		}
	}
}

// C4: under 50 columns the layout is one line and a pending gate
// stays visible — the run's state is preserved, not dropped.
func TestNarrowModePreservesState(t *testing.T) {
	m := classicFixtureSetup(100, 27, "frame4")
	if !m.state.Gate.Active {
		t.Fatal("precondition: gate open at frame4")
	}
	narrow, _ := m.Update(tea.WindowSizeMsg{Width: 49, Height: 24})
	m = narrow.(AppModel)
	view := Downsample(m.View().Content)
	lines := strings.Split(strings.TrimRight(view, "\n"), "\n")
	if !strings.HasPrefix(lines[0], "resize to 50 or wider") {
		t.Fatalf("narrow mode must lead with the notice: %q", view)
	}
	if !strings.Contains(view, "needs you") || !strings.Contains(view, "write the limiter without it") {
		t.Fatalf("the gate must stay visible under 50 columns: %q", view)
	}
	for _, line := range lines {
		if CellWidth(line) > 49 {
			t.Fatalf("narrow gate line too wide (%d): %q", CellWidth(line), line)
		}
	}
	if !m.state.Gate.Active {
		t.Fatal("resize must not drop the gate state")
	}
}

// C4: a resize preserves focus, scroll, and gate state.
func TestResizePreservesState(t *testing.T) {
	m := classicFixtureSetup(100, 27, "frame4")
	m.viewport.SetYOffset(3)
	before := m.viewport.YOffset()
	cursorBefore := m.composer.Input.Position()

	resized, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	m = resized.(AppModel)
	if !m.state.Gate.Active {
		t.Fatal("gate state lost on resize")
	}
	if m.composer.Input.Position() != cursorBefore {
		t.Fatalf("composer focus lost on resize: %d -> %d", cursorBefore, m.composer.Input.Position())
	}
	// The scroll offset survives a taller viewport (it may clamp to
	// the new max, but content beyond the top stays reachable).
	if after := m.viewport.YOffset(); after < before && m.viewport.Height() > before {
		t.Fatalf("scroll offset lost on resize: %d -> %d", before, after)
	}
	if m.state.Busy != true {
		t.Fatal("run state lost on resize")
	}
}

// C4 plain mode (NIKI_PLAIN=1): every glyph renders as a word and
// the frame is pure ASCII — orb art, sweep, state glyphs, rule,
// prompt, loop, and gate marks all included.
func TestClassicPlainModeGlyphWords(t *testing.T) {
	t.Setenv("NIKI_PLAIN", "1")
	ApplyDisplayEnv(io.Discard)

	checks := []struct {
		name  string
		setup func(*AppModel)
	}{
		{"frame3", func(m *AppModel) {}},
		{"frame4", func(m *AppModel) {}},
		{"frame5", func(m *AppModel) {}},
	}
	for _, c := range checks {
		m := classicFixtureSetup(100, 27, c.name)
		view := Downsample(m.View().Content)
		for _, r := range view {
			if r > 127 {
				t.Fatalf("[%s plain] non-ASCII rune %q in:\n%s", c.name, r, view)
			}
		}
		for _, word := range []string{"done", "running"} {
			if !strings.Contains(view, word) {
				t.Fatalf("[%s plain] state glyph word %q missing:\n%s", c.name, word, view)
			}
		}
	}
	// The orb degrades to bracket art and the composer prompt to ">".
	launch := classicFixtureSetup(100, 27, "frame1")
	view := Downsample(launch.View().Content)
	if !strings.Contains(view, "/") || strings.Contains(view, "▄") {
		t.Fatalf("[plain] ascii orb art expected:\n%s", view)
	}
}

// C4 ASCII mode (NIKI_ASCII=1, no plain): glyphs fall back to ASCII
// but color styling may remain.
func TestClassicAsciiMode(t *testing.T) {
	t.Setenv("NIKI_ASCII", "1")
	ApplyDisplayEnv(io.Discard)
	m := classicFixtureSetup(100, 27, "frame3")
	view := m.View().Content
	for _, bad := range []string{"✓", "✗", "◐", "○", "↺", "■", "›", "·"} {
		if strings.Contains(view, bad) {
			t.Fatalf("[ascii] glyph %q must fall back", bad)
		}
	}
	for _, want := range []string{"+", "*", "o"} {
		if !strings.Contains(view, want) {
			t.Fatalf("[ascii] fallback %q missing:\n%s", want, view)
		}
	}
}

// C4 NO_COLOR: the downsampled frame carries no escape sequences.
func TestClassicNoColorMode(t *testing.T) {
	t.Setenv("NO_COLOR", "1")
	ApplyDisplayEnv(io.Discard)
	m := classicFixtureSetup(100, 27, "frame4")
	out := Downsample(m.View().Content)
	if strings.Contains(out, "\x1b[") {
		t.Fatalf("[nocolor] escapes present:\n%q", out)
	}
}

// C4: the gate remains answerable at every width (the safe option
// stays the default focus after a resize).
func TestGateDefaultSurvivesResize(t *testing.T) {
	evt := protocol.EngineEvent{Type: protocol.EventQuestionPrompted, Questions: []protocol.UserQuestion{{
		Question: "add golang.org/x/time?",
		Options:  []string{"allow once", "write the limiter without it"},
		Safe:     1,
	}}}
	m := classicModel(100, 27, false)
	um, _ := m.Update(engineEventMsg(evt))
	m = um.(AppModel)
	for _, w := range []int{49, 60, 80, 100, 120} {
		rs, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: 24})
		m = rs.(AppModel)
		if m.state.Gate.Selected != 1 {
			t.Fatalf("width %d: gate focus moved to %d, want the safe option 1", w, m.state.Gate.Selected)
		}
	}
}
