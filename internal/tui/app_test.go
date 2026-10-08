package tui

import (
	"testing"

	"github.com/charmbracelet/bubbles/cursor"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestAppModelUpdate(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m2, _ := m.Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	if !m2.(AppModel).state.Ready {
		t.Fatal("viewport should be ready after window size")
	}
}

// TestReducedMotion (U8): the reduced-motion
// preference swaps the blinking composer cursor
// for a static one and records the state.
func TestReducedMotion(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	if m.composer.Input.Cursor.Mode() != cursor.CursorBlink {
		t.Fatalf("default cursor mode = %v, want %v", m.composer.Input.Cursor.Mode(), cursor.CursorBlink)
	}
	m.SetReducedMotion(true)
	if !m.state.ReducedMotion {
		t.Fatal("reduced motion not recorded")
	}
	if m.composer.Input.Cursor.Mode() != cursor.CursorStatic {
		t.Fatalf("cursor mode = %v, want %v", m.composer.Input.Cursor.Mode(), cursor.CursorStatic)
	}
	// Toggling back restores the blink.
	m.SetReducedMotion(false)
	if m.composer.Input.Cursor.Mode() != cursor.CursorBlink {
		t.Fatalf("cursor mode after restore = %v, want %v", m.composer.Input.Cursor.Mode(), cursor.CursorBlink)
	}
}

func TestComposerSuggestions(t *testing.T) {
	if got := composerSuggestions("/do"); len(got) == 0 {
		t.Fatal("expected command suggestions")
	}
	if got := composerSuggestions("@a"); len(got) == 0 {
		t.Fatal("expected file suggestions")
	}
	if got := composerSuggestions("plain"); len(got) != 0 {
		t.Fatalf("unexpected suggestions: %v", got)
	}
}

func TestSlashCommandsIntercepted(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// Test /model
	m.composer.Input.SetValue("/model")
	m2, _ := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if len(m.history.Cells) == 0 || m.history.Cells[len(m.history.Cells)-1].Role != "system" {
		t.Fatal("expected /model to append system cell")
	}

	// Test /doctor
	m.composer.Input.SetValue("/doctor")
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if len(m.history.Cells) < 2 || m.history.Cells[len(m.history.Cells)-1].Role != "system" {
		t.Fatal("expected /doctor to append system cell")
	}
}
