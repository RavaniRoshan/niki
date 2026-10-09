package tui

import (
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestSessionPickerFiltering(t *testing.T) {
	sessions := []protocol.SessionMetadata{
		{ID: "sess-1", Title: "Fix parser bug", CreatedAt: time.Now(), TurnCount: 5},
		{ID: "sess-2", Title: "Implement TUI palette", CreatedAt: time.Now().Add(-1 * time.Hour), TurnCount: 12},
		{ID: "sess-3", Title: "Docker containerization", CreatedAt: time.Now().Add(-24 * time.Hour), TurnCount: 3},
	}

	filtered := FilterSessions(sessions, "parser")
	if len(filtered) != 1 || filtered[0].ID != "sess-1" {
		t.Fatalf("expected sess-1, got %v", filtered)
	}

	filteredID := FilterSessions(sessions, "sess-2")
	if len(filteredID) != 1 || filteredID[0].ID != "sess-2" {
		t.Fatalf("expected sess-2, got %v", filteredID)
	}

	filteredNone := FilterSessions(sessions, "nonexistent")
	if len(filteredNone) != 0 {
		t.Fatalf("expected empty, got %v", filteredNone)
	}
}

func TestSessionPickerKeyboardNavigation(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 10)
	eventChan := make(chan protocol.EngineEvent, 10)
	app := NewAppModel(cmdChan, eventChan)

	// Press Ctrl+S to open
	updated, _ := app.Update(tea.KeyMsg{Type: tea.KeyCtrlS})
	m := updated.(AppModel)
	if !m.state.SessionPicker.Active {
		t.Fatalf("expected SessionPicker to be active")
	}

	// Supply session list event
	listEvt := protocol.EngineEvent{
		Type: protocol.EventSessionList,
		Sessions: []protocol.SessionMetadata{
			{ID: "s-1", Title: "Session One", TurnCount: 2},
			{ID: "s-2", Title: "Session Two", TurnCount: 4},
		},
		History: []string{"> prompt 1", "● response 1"},
	}
	updated, _ = m.Update(engineEventMsg(listEvt))
	m = updated.(AppModel)

	if len(m.state.SessionPicker.Sessions) != 2 {
		t.Fatalf("expected 2 sessions, got %d", len(m.state.SessionPicker.Sessions))
	}

	// Press Down arrow
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyDown})
	m = updated.(AppModel)
	if m.state.SessionPicker.Selected != 1 {
		t.Fatalf("expected selected index 1, got %d", m.state.SessionPicker.Selected)
	}

	// Press Enter to resume
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = updated.(AppModel)
	if m.state.SessionPicker.Active {
		t.Fatalf("expected SessionPicker to be closed after Enter")
	}

	select {
	case cmd := <-cmdChan:
		if cmd.Type != protocol.CmdListSessions {
			t.Fatalf("expected CmdListSessions first, got %v", cmd.Type)
		}
	default:
		t.Fatalf("expected CmdListSessions command")
	}

	select {
	case cmd := <-cmdChan:
		if cmd.Type != protocol.CmdResumeSession || cmd.SessionID != "s-2" {
			t.Fatalf("expected CmdResumeSession s-2, got %v", cmd)
		}
	default:
		t.Fatalf("expected CmdResumeSession command")
	}
}

func TestSessionPickerDeleteFlow(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 10)
	eventChan := make(chan protocol.EngineEvent, 10)
	app := NewAppModel(cmdChan, eventChan)
	app.state.SessionPicker.Active = true
	app.state.SessionPicker.Sessions = []protocol.SessionMetadata{
		{ID: "s-del", Title: "Delete Me", TurnCount: 1},
	}

	// Press 'd' to trigger delete confirmation
	updated, _ := app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'d'}})
	m := updated.(AppModel)
	if !m.state.SessionPicker.ConfirmDelete {
		t.Fatalf("expected ConfirmDelete to be true")
	}

	// Press 'y' to confirm deletion
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'y'}})
	m = updated.(AppModel)
	if m.state.SessionPicker.ConfirmDelete {
		t.Fatalf("expected ConfirmDelete to be false after confirmation")
	}
	if len(m.state.SessionPicker.Sessions) != 0 {
		t.Fatalf("expected session list to be empty after delete")
	}

	select {
	case cmd := <-cmdChan:
		if cmd.Type != protocol.CmdDeleteSession || cmd.SessionID != "s-del" {
			t.Fatalf("expected CmdDeleteSession s-del, got %v", cmd)
		}
	default:
		t.Fatalf("expected CmdDeleteSession command")
	}
}

func TestSessionPickerRender(t *testing.T) {
	th := NewDefaultTheme()
	state := NewSessionPickerState()
	state.Sessions = []protocol.SessionMetadata{
		{ID: "sess-abc", Title: "Test Session Render", CreatedAt: time.Now(), TurnCount: 8},
	}
	state.Preview["sess-abc"] = []string{"> hello", "● world"}

	viewWide := RenderSessionPicker(state, th, 100, 24)
	if len(viewWide) == 0 {
		t.Fatalf("expected non-empty wide render")
	}

	viewNarrow := RenderSessionPicker(state, th, 60, 24)
	if len(viewNarrow) == 0 {
		t.Fatalf("expected non-empty narrow render")
	}
}
