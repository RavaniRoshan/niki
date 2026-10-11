package tui

import (
	"strings"

	tea "github.com/charmbracelet/bubbletea"
)

// KeyBinding is one advertised key (H2): the help text and the key
// are registered together, so the in-app help can only advertise
// keys that exist in this registry. The dispatch test
// (keys_h2_test.go) presses every entry and requires observable
// dispatch.
type KeyBinding struct {
	// ID is the advertised name, e.g. "shift+tab".
	ID string
	// Help is the one-line description shown by /help.
	Help string
	// Key is the canonical message the test presses.
	Key tea.KeyMsg
}

// KeyBindings is the single registry of advertised keys.
var KeyBindings = []KeyBinding{
	{ID: "shift+tab", Help: "Cycle mode (Ask When Needed / Always Ask / Plan Mode)", Key: tea.KeyMsg{Type: tea.KeyShiftTab}},
	{ID: "ctrl+c", Help: "Clear input / Interrupt", Key: tea.KeyMsg{Type: tea.KeyCtrlC}},
	{ID: "esc", Help: "Pause turn (Esc twice stops) / Deny approval / Close palette", Key: tea.KeyMsg{Type: tea.KeyEsc}},
	{ID: "ctrl+d", Help: "Exit on empty input", Key: tea.KeyMsg{Type: tea.KeyCtrlD}},
	{ID: "ctrl+z", Help: "Suspend process", Key: tea.KeyMsg{Type: tea.KeyCtrlZ}},
	{ID: "ctrl+s", Help: "Session manager & switcher", Key: tea.KeyMsg{Type: tea.KeyCtrlS}},
	{ID: "ctrl+g", Help: "Open input in external $EDITOR", Key: tea.KeyMsg{Type: tea.KeyCtrlG}},
	{ID: "ctrl+o", Help: "Toggle tool output folding", Key: tea.KeyMsg{Type: tea.KeyCtrlO}},
	{ID: "ctrl+p", Help: "Command palette & settings", Key: tea.KeyMsg{Type: tea.KeyCtrlP}},
	{ID: "enter", Help: "Submit prompt", Key: tea.KeyMsg{Type: tea.KeyEnter}},
}

// KeyHelpText renders the key section of /help from the registry —
// the help can never advertise a key that is not registered here.
func KeyHelpText() string {
	var b strings.Builder
	for _, k := range KeyBindings {
		b.WriteString("  " + k.ID + ": " + k.Help + "\n")
	}
	return b.String()
}
