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
	{Name: "/btw", Description: "Docked mini-agent for quick queries without context pollution (/btw <query>)"},
	{Name: "/plan", Description: "Toggle read-only plan mode exploration"},
	{Name: "/rewind", Description: "Rewind code files or conversation to a checkpoint"},
	{Name: "/unrevert", Description: "Redo / restore files from the pre-rewind state"},
	{Name: "/agents", Description: "View active subagents and hierarchy"},
	{Name: "/explain", Description: "Explain code with file:line citations (/explain <symbol|file|question>)"},
	{Name: "/palette", Description: "Command palette to search all commands and actions (or Ctrl+P)"},
	{Name: "/settings", Description: "Open settings & configuration palette"},
	{Name: "/connect", Description: "Connect provider API key (/connect <anthropic|openai|openrouter>)"},
	{Name: "/spinner", Description: "Switch spinner animation (/spinner <bloom|braille|sweep|pulse>)"},
	{Name: "/cost", Description: "Show session token usage and accumulated cost"},
	{Name: "/theme", Description: "Switch color theme (default, dark, light, monochrome)"},
	{Name: "/doctor", Description: "Check system health and environment"},
	{Name: "/reload", Description: "Reload config without restart"},
	{Name: "/debug", Description: "Toggle debug telemetry overlay"},
	{Name: "/sessions", Description: "Interactive session browser and rollouts (or Ctrl+S)"},
	{Name: "/editor", Description: "Open external $EDITOR for multi-line drafting (or Ctrl+G)"},
	{Name: "/export", Description: "Export session transcript to Markdown or HTML (/export [markdown|html])"},
	{Name: "/quit", Description: "Exit NikiCode"},
}

// SurfaceClaims maps slash commands to CLAIMS.md rows for claimcheck:
// /explain proves C4; the rest are foundation capabilities (C17).
func SurfaceClaims() map[string]string {
	return map[string]string{"/explain": "C4"}
}

// SurfaceStrings lists TUI user-facing strings with claim tags.
func SurfaceStrings() []string {
	claims := SurfaceClaims()
	var out []string
	for _, c := range CoreSlashCommands {
		tag, ok := claims[c.Name]
		if !ok {
			tag = "C17"
		}
		out = append(out, "slash:"+c.Name+" | "+c.Description+" ["+tag+"]")
	}
	out = append(out, "tui:brand | "+BrandLine()+" [C1]")
	out = append(out, "tui:wordmark-tag | personal coding agent [C1]")
	return out
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
