package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/paths"
)

// PaletteCategory classifies commands in the palette.
type PaletteCategory string

const (
	CatModel    PaletteCategory = "MODEL"
	CatSubagent PaletteCategory = "SUBAGENTS"
	CatAuth     PaletteCategory = "AUTH"
	CatMCP      PaletteCategory = "MCP"
	CatMode     PaletteCategory = "SAFETY"
	CatVisual   PaletteCategory = "VISUAL"
	CatAction   PaletteCategory = "ACTION"
)

// PaletteItem represents one selectable command or setting.
type PaletteItem struct {
	ID          string
	Category    PaletteCategory
	Title       string
	Description string
	ActionType  string // "set_model", "set_secondary_model", "connect_prompt", "custom_model_prompt", "custom_endpoint_prompt", "mcp_add_prompt", "set_mode", "set_theme", "set_spinner", "slash"
	Payload     string
}

// PaletteState holds interactive state for the Ctrl+P command palette modal.
type PaletteState struct {
	Open            bool
	Mode            string // "palette", "connect", "custom_model", "custom_endpoint", "mcp_add"
	Query           string
	Selected        int
	ConnectProvider string
	ConnectKey      string
	InputBuffer     string
	InputStep       int
	MCPName         string
	MCPCmd          string
	MCPArgs         string
}

// DefaultPaletteCatalog returns the comprehensive list of actions available via Ctrl+P.
func DefaultPaletteCatalog() []PaletteItem {
	items := []PaletteItem{
		// Models
		{
			ID:          "model-claude-3-5",
			Category:    CatModel,
			Title:       "Switch to Claude 3.5 Sonnet",
			Description: "Anthropic flagship model · 200k context",
			ActionType:  "set_model",
			Payload:     "anthropic:claude-3-5-sonnet",
		},
		{
			ID:          "model-claude-3-7",
			Category:    CatModel,
			Title:       "Switch to Claude 3.7 Sonnet",
			Description: "Anthropic hybrid reasoning model",
			ActionType:  "set_model",
			Payload:     "anthropic:claude-3-7-sonnet",
		},
		{
			ID:          "model-gpt-4o",
			Category:    CatModel,
			Title:       "Switch to GPT-4o",
			Description: "OpenAI flagship multimodal model",
			ActionType:  "set_model",
			Payload:     "openai:gpt-4o",
		},
		{
			ID:          "model-gpt-4o-mini",
			Category:    CatModel,
			Title:       "Switch to GPT-4o-mini",
			Description: "Fast, cost-efficient OpenAI model",
			ActionType:  "set_model",
			Payload:     "openai:gpt-4o-mini",
		},
		{
			ID:          "model-deepseek",
			Category:    CatModel,
			Title:       "Switch to DeepSeek Chat",
			Description: "DeepSeek V3 API (OpenAI-compatible)",
			ActionType:  "set_model",
			Payload:     "openai:deepseek-chat",
		},
		{
			ID:          "model-ollama",
			Category:    CatModel,
			Title:       "Switch to Local Ollama (llama3)",
			Description: "Local inference at http://localhost:11434",
			ActionType:  "set_model",
			Payload:     "openai:llama3",
		},
		{
			ID:          "model-openrouter",
			Category:    CatModel,
			Title:       "Switch to OpenRouter (Claude 3.5)",
			Description: "Access Anthropic and open models via OpenRouter gateway",
			ActionType:  "set_model",
			Payload:     "openrouter:anthropic/claude-3.5-sonnet",
		},
		{
			ID:          "model-custom",
			Category:    CatModel,
			Title:       "Set Custom Model Name…",
			Description: "Configure any custom model identifier interactively",
			ActionType:  "custom_model_prompt",
			Payload:     "",
		},
		{
			ID:          "model-endpoint",
			Category:    CatModel,
			Title:       "Set Custom Base URL…",
			Description: "Configure local or enterprise inference endpoint URL",
			ActionType:  "custom_endpoint_prompt",
			Payload:     "",
		},

		// Subagents (Secondary Model Pool)
		{
			ID:          "subagent-haiku",
			Category:    CatSubagent,
			Title:       "Subagent Model: Claude 3.5 Haiku",
			Description: "Fast & cost-efficient background subagents",
			ActionType:  "set_secondary_model",
			Payload:     "anthropic:claude-3-5-haiku",
		},
		{
			ID:          "subagent-mini",
			Category:    CatSubagent,
			Title:       "Subagent Model: GPT-4o-mini",
			Description: "Lightweight subagent pool",
			ActionType:  "set_secondary_model",
			Payload:     "openai:gpt-4o-mini",
		},
		{
			ID:          "subagent-deepseek",
			Category:    CatSubagent,
			Title:       "Subagent Model: DeepSeek Chat",
			Description: "Economical high-performance subagents",
			ActionType:  "set_secondary_model",
			Payload:     "openai:deepseek-chat",
		},

		// Auth & Connect
		{
			ID:          "connect-anthropic",
			Category:    CatAuth,
			Title:       "Connect Anthropic API Key",
			Description: "Set ANTHROPIC_API_KEY in ~/.config/nikicode/nikicode.toml",
			ActionType:  "connect_prompt",
			Payload:     "anthropic",
		},
		{
			ID:          "connect-openai",
			Category:    CatAuth,
			Title:       "Connect OpenAI API Key",
			Description: "Set OPENAI_API_KEY in ~/.config/nikicode/nikicode.toml",
			ActionType:  "connect_prompt",
			Payload:     "openai",
		},
		{
			ID:          "connect-openrouter",
			Category:    CatAuth,
			Title:       "Connect OpenRouter API Key",
			Description: "Access 100+ models via OpenRouter endpoint",
			ActionType:  "connect_prompt",
			Payload:     "openrouter",
		},
		{
			ID:          "connect-deepseek",
			Category:    CatAuth,
			Title:       "Connect DeepSeek API Key",
			Description: "Set DEEPSEEK_API_KEY for DeepSeek V3 and R1",
			ActionType:  "connect_prompt",
			Payload:     "deepseek",
		},

		// MCP Server Management
		{
			ID:          "mcp-add",
			Category:    CatMCP,
			Title:       "Add New MCP Server…",
			Description: "Configure STDIO command/executable MCP server",
			ActionType:  "mcp_add_prompt",
			Payload:     "",
		},
		{
			ID:          "mcp-refresh",
			Category:    CatMCP,
			Title:       "Refresh MCP Tools Catalog",
			Description: "Re-query active MCP tools without restarting session",
			ActionType:  "slash",
			Payload:     "/status",
		},

		// Safety & Modes
		{
			ID:          "toggle-plan",
			Category:    CatMode,
			Title:       "Toggle Plan Mode (Read-Only)",
			Description: "Explore codebase without modifying files or executing commands (Shift+Tab)",
			ActionType:  "slash",
			Payload:     "/plan",
		},
		{
			ID:          "perm-workspace",
			Category:    CatMode,
			Title:       "Set Permission: Ask When Needed",
			Description: "Allow tools to write inside workspace roots (standard mode)",
			ActionType:  "set_mode",
			Payload:     "workspace_write",
		},
		{
			ID:          "perm-manual",
			Category:    CatMode,
			Title:       "Set Permission: Always Ask",
			Description: "Strict safety: require confirmation on all modifications",
			ActionType:  "set_mode",
			Payload:     "manual",
		},
		{
			ID:          "perm-full",
			Category:    CatMode,
			Title:       "Set Permission: Never Ask (Yolo)",
			Description: "Unrestricted execution without confirmation prompts",
			ActionType:  "set_mode",
			Payload:     "full_access",
		},
		{
			ID:          "perm-readonly",
			Category:    CatMode,
			Title:       "Set Permission: Read Only",
			Description: "Strict safety gate denying all filesystem writes",
			ActionType:  "set_mode",
			Payload:     "readonly",
		},

		// Thinking & Reasoning
		{
			ID:          "thinking-high",
			Category:    CatModel,
			Title:       "Thinking Effort: High",
			Description: "Maximum reasoning budget for complex architectural tasks",
			ActionType:  "set_thinking",
			Payload:     "high",
		},
		{
			ID:          "thinking-medium",
			Category:    CatModel,
			Title:       "Thinking Effort: Medium",
			Description: "Balanced reasoning budget for typical coding tasks",
			ActionType:  "set_thinking",
			Payload:     "medium",
		},
		{
			ID:          "thinking-low",
			Category:    CatModel,
			Title:       "Thinking Effort: Low",
			Description: "Fast, minimal reasoning budget",
			ActionType:  "set_thinking",
			Payload:     "low",
		},
		{
			ID:          "thinking-off",
			Category:    CatModel,
			Title:       "Thinking Effort: Off",
			Description: "Standard inference without extended reasoning",
			ActionType:  "set_thinking",
			Payload:     "none",
		},

		// Visuals & Animations
		{
			ID:          "spinner-bloom",
			Category:    CatVisual,
			Title:       "Spinner: Bloom Flower (✻ ✼ ✽ ✾ ✿ ❀)",
			Description: "Claude-style organic petal sequence (default)",
			ActionType:  "set_spinner",
			Payload:     "bloom",
		},
		{
			ID:          "spinner-braille",
			Category:    CatVisual,
			Title:       "Spinner: Braille Dots (⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏)",
			Description: "OpenCode-style rotating 10-frame braille matrix",
			ActionType:  "set_spinner",
			Payload:     "braille",
		},
		{
			ID:          "spinner-sweep",
			Category:    CatVisual,
			Title:       "Spinner: Orb Sweep (◐ ◓ ◑ ◒)",
			Description: "Classic Niki quad-quadrant sweep",
			ActionType:  "set_spinner",
			Payload:     "sweep",
		},
		{
			ID:          "spinner-pulse",
			Category:    CatVisual,
			Title:       "Spinner: Audio Pulse (  ▃ ▄ ▅ ▆ ▇ █)",
			Description: "Sound wave amplitude pulse bars",
			ActionType:  "set_spinner",
			Payload:     "pulse",
		},
		{
			ID:          "theme-default",
			Category:    CatVisual,
			Title:       "Theme: Default Modern Dark",
			Description: "Balanced cyan and slate dark palette",
			ActionType:  "set_theme",
			Payload:     "default",
		},
		{
			ID:          "theme-monochrome",
			Category:    CatVisual,
			Title:       "Theme: Monochrome Minimal",
			Description: "Zero ANSI color accents, high-contrast text",
			ActionType:  "set_theme",
			Payload:     "monochrome",
		},

		// Actions
		{
			ID:          "act-compact",
			Category:    CatAction,
			Title:       "Compact Context (/compact)",
			Description: "Summarize previous turns and reclaim token budget",
			ActionType:  "slash",
			Payload:     "/compact",
		},
		{
			ID:          "act-cost",
			Category:    CatAction,
			Title:       "Session Cost & Tokens (/cost)",
			Description: "Inspect token breakdown and dollar expenditure",
			ActionType:  "slash",
			Payload:     "/cost",
		},
		{
			ID:          "act-doctor",
			Category:    CatAction,
			Title:       "Run Health Doctor (/doctor)",
			Description: "Verify bubblewrap sandbox and tool environment",
			ActionType:  "slash",
			Payload:     "/doctor",
		},
		{
			ID:          "act-clear",
			Category:    CatAction,
			Title:       "Clear History (/clear)",
			Description: "Clear visible conversation scrollback",
			ActionType:  "slash",
			Payload:     "/clear",
		},
		{
			ID:          "act-reload",
			Category:    CatAction,
			Title:       "Reload Config (/reload)",
			Description: "Hot-reload nikicode.toml from disk without restart",
			ActionType:  "slash",
			Payload:     "/reload",
		},
		{
			ID:          "act-quit",
			Category:    CatAction,
			Title:       "Exit NikiCode (/quit)",
			Description: "Restore terminal state and exit cleanly",
			ActionType:  "slash",
			Payload:     "/quit",
		},
	}
	if paths.Env("MOCK") != "" || paths.Env("DEMO_TOUR") != "" {
		items = append(items, PaletteItem{
			ID:          "model-mock",
			Category:    CatModel,
			Title:       "Switch to Mock Demo Provider",
			Description: "Zero-cost local tour without API key",
			ActionType:  "set_model",
			Payload:     "mock:gpt-4o-mini",
		})
	}
	return items
}

// FilterPalette returns items matching query in title, category, or description.
func FilterPalette(items []PaletteItem, query string) []PaletteItem {
	q := strings.TrimSpace(strings.ToLower(query))
	if q == "" {
		return items
	}
	var out []PaletteItem
	for _, item := range items {
		if strings.Contains(strings.ToLower(item.Title), q) ||
			strings.Contains(strings.ToLower(string(item.Category)), q) ||
			strings.Contains(strings.ToLower(item.Description), q) {
			out = append(out, item)
		}
	}
	return out
}

// RenderPaletteView renders the floating modal overlay for Ctrl+P.
func RenderPaletteView(state PaletteState, th Theme, termWidth int) string {
	width := termWidth - 8
	if width > 80 {
		width = 80
	}
	if width < 40 {
		width = 40
	}

	switch state.Mode {
	case "connect":
		return renderConnectModal(state, th, width)
	case "custom_model":
		return renderPromptInputModal("🤖 Set Custom Model Name", "Enter provider:model or model identifier (e.g. anthropic:claude-3-7-sonnet)", "e.g. openai:deepseek-r1", state.InputBuffer, th, width)
	case "custom_endpoint":
		return renderPromptInputModal("🌐 Set Custom Provider Base URL", "Enter endpoint base URL for local vLLM, LM Studio, Ollama, or OpenRouter", "e.g. http://localhost:11434/v1", state.InputBuffer, th, width)
	case "mcp_add":
		return renderMCPAddModal(state, th, width)
	}

	filtered := FilterPalette(DefaultPaletteCatalog(), state.Query)
	cursor := state.Selected
	if cursor >= len(filtered) {
		cursor = len(filtered) - 1
	}
	if cursor < 0 {
		cursor = 0
	}

	title := th.CardTitle.Render("⚡ Command Palette")
	hintClose := th.Muted.Render("(Ctrl+P / Esc to close)")
	headerLine := fmt.Sprintf("%s  %s", title, hintClose)

	queryDisplay := state.Query
	if queryDisplay == "" {
		queryDisplay = th.Placeholder.Render("Type to search settings, models, actions…")
	} else {
		queryDisplay = th.UserText.Render(state.Query)
	}
	searchBox := th.PaletteSearch.
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(width - 4).
		Render("❯ " + queryDisplay + "█")

	const maxRows = 6
	startIdx := 0
	if cursor >= maxRows {
		startIdx = cursor - maxRows + 1
	}
	endIdx := startIdx + maxRows
	if endIdx > len(filtered) {
		endIdx = len(filtered)
	}

	var rows []string
	if len(filtered) == 0 {
		rows = append(rows, th.Muted.Render("  No matching settings or commands."))
	} else {
		for i := startIdx; i < endIdx; i++ {
			item := filtered[i]
			isSel := (i == cursor)

			catTag := th.PaletteTag.
				Padding(0, 1).
				Render(string(item.Category))

			titleStyle := th.Assistant
			if isSel {
				titleStyle = th.PaletteSelected
			}

			line := ""
			if isSel {
				line = th.PaletteSelected.Render("▶ ") +
					titleStyle.Render(item.Title) + "  " + catTag + "\n    " +
					th.Muted.Render(item.Description)
			} else {
				line = "  " + titleStyle.Render(item.Title) + "  " + catTag + "\n    " +
					th.Muted.Render(item.Description)
			}
			rows = append(rows, line)
		}
	}

	footer := th.Muted.Render("↑/↓ navigate · enter select · esc close")
	content := lipgloss.JoinVertical(lipgloss.Left,
		headerLine,
		"",
		searchBox,
		"",
		strings.Join(rows, "\n\n"),
		"",
		footer,
	)

	return th.PaletteBorder.
		Border(lipgloss.DoubleBorder()).
		Padding(1, 2).
		Width(width).
		Render(content)
}

func renderConnectModal(state PaletteState, th Theme, width int) string {
	title := th.CardTitle.Render("🔑 Connect API Key: " + strings.ToUpper(state.ConnectProvider))
	help := th.Muted.Render("Key will be saved securely to ~/.config/nikicode/nikicode.toml")

	inputDisplay := state.ConnectKey
	if inputDisplay == "" {
		inputDisplay = th.Placeholder.Render("Paste your API key here (sk-...)")
	} else {
		// Mask the key partially for privacy
		if len(inputDisplay) > 8 {
			masked := inputDisplay[:4] + strings.Repeat("•", len(inputDisplay)-8) + inputDisplay[len(inputDisplay)-4:]
			inputDisplay = th.UserText.Render(masked)
		} else {
			inputDisplay = th.UserText.Render(strings.Repeat("•", len(inputDisplay)))
		}
	}

	keyBox := th.PaletteSuccess.
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(width - 4).
		Render("Key: " + inputDisplay + "█")

	footer := th.Muted.Render("enter save and reload · esc cancel")

	content := lipgloss.JoinVertical(lipgloss.Left,
		title,
		help,
		"",
		keyBox,
		"",
		footer,
	)

	return th.PaletteSuccess.
		Border(lipgloss.DoubleBorder()).
		Padding(1, 2).
		Width(width).
		Render(content)
}

func renderPromptInputModal(titleText, helpText, placeholder, val string, th Theme, width int) string {
	title := th.CardTitle.Render(titleText)
	help := th.Muted.Render(helpText)

	display := val
	if display == "" {
		display = th.Placeholder.Render(placeholder)
	} else {
		display = th.UserText.Render(display)
	}

	box := th.PaletteSuccess.
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(width - 4).
		Render("❯ " + display + "█")

	footer := th.Muted.Render("enter save · esc cancel")
	content := lipgloss.JoinVertical(lipgloss.Left,
		title,
		help,
		"",
		box,
		"",
		footer,
	)

	return th.PaletteSuccess.
		Border(lipgloss.DoubleBorder()).
		Padding(1, 2).
		Width(width).
		Render(content)
}

func renderMCPAddModal(state PaletteState, th Theme, width int) string {
	steps := []string{
		"Step 1/3: Enter MCP Server Name (e.g. filesystem, github, postgres):",
		"Step 2/3: Enter Executable / Command (e.g. npx, uvx, docker):",
		"Step 3/3: Enter Command Arguments (e.g. -y @modelcontextprotocol/server-filesystem /path):",
	}
	stepTitle := "📦 Add MCP Server"
	help := th.Muted.Render(steps[state.InputStep%3])

	display := state.InputBuffer
	if display == "" {
		display = th.Placeholder.Render("type value here…")
	} else {
		display = th.UserText.Render(display)
	}

	box := th.PaletteSuccess.
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(width - 4).
		Render("❯ " + display + "█")

	footer := th.Muted.Render("enter next · esc cancel")
	content := lipgloss.JoinVertical(lipgloss.Left,
		th.CardTitle.Render(stepTitle),
		help,
		"",
		box,
		"",
		footer,
	)

	return th.PaletteSuccess.
		Border(lipgloss.DoubleBorder()).
		Padding(1, 2).
		Width(width).
		Render(content)
}
