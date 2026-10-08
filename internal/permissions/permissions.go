package permissions

import (
	"strings"
)

type Mode string

const (
	ModeReadOnly      Mode = "readonly"
	ModeWorkspaceWrite Mode = "workspace_write"
	ModeFullAccess    Mode = "full_access"
)

type Guard struct {
	Mode Mode
	// SandboxedShell permits the shell tool in
	// read-only mode when a real sandbox backend
	// enforces the read-only boundary at the OS
	// level: writes land nowhere outside the
	// bound workspace, so the mode's guarantee
	// still holds (S1 auto-allow).
	SandboxedShell bool
	AuditLog       []DecisionRecord
}

func NewGuard(mode Mode) *Guard { return &Guard{Mode: mode} }

var readOnlyTools = map[string]bool{"read_file": true, "glob": true, "grep": true}

func (g *Guard) Allow(toolName string) bool {
	switch g.Mode {
	case ModeFullAccess:
		return true
	case ModeWorkspaceWrite:
		return true
	case ModeReadOnly:
		if toolName == "shell" && g.SandboxedShell {
			return true
		}
		return readOnlyTools[toolName]
	}
	return false
}

// ClassifyCommand scores a shell command for risk.
func ClassifyCommand(cmd string) string {
	c := strings.ToLower(cmd)
	switch {
	case strings.Contains(c, "rm -rf"), strings.Contains(c, "sudo"), strings.Contains(c, "mkfs"):
		return "dangerous"
	case strings.Contains(c, "git push"), strings.Contains(c, "git reset --hard"):
		return "elevated"
	case strings.Contains(c, "curl"), strings.Contains(c, "wget"):
		return "network"
	default:
		return "safe"
	}
}

type ApprovalOption int

const (
	OptionDeny ApprovalOption = iota // 0: Safest option, focused by default
	OptionAllowOnce
	OptionAllowAlways
)

type DecisionRecord struct {
	Tool     string
	Command  string
	Decision ApprovalOption
	Reason   string
}

type ApprovalPrompt struct {
	ToolName string
	Command  string
	Focused  ApprovalOption // Defaults to OptionDeny (safest)
}

func NewApprovalPrompt(tool, cmd string) *ApprovalPrompt {
	return &ApprovalPrompt{
		ToolName: tool,
		Command:  cmd,
		Focused:  OptionDeny, // Safest option focused by default
	}
}

// HandleKey processes key events: Esc always denies.
func (p *ApprovalPrompt) HandleKey(key string) ApprovalOption {
	switch strings.ToLower(key) {
	case "esc", "n":
		return OptionDeny
	case "enter":
		return p.Focused
	case "1":
		return OptionDeny
	case "2":
		return OptionAllowOnce
	case "3":
		return OptionAllowAlways
	default:
		return OptionDeny // Fail-closed
	}
}

func (g *Guard) LogDecision(tool, cmd string, decision ApprovalOption, reason string) {
	g.AuditLog = append(g.AuditLog, DecisionRecord{
		Tool:     tool,
		Command:  cmd,
		Decision: decision,
		Reason:   reason,
	})
}
