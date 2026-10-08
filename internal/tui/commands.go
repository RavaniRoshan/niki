package tui

import "strings"

// CommandDef represents a registered slash command.
type CommandDef struct {
	Name        string
	Description string
}

// CoreSlashCommands is the single registry of slash commands feeding help, suggestions, and dispatch.
var CoreSlashCommands = []CommandDef{
	{Name: "/help", Description: "Show available commands and keybindings"},
	{Name: "/model", Description: "Switch or view current model"},
	{Name: "/clear", Description: "Clear the active conversation history"},
	{Name: "/new", Description: "Start a fresh session"},
	{Name: "/compact", Description: "Compact conversation context"},
	{Name: "/resume", Description: "Resume a previous session"},
	{Name: "/status", Description: "Show engine and MCP readiness status"},
	{Name: "/diff", Description: "Show pending and applied file diffs"},
	{Name: "/plan", Description: "Toggle read-only plan mode exploration"},
	{Name: "/rewind", Description: "Rewind code files or conversation to a checkpoint"},
	{Name: "/agents", Description: "View active subagents and hierarchy"},
	{Name: "/palette", Description: "Command palette to search all commands and actions"},
	{Name: "/cost", Description: "Show session token usage and accumulated cost"},
	{Name: "/theme", Description: "Switch color theme (default, dark, light, monochrome)"},
	{Name: "/doctor", Description: "Check system health and environment"},
	{Name: "/reload", Description: "Reload config without restart"},
	{Name: "/debug", Description: "Toggle debug telemetry overlay"},
	{Name: "/quit", Description: "Exit Niki"},
}

// SuggestSlashCommands filters CoreSlashCommands based on user prefix.
func SuggestSlashCommands(query string) []string {
	q := strings.TrimPrefix(query, "/")
	var out []string
	for _, c := range CoreSlashCommands {
		if strings.Contains(strings.TrimPrefix(c.Name, "/"), q) {
			out = append(out, c.Name+" — "+c.Description)
		}
	}
	return out
}
