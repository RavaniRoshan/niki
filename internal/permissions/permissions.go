package permissions

import (
	"fmt"
	"strings"
)

type Mode string

const (
	ModeReadOnly       Mode = "readonly"
	ModeWorkspaceWrite Mode = "workspace_write"
	ModeFullAccess     Mode = "full_access"
	ModeManual         Mode = "manual"
)

type Guard struct {
	Mode           Mode
	PlanMode       bool
	SandboxedShell bool
	AuditLog       []DecisionRecord
}

func NewGuard(mode Mode) *Guard { return &Guard{Mode: mode} }

func (g *Guard) EnterPlanMode() {
	g.PlanMode = true
}

func (g *Guard) ExitPlanMode(approved bool) error {
	if !approved {
		return fmt.Errorf("explicit user approval is required to exit plan mode")
	}
	g.PlanMode = false
	return nil
}

func (g *Guard) InPlanMode() bool {
	return g.PlanMode
}

var readOnlyTools = map[string]bool{
	"read_file":         true,
	"glob":              true,
	"grep":              true,
	"web_search":        true,
	"web_fetch":         true,
	"view_image":        true,
	"tool_search":       true,
	"bash_output":       true,
	"ask_user_question": true,
	"git_status":        true,
	"git_blame":         true,
	"git_log":           true,
	"git_review":        true,
	"git_changelog":     true,
	"git_diff_summary":  true,
	"git_pr_summary":    true,
	"symbol_search":     true,
}

// IsReadOnlyTool reports whether a tool is known to be read-only.
func IsReadOnlyTool(toolName string) bool {
	return readOnlyTools[toolName]
}

func (g *Guard) Allow(toolName string) bool {
	// Plan mode: strictly read-only exploration state; withhold write & exec tools
	if g.PlanMode {
		return readOnlyTools[toolName]
	}

	switch g.Mode {
	case ModeFullAccess:
		return true
	case ModeWorkspaceWrite:
		return true
	case ModeManual:
		return readOnlyTools[toolName]
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
