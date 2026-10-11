package tui

import (
	"testing"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/protocol"
)

// X2: approval modes — safest focused, Esc denies (permissions
// package), and Never Ask is never the default and never one
// keypress away: cycling skips it, only explicit /yolo confirms it.
func TestNeverAskNeedsExplicitConfirmation(t *testing.T) {
	// Never the default: fresh config and fresh TUI start asking.
	cfg := config.Default()
	if cfg.Permissions.Mode == "full_access" || cfg.Permissions.Mode == "never" || cfg.Permissions.Mode == "yolo" {
		t.Fatalf("[X2] default mode must ask, got %q", cfg.Permissions.Mode)
	}
	t.Logf("[X2] default mode asks: %q", cfg.Permissions.Mode)

	cmdChan := make(chan protocol.EngineCommand, 16)
	eventChan := make(chan protocol.EngineEvent, 16)
	app := NewAppModel(cmdChan, eventChan)

	// Cycle through every mode from the default: Never Ask must
	// never appear.
	seen := map[string]bool{}
	for i := 0; i < 6; i++ {
		nm, _ := app.cycleWorkingMode()
		app = nm
		seen[app.state.PermissionMode] = true
	}
	if seen["full_access"] || seen["yolo"] || seen["never"] {
		t.Fatalf("[X2] cycling reached Never Ask: %v", seen)
	}
	t.Logf("[X2] 6 cycles, Never Ask never appears: %v", seen)

	// Explicit /yolo is the only path: it sets full_access and the
	// engine command carries it.
	app.state.PermissionMode = "manual"
	for len(cmdChan) > 0 {
		<-cmdChan
	}
	_, _ = app.executeSlashCommand("/yolo")
	{
		m2, _ := app.executeSlashCommand("/yolo")
		app = m2.(AppModel)
	}
	if app.state.PermissionMode != "full_access" {
		t.Fatalf("[X2] /yolo must set full_access, got %q", app.state.PermissionMode)
	}
	found := false
	for len(cmdChan) > 0 {
		c := <-cmdChan
		if c.Type == protocol.CmdSetPermissionMode && c.Mode == "full_access" {
			found = true
		}
	}
	if !found {
		t.Fatalf("[X2] /yolo sent no full_access engine command")
	}
	t.Logf("[X2] explicit /yolo confirms Never Ask (engine command sent)")
}
