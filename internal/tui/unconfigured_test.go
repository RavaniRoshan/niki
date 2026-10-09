package tui

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestUnconfiguredModelPromptShowsAlert(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// App starts with ModelName = "None"
	if m.state.ModelName != "None" {
		t.Fatalf("expected initial ModelName to be 'None', got %q", m.state.ModelName)
	}

	// Submit an AI prompt
	m.composer.Input.SetValue("Write me a sorting function")
	m2, _ := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)

	// Must NOT send to cmdChan, and must record alert card in history
	if len(cmdChan) != 0 {
		t.Fatal("expected no engine commands sent when model is unconfigured")
	}

	// Must have alert in history
	foundAlert := false
	for _, cell := range m.history.Cells {
		if strings.Contains(cell.Text, "No AI Model Configured") || strings.Contains(cell.Text, "/connect") {
			foundAlert = true
			break
		}
	}
	if !foundAlert {
		t.Fatal("expected NoModelAlert card in history")
	}

	// Palette must automatically open in connect search mode
	if !m.state.Palette.Open {
		t.Fatal("expected palette to open automatically to assist user with connecting a model")
	}
}

func TestUnconfiguredModelAllowsLocalShellCommands(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)

	// Shell command prefixed with '!' should execute without needing an AI model
	m.composer.Input.SetValue("! echo 'offline works'")
	m2, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)

	if cmd == nil {
		t.Fatal("expected shell execution command tea.Cmd returned")
	}
	if len(cmdChan) != 0 {
		t.Fatal("expected no LLM engine command dispatched")
	}
}
