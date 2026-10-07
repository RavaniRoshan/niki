package tui

import (
	"testing"

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
