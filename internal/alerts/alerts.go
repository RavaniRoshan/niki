package alerts

import (
	"fmt"
	"strings"
)

// AlertType identifies the operational alert condition.
type AlertType int

const (
	AlertNoModel AlertType = iota
	AlertAuthFailure
	AlertRateLimit
	AlertNetworkError
	AlertContextBudget
	AlertUnsandboxedCommand
)

// Alert represents an actionable dynamic notification within the harness.
type Alert struct {
	Type          AlertType
	Title         string
	Description   string
	Guidance      []string
	ActionCommand string
	Percentage    int
}

// NewNoModelAlert constructs an actionable alert when no model provider is configured.
func NewNoModelAlert() Alert {
	return Alert{
		Type:        AlertNoModel,
		Title:       "▲ No AI Model Configured",
		Description: "Cannot execute model generation without a connected provider.",
		Guidance: []string{
			"Connect a provider: Type /connect or press Ctrl+P to add an API key (Anthropic, OpenAI, OpenRouter, DeepSeek, Ollama)",
			"Environment variable: Set ANTHROPIC_API_KEY, OPENAI_API_KEY, OPENROUTER_API_KEY, or DEEPSEEK_API_KEY in your shell",
			"Local shell mode: Prefix your command with '!' to execute shell commands locally without an AI model (e.g. ! git status)",
		},
		ActionCommand: "/connect",
	}
}

// NewAuthFailureAlert constructs an alert for 401 Unauthorized errors.
func NewAuthFailureAlert(provider string) Alert {
	return Alert{
		Type:        AlertAuthFailure,
		Title:       "▲ Authentication Failed (401)",
		Description: fmt.Sprintf("The API key for provider '%s' is missing, invalid, or expired.", provider),
		Guidance: []string{
			"Update API key: Run /connect or press Ctrl+P to enter a valid key",
			"Check credentials in ~/.config/nikicode/nikicode.toml",
			"Verify your account status, active quota, and credits on the provider portal",
		},
		ActionCommand: "/connect",
	}
}

// NewRateLimitAlert constructs an alert for 429 Too Many Requests errors.
func NewRateLimitAlert(provider, retryAfter string) Alert {
	desc := fmt.Sprintf("Provider '%s' returned 429 rate limit or quota exceeded.", provider)
	if retryAfter != "" {
		desc += fmt.Sprintf(" Retry after: %s.", retryAfter)
	}
	return Alert{
		Type:        AlertRateLimit,
		Title:       "▲ Rate Limit Exceeded (429)",
		Description: desc,
		Guidance: []string{
			"Wait briefly for the provider rate limit cooldown period to expire",
			"Switch to a secondary or fallback provider via Ctrl+P",
			"Check your organization usage tier on the provider portal",
		},
		ActionCommand: "/model",
	}
}

// NewNetworkErrorAlert constructs an alert for connection/DNS dropouts.
func NewNetworkErrorAlert(endpoint, detail string) Alert {
	return Alert{
		Type:        AlertNetworkError,
		Title:       "▲ Network Endpoint Unreachable",
		Description: fmt.Sprintf("Could not connect to endpoint '%s': %s", endpoint, detail),
		Guidance: []string{
			"Check your internet connectivity, VPN, or corporate proxy settings",
			"Verify the base_url in your provider settings or ~/.config/nikicode/nikicode.toml",
			"Run 'nikicode doctor' to verify system and network health",
		},
		ActionCommand: "/doctor",
	}
}

// NewContextBudgetAlert constructs an alert when session tokens near context window limits.
func NewContextBudgetAlert(used, max int) Alert {
	pct := 0
	if max > 0 {
		pct = int(float64(used) / float64(max) * 100.0)
	}
	return Alert{
		Type:        AlertContextBudget,
		Title:       "▲ Context Window Threshold Reached",
		Description: fmt.Sprintf("Current session has utilized %d%% of context window (%d / %d tokens).", pct, used, max),
		Guidance: []string{
			"Run /compact to compress conversation history and free up token space",
			"Run /new to start a clean session with the same workspace context",
		},
		ActionCommand: "/compact",
		Percentage:    pct,
	}
}

// FormatPlain produces a clean, readable text representation for terminal display.
func (a Alert) FormatPlain(width int) string {
	var sb strings.Builder
	sb.WriteString(a.Title + "\n")
	sb.WriteString("  " + a.Description + "\n\n")
	sb.WriteString("  What you can do:\n")
	for _, g := range a.Guidance {
		sb.WriteString("  • " + g + "\n")
	}
	return sb.String()
}
