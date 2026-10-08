package tui

import (
	"os"
	"strings"
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

func TestSlashExplainCites(t *testing.T) {
	dir := t.TempDir()
	content := "package demo\n\n// ComputeThing does the thing.\nfunc ComputeThing() int {\n\treturn 42\n}\n"
	if err := os.WriteFile(dir+"/demo.go", []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.state.Directory = dir

	m.composer.Input.SetValue("/explain `ComputeThing`")
	m2, _ := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if len(m.history.Cells) == 0 {
		t.Fatal("expected /explain to append a cell")
	}
	last := m.history.Cells[len(m.history.Cells)-1]
	if last.Role != "system" || !strings.Contains(last.Text, "demo.go:") {
		t.Fatalf("expected cited answer, got role=%q text=%q", last.Role, last.Text)
	}

	// Unknown symbols are refused, not invented.
	m.composer.Input.SetValue("/explain `NoSuchSymbolZZZ`")
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	last = m.history.Cells[len(m.history.Cells)-1]
	if !strings.Contains(last.Text, "Cannot answer") {
		t.Fatalf("expected refusal, got %q", last.Text)
	}
}
