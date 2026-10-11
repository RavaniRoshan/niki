package tui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
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
	// Hint is the short footer label; the footer renders hints
	// only from this registry (Classic Flow §1.4).
	Hint string
	// States lists the UI states in which the footer shows this
	// hint: "idle", "running", "gate", "verdict". Empty means
	// always. Every rendered hint must dispatch in that state
	// (footer_hints_test.go).
	States []string
	// Key is the canonical message the test presses.
	Key tea.KeyPressMsg
}

// KeyBindings is the single registry of advertised keys.
var KeyBindings = []KeyBinding{
	{ID: "shift+tab", Help: "Cycle mode (Ask When Needed / Always Ask / Plan Mode)", Hint: "shift+tab mode", States: []string{"idle"}, Key: tea.KeyPressMsg{Code: tea.KeyTab, Mod: tea.ModShift}},
	{ID: "ctrl+c", Help: "Clear input / Interrupt", Hint: "ctrl+c stop", States: []string{"running"}, Key: tea.KeyPressMsg{Code: 'c', Mod: tea.ModCtrl}},
	{ID: "esc", Help: "Pause turn (Esc twice stops) / Deny approval / Close palette", Hint: "esc pause", States: []string{"running", "gate"}, Key: tea.KeyPressMsg{Code: tea.KeyEsc}},
	{ID: "ctrl+d", Help: "Exit on empty input", Hint: "", States: nil, Key: tea.KeyPressMsg{Code: 'd', Mod: tea.ModCtrl}},
	{ID: "ctrl+z", Help: "Suspend process", Hint: "", States: nil, Key: tea.KeyPressMsg{Code: 'z', Mod: tea.ModCtrl}},
	{ID: "ctrl+s", Help: "Session manager & switcher", Hint: "", States: nil, Key: tea.KeyPressMsg{Code: 's', Mod: tea.ModCtrl}},
	{ID: "ctrl+g", Help: "Open input in external $EDITOR", Hint: "", States: nil, Key: tea.KeyPressMsg{Code: 'g', Mod: tea.ModCtrl}},
	{ID: "ctrl+o", Help: "Toggle tool output folding", Hint: "ctrl+o expand", States: []string{"idle", "running"}, Key: tea.KeyPressMsg{Code: 'o', Mod: tea.ModCtrl}},
	{ID: "ctrl+p", Help: "Command palette & settings", Hint: "", States: nil, Key: tea.KeyPressMsg{Code: 'p', Mod: tea.ModCtrl}},
	{ID: "ctrl+k", Help: "Actions palette", Hint: "ctrl+k menu", States: []string{"idle"}, Key: tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl}},
	{ID: "?", Help: "Show keys, generated from this registry", Hint: "? help", States: []string{"idle"}, Key: tea.KeyPressMsg{Text: "?", Code: '?'}},
	{ID: "enter", Help: "Submit prompt", Hint: "enter accept", States: []string{"verdict"}, Key: tea.KeyPressMsg{Code: tea.KeyEnter}},
	{ID: "n", Help: "Gate: keep the constraint (the default option)", Hint: "n keep constraint", States: []string{"gate"}, Key: tea.KeyPressMsg{Text: "n", Code: 'n'}},
	{ID: "y", Help: "Gate: allow once", Hint: "y allow once", States: []string{"gate"}, Key: tea.KeyPressMsg{Text: "y", Code: 'y'}},
	{ID: "v", Help: "Gate: view the diff", Hint: "v view diff", States: []string{"gate"}, Key: tea.KeyPressMsg{Text: "v", Code: 'v'}},
	{ID: "d", Help: "Verdict: show the diff", Hint: "d diff", States: []string{"verdict"}, Key: tea.KeyPressMsg{Text: "d", Code: 'd'}},
}

// FooterHintStates names the footer hint states.
const (
	hintIdle    = "idle"
	hintRunning = "running"
	hintGate    = "gate"
	hintVerdict = "verdict"
)

// FooterHints returns the registry entries whose hints show in the
// given state, in registry order. Empty state = always shown.
func FooterHints(state string) []KeyBinding {
	var out []KeyBinding
	for _, k := range KeyBindings {
		if k.Hint == "" {
			continue
		}
		if len(k.States) == 0 || containsStr(k.States, state) {
			out = append(out, k)
		}
	}
	return out
}

func containsStr(list []string, s string) bool {
	for _, v := range list {
		if v == s {
			return true
		}
	}
	return false
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
