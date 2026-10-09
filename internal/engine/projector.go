package engine

import (
	"strings"

	"github.com/RavaniRoshan/niki/internal/provider"
)

// ProjectContext inspects, repairs, and enriches conversation messages
// before they are transmitted to any model provider.
//
// Conformance:
// - Repairs broken/unclosed tool calls from interrupted turns (synthesizes
//   a "[tool execution interrupted]" placeholder to prevent 400 Bad Request
//   errors from Anthropic and OpenAI).
// - Merges consecutive assistant messages so turn-alternation schemas pass.
// - Removes empty/vacuous messages.
// - Injects 3-point prompt cache hints (CacheControl="ephemeral") on the
//   system prompt, last tool message, and latest user prompt (OpenCode parity).
func ProjectContext(messages []provider.Message) []provider.Message {
	if len(messages) == 0 {
		return messages
	}

	// Step 1: Filter out empty/vacuous messages (except tool results which may have empty output)
	var filtered []provider.Message
	for _, m := range messages {
		trimmed := strings.TrimSpace(m.Content)
		if m.Role != "tool" && trimmed == "" {
			continue
		}
		filtered = append(filtered, m)
	}

	if len(filtered) == 0 {
		return filtered
	}

	// Step 2: Merge consecutive assistant messages
	var collapsed []provider.Message
	for _, curr := range filtered {
		if curr.Role == "assistant" && len(collapsed) > 0 && collapsed[len(collapsed)-1].Role == "assistant" {
			collapsed[len(collapsed)-1].Content += "\n\n" + curr.Content
			continue
		}
		collapsed = append(collapsed, curr)
	}

	// Step 3: Repair unclosed tool calls.
	// If an assistant message contains a tool call indication, but the next message
	// is a user message without an intervening tool message, synthesize a cancelled tool result.
	var repaired []provider.Message
	for i := 0; i < len(collapsed); i++ {
		curr := collapsed[i]
		repaired = append(repaired, curr)

		if curr.Role == "assistant" && strings.Contains(curr.Content, "[ToolCall:") {
			// Check if next message is tool result
			hasToolNext := (i+1 < len(collapsed) && collapsed[i+1].Role == "tool")
			if !hasToolNext {
				repaired = append(repaired, provider.Message{
					Role:    "tool",
					Content: "[tool execution interrupted or cancelled by user]",
				})
			}
		}
	}

	// Step 4: 3-point prompt caching breakpoints (applyCachePolicy)
	// Point 1: System prompt (first system message)
	// Point 2: Latest user message
	// Point 3: Last tool result (if any)
	lastUserIdx := -1
	lastToolIdx := -1
	for idx, m := range repaired {
		switch m.Role {
		case "user":
			lastUserIdx = idx
		case "tool":
			lastToolIdx = idx
		}
	}

	out := make([]provider.Message, len(repaired))
	for idx, m := range repaired {
		mCopy := m
		if (idx == 0 && m.Role == "system") || idx == lastUserIdx || (idx == lastToolIdx && lastToolIdx != -1) {
			mCopy.CacheControl = "ephemeral"
		}
		out[idx] = mCopy
	}

	return out
}
