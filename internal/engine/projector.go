package engine

import (
	"strings"

	"github.com/RavaniRoshan/niki/internal/provider"
)

// ProjectContext inspects, repairs, and enriches conversation messages
// before they are transmitted to any model provider.
//
// Conformance:
//   - Repairs broken/unclosed tool calls from interrupted turns (synthesizes
//     a "[tool execution interrupted]" placeholder to prevent 400 Bad Request
//     errors from Anthropic and OpenAI).
//   - Merges consecutive assistant messages so turn-alternation schemas pass.
//   - Removes empty/vacuous messages.
//   - Injects 3-point prompt cache hints (CacheControl="ephemeral") on the
//     system prompt, last tool message, and latest user prompt (OpenCode parity).
func ProjectContext(messages []provider.Message) []provider.Message {
	if len(messages) == 0 {
		return messages
	}

	// Step 1: Filter out empty/vacuous messages (tool results and assistant
	// tool-call messages may carry empty text but essential payload).
	var filtered []provider.Message
	for _, m := range messages {
		trimmed := strings.TrimSpace(m.Content)
		if m.Role != "tool" && trimmed == "" && len(m.ToolCalls) == 0 {
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
			collapsed[len(collapsed)-1].ToolCalls = append(collapsed[len(collapsed)-1].ToolCalls, curr.ToolCalls...)
			continue
		}
		collapsed = append(collapsed, curr)
	}

	// Step 3: Repair unclosed tool calls.
	// Every assistant tool call must be answered by a tool result before the
	// next non-tool message; unanswered calls get a synthesized interrupted
	// result so provider-native tool protocols never see a dangling call.
	var repaired []provider.Message
	for i := 0; i < len(collapsed); {
		curr := collapsed[i]
		repaired = append(repaired, curr)

		needsRepair := curr.Role == "assistant" &&
			(len(curr.ToolCalls) > 0 || strings.Contains(curr.Content, "[ToolCall:"))
		if !needsRepair {
			i++
			continue
		}

		// Absorb the contiguous run of tool results that answers this message.
		answered := map[string]bool{}
		legacyFree := 0
		j := i + 1
		for j < len(collapsed) && collapsed[j].Role == "tool" {
			repaired = append(repaired, collapsed[j])
			if collapsed[j].ToolCallID != "" {
				answered[collapsed[j].ToolCallID] = true
			} else {
				legacyFree++
			}
			j++
		}

		// Synthesize interrupted results for calls the run did not answer.
		synthesize := func(id, name string) {
			repaired = append(repaired, provider.Message{
				Role:       "tool",
				Content:    "[tool execution interrupted or cancelled by user]",
				ToolCallID: id,
				Name:       name,
			})
		}
		if len(curr.ToolCalls) == 0 {
			if j == i+1 {
				synthesize("", "")
			}
		} else {
			for _, tc := range curr.ToolCalls {
				if answered[tc.ID] {
					continue
				}
				if legacyFree > 0 {
					// A legacy result without ID answers this call.
					legacyFree--
					continue
				}
				synthesize(tc.ID, tc.Tool)
			}
		}
		i = j
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
