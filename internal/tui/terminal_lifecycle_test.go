package tui

import (
	"os"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// TestCtrlZSuspends (L2): Ctrl+Z hands the program a
// Suspend command, which bubbletea turns into a terminal
// release + SIGSTOP + restore cycle.
func TestCtrlZSuspends(t *testing.T) {
	m, _, _ := newModel(false)
	um, cmd := m.Update(tea.KeyMsg{Type: tea.KeyCtrlZ})
	if _, ok := um.(AppModel); !ok {
		t.Fatalf("ctrl+z must keep the model, got %T", um)
	}
	if cmd == nil {
		t.Fatal("ctrl+z must return a command")
	}
	msg := cmd()
	if _, ok := msg.(tea.SuspendMsg); !ok {
		t.Errorf("ctrl+z command produced %T, want tea.SuspendMsg", msg)
	}
}

// TestResizeNeverPanics (L3): degenerate and extreme
// sizes clamp instead of panicking, in both inline and
// altscreen modes, on first size and on resize.
func TestResizeNeverPanics(t *testing.T) {
	sizes := []tea.WindowSizeMsg{
		{Width: 0, Height: 0},
		{Width: 1, Height: 1},
		{Width: 1, Height: 4},
		{Width: 80, Height: 24},
		{Width: 300, Height: 100},
		{Width: 10000, Height: 10000},
	}
	for _, inline := range []bool{false, true} {
		for _, size := range sizes {
			cmdChan := make(chan protocol.EngineCommand, 64)
			eventChan := make(chan protocol.EngineEvent, 64)
			m := NewAppModel(cmdChan, eventChan)
			m.state.Inline = inline
			func() {
				defer func() {
					if r := recover(); r != nil {
						t.Fatalf("resize %+v (inline=%v) panicked: %v", size, inline, r)
					}
				}()
				um, _ := m.Update(size)
				m2 := um.(AppModel)
				// A second resize on the live model must
				// be safe too.
				um2, _ := m2.Update(tea.WindowSizeMsg{Width: 5, Height: 2})
				_ = um2.(AppModel).View()
			}()
		}
	}
}

// TestNoColorStripsColor (U7): with NO_COLOR set the
// rendered frame contains no ANSI escape sequences at all.
func TestNoColorStripsColor(t *testing.T) {
	t.Setenv("NO_COLOR", "1")
	old := lipgloss.DefaultRenderer()
	defer lipgloss.SetDefaultRenderer(old)
	// A fresh renderer picks its color profile from the
	// environment, so NO_COLOR degrades it to Ascii.
	r := lipgloss.NewRenderer(os.Stdout)
	lipgloss.SetDefaultRenderer(r)

	m, _, _ := newModel(false)
	out := m.View()
	if strings.Contains(out, "\x1b[") {
		t.Errorf("NO_COLOR set but view contains escape sequences: %q", out)
	}
}
