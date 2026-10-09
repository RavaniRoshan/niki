package tui

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestShiftTabCyclesModes(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 16)
	eventChan := make(chan protocol.EngineEvent, 16)
	m := NewAppModel(cmdChan, eventChan)

	// Default state: agent mode, workspace_write (Ask When Needed)
	if m.state.Mode != "agent" || m.state.PermissionMode != "workspace_write" {
		t.Fatalf("expected default agent/workspace_write, got %s/%s", m.state.Mode, m.state.PermissionMode)
	}

	// 1st Shift+Tab -> Agent Always Ask (manual)
	m2, _ := m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
	m = m2.(AppModel)
	if m.state.Mode != "agent" || m.state.PermissionMode != "manual" {
		t.Fatalf("expected agent/manual, got %s/%s", m.state.Mode, m.state.PermissionMode)
	}
	cmd1 := <-cmdChan
	if cmd1.Type != protocol.CmdSetPermissionMode || cmd1.Mode != "manual" {
		t.Fatalf("expected CmdSetPermissionMode manual, got %+v", cmd1)
	}

	// 2nd Shift+Tab -> Agent Never Ask (full_access / Yolo)
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
	m = m2.(AppModel)
	if m.state.Mode != "agent" || m.state.PermissionMode != "full_access" {
		t.Fatalf("expected agent/full_access, got %s/%s", m.state.Mode, m.state.PermissionMode)
	}
	cmd2 := <-cmdChan
	if cmd2.Type != protocol.CmdSetPermissionMode || cmd2.Mode != "full_access" {
		t.Fatalf("expected CmdSetPermissionMode full_access, got %+v", cmd2)
	}

	// 3rd Shift+Tab -> Plan Mode (readonly)
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
	m = m2.(AppModel)
	if m.state.Mode != "plan" || m.state.PermissionMode != "readonly" {
		t.Fatalf("expected plan/readonly, got %s/%s", m.state.Mode, m.state.PermissionMode)
	}
	cmd3a := <-cmdChan
	cmd3b := <-cmdChan
	if cmd3a.Type != protocol.CmdSetPlanMode || cmd3a.Mode != "plan" {
		t.Fatalf("expected CmdSetPlanMode plan, got %+v", cmd3a)
	}
	if cmd3b.Type != protocol.CmdSetPermissionMode || cmd3b.Mode != "readonly" {
		t.Fatalf("expected CmdSetPermissionMode readonly, got %+v", cmd3b)
	}

	// 4th Shift+Tab -> Cycles back to Agent Ask When Needed
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyShiftTab})
	m = m2.(AppModel)
	if m.state.Mode != "agent" || m.state.PermissionMode != "workspace_write" {
		t.Fatalf("expected agent/workspace_write, got %s/%s", m.state.Mode, m.state.PermissionMode)
	}
	cmd4a := <-cmdChan
	cmd4b := <-cmdChan
	if cmd4a.Type != protocol.CmdSetPlanMode || cmd4a.Mode != "false" {
		t.Fatalf("expected CmdSetPlanMode false, got %+v", cmd4a)
	}
	if cmd4b.Type != protocol.CmdSetPermissionMode || cmd4b.Mode != "workspace_write" {
		t.Fatalf("expected CmdSetPermissionMode workspace_write, got %+v", cmd4b)
	}
}

func TestShellFastPathDirectExecution(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 16)
	eventChan := make(chan protocol.EngineEvent, 16)
	m := NewAppModel(cmdChan, eventChan)

	// Type `! echo hello_niki_fast_path` and submit with Enter
	m.composer.Input.SetValue("! echo hello_niki_fast_path")
	m2, cmd := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)

	// Must NOT send an LLM prompt turn on cmdChan!
	select {
	case c := <-cmdChan:
		t.Fatalf("unexpected engine command on shell execution: %+v", c)
	default:
	}

	// Should be busy running shell
	if !m.state.Busy {
		t.Fatal("expected model to be busy executing shell")
	}

	// Run the tea.Cmd returned
	if cmd == nil {
		t.Fatal("expected non-nil tea.Cmd for shell execution")
	}
	msg := cmd()
	sMsg, ok := msg.(shellResultMsg)
	if !ok {
		t.Fatalf("expected shellResultMsg, got %T", msg)
	}
	if sMsg.ExitCode != 0 {
		t.Fatalf("expected exit code 0, got %d (err: %v)", sMsg.ExitCode, sMsg.Err)
	}
	if !strings.Contains(sMsg.Output, "hello_niki_fast_path") {
		t.Fatalf("expected output to contain hello_niki_fast_path, got %q", sMsg.Output)
	}

	// Feed result msg back to Update
	m3, _ := m.Update(sMsg)
	m = m3.(AppModel)

	if m.state.Busy {
		t.Fatal("expected model to not be busy after shellResultMsg")
	}

	// Check history cells
	var foundInput, foundOutput, foundStatus bool
	for _, cell := range m.history.Cells {
		if cell.Role == "shell_input" && strings.Contains(cell.Text, "echo hello_niki_fast_path") {
			foundInput = true
		}
		if cell.Role == "shell_output" && strings.Contains(cell.Text, "hello_niki_fast_path") {
			foundOutput = true
		}
		if cell.Role == "shell_status" && strings.Contains(cell.Text, "exit 0") {
			foundStatus = true
		}
	}

	if !foundInput {
		t.Fatal("missing shell_input history cell")
	}
	if !foundOutput {
		t.Fatal("missing shell_output history cell")
	}
	if !foundStatus {
		t.Fatal("missing shell_status history cell")
	}
}

func TestShellPromptSwitching(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 16)
	eventChan := make(chan protocol.EngineEvent, 16)
	m := NewAppModel(cmdChan, eventChan)

	if m.composer.Input.Prompt != "> " {
		t.Fatalf("expected default prompt '> ', got %q", m.composer.Input.Prompt)
	}

	// Simulate typing !
	m.composer.Input.SetValue("!")
	m2, _ := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'!'}})
	m = m2.(AppModel)

	if m.composer.Input.Prompt != "$ " {
		t.Fatalf("expected shell mode prompt '$ ', got %q", m.composer.Input.Prompt)
	}

	// Simulate deleting back to empty
	m.composer.Input.SetValue("")
	m3, _ := m.Update(tea.KeyMsg{Type: tea.KeyBackspace})
	m = m3.(AppModel)

	if m.composer.Input.Prompt != "> " {
		t.Fatalf("expected restored prompt '> ', got %q", m.composer.Input.Prompt)
	}
}

func TestTruthfulFooterView(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 16)
	eventChan := make(chan protocol.EngineEvent, 16)
	m := NewAppModel(cmdChan, eventChan)
	m.state.Width = 100

	view := m.footerView()

	// 1. Must NOT contain fake hardcoded "thinking: high"
	if strings.Contains(view, "thinking: high") {
		t.Fatalf("footer must not render fake hardcoded thinking: high: %s", view)
	}

	// 2. Must contain authentic mode badge "[Ask When Needed]"
	if !strings.Contains(view, "Ask When Needed") {
		t.Fatalf("footer should show Ask When Needed by default: %s", view)
	}

	// 3. Must contain authentic shortcuts
	if !strings.Contains(view, "shift+tab mode") || !strings.Contains(view, "! shell") {
		t.Fatalf("footer should show truthful shortcut hints: %s", view)
	}

	// 4. Test with thinking effort explicitly configured
	m.state.ThinkingEffort = "medium"
	view2 := m.footerView()
	if !strings.Contains(view2, "thinking: medium") {
		t.Fatalf("footer should render configured thinking: medium: %s", view2)
	}

	// 5. Test Plan mode footer
	m.state.Mode = "plan"
	view3 := m.footerView()
	if !strings.Contains(view3, "Plan: Read-Only") {
		t.Fatalf("footer should show Plan: Read-Only in plan mode: %s", view3)
	}
}

func TestSlashModeCommands(t *testing.T) {
	cmdChan := make(chan protocol.EngineCommand, 16)
	eventChan := make(chan protocol.EngineEvent, 16)
	m := NewAppModel(cmdChan, eventChan)

	// /yolo
	m.composer.Input.SetValue("/yolo")
	m2, _ := m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if m.state.PermissionMode != "full_access" {
		t.Fatalf("expected full_access after /yolo, got %s", m.state.PermissionMode)
	}

	// /plan
	m.composer.Input.SetValue("/plan")
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if m.state.Mode != "plan" {
		t.Fatalf("expected plan after /plan, got %s", m.state.Mode)
	}

	// /auto
	m.composer.Input.SetValue("/auto")
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if m.state.PermissionMode != "workspace_write" {
		t.Fatalf("expected workspace_write after /auto, got %s", m.state.PermissionMode)
	}

	// /thinking high
	m.composer.Input.SetValue("/thinking high")
	m2, _ = m.Update(tea.KeyMsg{Type: tea.KeyEnter})
	m = m2.(AppModel)
	if m.state.ThinkingEffort != "high" {
		t.Fatalf("expected thinking effort high, got %s", m.state.ThinkingEffort)
	}
}
