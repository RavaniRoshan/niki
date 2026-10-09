package tui

import (
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
	tea "github.com/charmbracelet/bubbletea"
)

func TestTieredCtrlCBehavior(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 10)
	eventChan := make(chan protocol.EngineEvent, 10)
	app := NewAppModel(cmdChan, eventChan)

	// Case 1: Non-empty composer text -> clears text without quitting
	app.composer.Input.SetValue("my draft prompt")
	model, cmd := app.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	app = model.(AppModel)
	if cmd != nil {
		t.Errorf("expected no quit cmd when input is non-empty, got %v", cmd)
	}
	if app.composer.Input.Value() != "" {
		t.Fatalf("expected composer to be cleared, got %q", app.composer.Input.Value())
	}
	if app.exitArmed {
		t.Error("exitArmed should be false after clearing text")
	}

	// Case 2: Turn is busy -> sends CmdInterruptTurn without quitting
	app.state.Busy = true
	model, cmd = app.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	app = model.(AppModel)
	if cmd != nil {
		t.Errorf("expected no quit cmd when busy, got %v", cmd)
	}
	select {
	case c := <-cmdChan:
		if c.Type != protocol.CmdInterruptTurn {
			t.Errorf("expected CmdInterruptTurn, got %v", c.Type)
		}
	default:
		t.Error("expected CmdInterruptTurn sent to cmdChan")
	}

	// Case 3: Empty and idle -> first Ctrl+C arms exit
	app.state.Busy = false
	app.exitArmed = false
	model, _ = app.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	app = model.(AppModel)
	if !app.exitArmed {
		t.Fatal("expected exitArmed to be true after first Ctrl+C on empty input")
	}

	// Case 4: Second Ctrl+C while armed -> returns tea.Quit
	model, cmd = app.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	_ = model.(AppModel)
	// cmd should be tea.Quit
	if cmd == nil {
		t.Fatal("expected tea.Quit command on second Ctrl+C")
	}
}

func TestCtrlDBehavior(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 10)
	eventChan := make(chan protocol.EngineEvent, 10)
	app := NewAppModel(cmdChan, eventChan)

	// Case 1: Empty input -> exits cleanly
	model, cmd := app.Update(tea.KeyMsg{Type: tea.KeyCtrlD})
	_ = model.(AppModel)
	if cmd == nil {
		t.Fatal("expected tea.Quit command on Ctrl+D with empty input")
	}

	// Case 2: Non-empty input -> forward deletes char without quitting
	app.composer.Input.SetValue("hello")
	app.composer.Input.SetCursor(1) // at 'e'
	model, cmd = app.Update(tea.KeyMsg{Type: tea.KeyCtrlD})
	app = model.(AppModel)
	if cmd != nil {
		t.Errorf("expected no quit cmd on Ctrl+D with text, got %v", cmd)
	}
	if app.composer.Input.Value() != "hllo" {
		t.Fatalf("expected 'hllo', got %q", app.composer.Input.Value())
	}
}

func TestKillRingAndUndoKeybindings(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 10)
	eventChan := make(chan protocol.EngineEvent, 10)
	app := NewAppModel(cmdChan, eventChan)

	// Test Ctrl+W (kill word backward)
	app.composer.Input.SetValue("foo bar baz")
	app.composer.Input.SetCursor(len("foo bar baz"))
	model, _ := app.Update(tea.KeyMsg{Type: tea.KeyCtrlW})
	app = model.(AppModel)
	if app.composer.Input.Value() != "foo bar " {
		t.Fatalf("expected 'foo bar ', got %q", app.composer.Input.Value())
	}
	if app.composer.KillRing.Yank() != "baz" {
		t.Fatalf("expected yanked 'baz', got %q", app.composer.KillRing.Yank())
	}

	// Test Ctrl+Y (yank back)
	model, _ = app.Update(tea.KeyMsg{Type: tea.KeyCtrlY})
	app = model.(AppModel)
	if app.composer.Input.Value() != "foo bar baz" {
		t.Fatalf("expected 'foo bar baz' after yank, got %q", app.composer.Input.Value())
	}

	// Test Ctrl+K (kill line forward)
	app.composer.Input.SetCursor(4) // after "foo "
	model, _ = app.Update(tea.KeyMsg{Type: tea.KeyCtrlK})
	app = model.(AppModel)
	if app.composer.Input.Value() != "foo " {
		t.Fatalf("expected 'foo ', got %q", app.composer.Input.Value())
	}
	if app.composer.KillRing.Yank() != "bar baz" {
		t.Fatalf("expected yanked 'bar baz', got %q", app.composer.KillRing.Yank())
	}

	// Test Undo (Ctrl+_)
	model, _ = app.Update(tea.KeyMsg{Type: tea.KeyCtrlUnderscore})
	app = model.(AppModel)
	if app.composer.Input.Value() != "foo bar baz" {
		t.Fatalf("expected restored 'foo bar baz' after undo, got %q", app.composer.Input.Value())
	}
}

func TestBracketedPasteCollapsingAndExpansion(t *testing.T) {
	// Create a 15-line paste payload
	longText := strings.Repeat("console.log('line');\n", 15)

	token, isCollapsed := HandlePastedText(longText)
	if !isCollapsed {
		t.Fatal("expected text >10 lines to be collapsed")
	}
	if !strings.HasPrefix(token, "[paste #") {
		t.Fatalf("expected token format [paste #...], got %q", token)
	}

	// Verify expansion restores original string
	expanded := ExpandPasteTokens("Analyze this code:\n" + token)
	if !strings.Contains(expanded, "console.log('line');") {
		t.Fatalf("expanded text must contain original lines, got: %s", expanded)
	}
}
