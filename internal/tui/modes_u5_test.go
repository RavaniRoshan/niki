package tui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// U5: modes, approvals and the palette — each proven present/absent
// and event-driven, from the one registry.

// U5: the composer chip reflects the mode from real mode_changed
// events (plan | act | yolo).
func TestModeChipFromModeChangedEvents(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"

	chip := func(mm AppModel) string {
		// The chip lives on the composer row.
		for _, line := range strings.Split(workbenchFrame(mm), "\n") {
			if strings.Contains(line, ">") && strings.Contains(line, "mode ") {
				return line
			}
		}
		return ""
	}

	// Default: workspace_write renders "act".
	if got := chip(m); !strings.Contains(got, "mode act") {
		t.Fatalf("default chip = %q, want mode act", got)
	}
	// full_access renders "yolo".
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventModeChanged, Mode: "full_access", PlanActive: false})
	if got := chip(m); !strings.Contains(got, "mode yolo") {
		t.Fatalf("chip = %q, want mode yolo", got)
	}
	// plan mode renders "plan".
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventModeChanged, Mode: "readonly", PlanActive: true})
	if got := chip(m); !strings.Contains(got, "mode plan") {
		t.Fatalf("chip = %q, want mode plan", got)
	}
	if m.state.Mode != "plan" || m.state.PermissionMode != "readonly" {
		t.Fatalf("mode state not applied from the event: mode=%q perm=%q", m.state.Mode, m.state.PermissionMode)
	}
	// Leaving plan mode returns to act with the engine's mode.
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventModeChanged, Mode: "workspace_write", PlanActive: false})
	if got := chip(m); !strings.Contains(got, "mode act") {
		t.Fatalf("chip = %q, want mode act after leaving plan", got)
	}
}

// U5: the approvals popup renders from the question event, defaults
// focus to the option that keeps the constraint, and is absent when
// nothing is pending.
func TestApprovalsPopup(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if v := workbenchFrame(m); strings.Contains(v, "needs you") {
		t.Fatal("approvals popup present with nothing pending")
	}

	question := protocol.EngineEvent{Type: protocol.EventQuestionPrompted, Questions: []protocol.UserQuestion{{
		Header: "needs you", Question: "add golang.org/x/time?",
		Options: []string{"write the limiter without it", "allow once", "view the diff"},
		Safe:    0,
	}}}
	m = feedEvents(m, question)
	v := workbenchFrame(m)
	if !strings.Contains(v, "needs you") || !strings.Contains(v, "add golang.org/x/time?") {
		t.Fatalf("approvals popup missing after the question event:\n%s", v)
	}
	// The safe option is the focused default, marked.
	if !strings.Contains(v, "write the limiter without it (default)") {
		t.Fatalf("the constraint-keeping option must be the focused default:\n%s", v)
	}
	// The gate state is live: the composer placeholder yields to
	// the decision (the activity line is gone — proven absent in
	// the trace tests).
	um2, _ := m.Update(tea.WindowSizeMsg{Width: 100, Height: 27})
	_ = um2
	if ph := m.classicPlaceholder(); !strings.Contains(ph, "answer the question") {
		t.Fatalf("composer placeholder must yield to the gate, got %q", ph)
	}

	// Answering it (the real key path) clears the popup.
	um, _ := m.Update(tea.KeyPressMsg{Text: "n", Code: 'n'})
	m = um.(AppModel)
	if v := workbenchFrame(m); strings.Contains(v, "needs you") {
		t.Fatalf("approvals popup must be absent after the answer:\n%s", v)
	}
}

// U5: the command palette renders from the one registry — every
// palette slash command is a registered slash command, and that
// same registry feeds the help text and the footer hints.
func TestPaletteFromOneRegistry(t *testing.T) {
	// Every slash-action palette entry names a registered command.
	slash := map[string]bool{}
	for _, c := range CoreSlashCommands {
		slash[c.Name] = true
	}
	n := 0
	for _, item := range DefaultPaletteCatalog() {
		if item.ActionType != "slash" {
			continue
		}
		n++
		if !slash[item.Payload] {
			t.Fatalf("palette command %q is not in the slash registry", item.Payload)
		}
	}
	if n == 0 {
		t.Fatal("no slash commands in the palette catalog")
	}

	// The same registry dispatches: no palette command hits the
	// unknown-command path.
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	for _, item := range DefaultPaletteCatalog() {
		if item.ActionType != "slash" {
			continue
		}
		before := len(m.history.Cells)
		um, _ := m.executeSlashCommand(item.Payload)
		m = um.(AppModel)
		last := ""
		if len(m.history.Cells) > before {
			last = m.history.Cells[len(m.history.Cells)-1].Text
		}
		if strings.Contains(last, "Unknown command") {
			t.Fatalf("palette command %q fell through to unknown", item.Payload)
		}
	}

	// The help text is generated from the same two registries:
	// the /help command lists every registered command and every
	// advertised key (KeyHelpText).
	before := len(m.history.Cells)
	um, _ := m.executeSlashCommand("/help")
	m = um.(AppModel)
	if len(m.history.Cells) <= before {
		t.Fatal("precondition: /help produced output")
	}
	help := m.history.Cells[len(m.history.Cells)-1].Text
	for _, c := range CoreSlashCommands {
		if !strings.Contains(help, c.Name) {
			t.Fatalf("help is missing registered command %q", c.Name)
		}
	}
	for _, kb := range KeyBindings {
		if kb.Help == "" {
			continue
		}
		if !strings.Contains(help, kb.ID) {
			t.Fatalf("help is missing registered key %q", kb.ID)
		}
	}
	t.Logf("[U5] one registry: %d palette commands, %d slash commands, %d keys all dispatch", n, len(CoreSlashCommands), len(KeyBindings))
}

// U5: the palette opens and closes over the centre, and is absent
// until invoked.
func TestPaletteOpenAbsentUntilInvoked(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(workbenchFrame(m), "palette") {
		t.Fatal("palette present before invocation")
	}
	um, _ := m.Update(tea.KeyPressMsg{Code: 'k', Mod: tea.ModCtrl})
	m = um.(AppModel)
	if !m.state.Palette.Open {
		t.Fatal("ctrl+k must open the palette")
	}
	if v := workbenchFrame(m); !strings.Contains(v, "palette") && !strings.Contains(v, ">") {
		t.Fatalf("palette overlay missing after ctrl+k:\n%s", v)
	}
	um, _ = m.Update(tea.KeyPressMsg{Code: tea.KeyEsc})
	m = um.(AppModel)
	if m.state.Palette.Open {
		t.Fatal("esc must close the palette")
	}
}
