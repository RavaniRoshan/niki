package tui

import (
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// H2: in-app help is generated from the same registry as the keys —
// every advertised key dispatches (observable state change, history
// append, channel send, or returned command).
func TestEveryAdvertisedKeyDispatches(t *testing.T) {
	for _, kb := range KeyBindings {
		cmdChan := make(chan protocol.EngineCommand, 16)
		eventChan := make(chan protocol.EngineEvent, 16)
		app := NewAppModel(cmdChan, eventChan)
		beforeHist := len(app.history.Cells)
		beforeMode := app.state.PermissionMode

		// Per-key arming so the dispatch is observable.
		switch kb.ID {
		case "esc":
			app.state.Palette.Open = true
			app.state.Palette.Mode = "palette"
		case "enter":
			app.composer.Input.SetValue("hello h2")
		case "ctrl+g":
			t.Setenv("EDITOR", "true")
		case "n", "y", "v":
			// Gate keys dispatch on an open gate; the default
			// unconfigured model arms the setup choices (k/o/d)
			// and ? types only on an empty composer.
			app.state.Gate = GateState{
				Active: true, Action: "arm",
				Options: []GateOption{
					{Key: "n", Label: "keep the constraint", Safe: true},
					{Key: "y", Label: "allow once"},
					{Key: "v", Label: "view the diff"},
				},
			}
			app.state.QuestionModal = QuestionState{Active: true, Questions: []protocol.UserQuestion{{
				Question: "arm", Options: []string{"keep the constraint", "allow once", "view the diff"}, Safe: 0,
			}}}
		}

		model, cmd := app.Update(kb.Key)
		app = model.(AppModel)

		dispatched := cmd != nil ||
			len(app.history.Cells) != beforeHist ||
			app.state.PermissionMode != beforeMode ||
			len(cmdChan) > 0
		// Key-specific state proofs (stronger than generic change).
		switch kb.ID {
		case "shift+tab":
			dispatched = app.state.PermissionMode != beforeMode
		case "esc":
			dispatched = !app.state.Palette.Open
		case "ctrl+d":
			dispatched = cmd != nil // tea.Quit on empty input
		case "ctrl+z":
			dispatched = cmd != nil // tea.Suspend
		case "ctrl+s":
			dispatched = app.state.SessionPicker.Active
		case "ctrl+g":
			dispatched = cmd != nil
		case "ctrl+p", "ctrl+k":
			dispatched = app.state.Palette.Open
		case "ctrl+o":
			dispatched = app.state.ExpandToolOutput
		case "?":
			dispatched = len(app.history.Cells) != beforeHist
		case "k", "o", "d":
			dispatched = cmd != nil || len(app.history.Cells) != beforeHist || len(cmdChan) > 0
		case "n", "y", "v":
			dispatched = !app.state.Gate.Active || len(cmdChan) > 0
		case "ctrl+c":
			dispatched = len(app.history.Cells) != beforeHist
		case "enter":
			dispatched = len(app.history.Cells) != beforeHist
		}
		if !dispatched {
			t.Fatalf("[H2] advertised key %q did not dispatch", kb.ID)
		}
		t.Logf("[H2] key %-10s dispatches", kb.ID)
	}

	// The /help text is generated from this registry.
	if len(KeyBindings) != 16 {
		t.Fatalf("[H2] registry = %d keys, help and test assume 16", len(KeyBindings))
	}
}
