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
