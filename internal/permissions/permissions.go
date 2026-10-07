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
