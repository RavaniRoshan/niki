package engine

import (
	"encoding/json"
	"fmt"
	"strings"
)

// Text-format tool calls (M2): small models sometimes emit a tool
// call as text — a fenced JSON block or a tagged block like
// <tool_name>{...}</tool_name> — instead of a structured tool-call
// delta. We parse such text ONLY when the tool name and its argument
// schema match exactly. A near-miss name (or args that fail schema
// validation) is NOT executed; the recovery is logged so the operator
// can see it happened.

// ParsedTextCall is a tool call recovered from text.
type ParsedTextCall struct {
	Tool  string
	Args  json.RawMessage
	Valid bool
}

// taggedToolRe matches <tool_name> ... </tool_name> where the body is
// JSON. The name must be a plain identifier (no spaces/slashes) so a
// stray HTML-ish tag is not mistaken for a tool call.
//
// (Declared as a compiled pattern in parseTaggedCall via a package var
// to avoid recompiling per call.)

// parseTaggedCall extracts a <name>{json}</name> block, if present.
func parseTaggedCall(text string) (name string, args json.RawMessage, ok bool) {
	// Find the first '<' that begins a plausible tag.
	for i := 0; i < len(text); i++ {
		if text[i] != '<' {
			continue
		}
		rest := text[i+1:]
		gt := strings.IndexByte(rest, '>')
		if gt < 0 {
			return "", nil, false
		}
		name = rest[:gt]
		if !isPlainIdentifier(name) {
			continue
		}
		body := rest[gt+1:]
		closeTag := "</" + name + ">"
		end := strings.Index(body, closeTag)
		if end < 0 {
			continue
		}
		body = strings.TrimSpace(body[:end])
		if json.Valid([]byte(body)) {
			return name, json.RawMessage(body), true
		}
	}
	return "", nil, false
}

// parseFencedJSON extracts the first ```json ... ``` (or bare ```)
// fenced block body, if it is valid JSON.
func parseFencedJSON(text string) (json.RawMessage, bool) {
	idx := strings.Index(text, "```")
	if idx < 0 {
		return nil, false
	}
	rest := text[idx+3:]
	// Skip an optional language tag on the fence line.
	if nl := strings.IndexByte(rest, '\n'); nl >= 0 {
		rest = rest[nl+1:]
	}
	end := strings.Index(rest, "```")
	if end < 0 {
		return nil, false
	}
	body := strings.TrimSpace(rest[:end])
	if json.Valid([]byte(body)) {
		return json.RawMessage(body), true
	}
	return nil, false
}

func isPlainIdentifier(s string) bool {
	if s == "" {
		return false
	}
	for _, r := range s {
		if !(r == '_' || (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')) {
			return false
		}
	}
	return true
}

// RecoverTextCall attempts to parse a tool call out of assistant text
// (M2). It returns the parsed call and a note describing what was
// recovered (for logging). The caller must still validate the tool
// name against the registry and the args against the schema before
// executing — RecoverTextCall only extracts a candidate.
//
// Two shapes are recognized:
//  1. <tool_name>{json}</tool_name>  — the name is explicit.
//  2. ```json {"tool": "...", "args": {...}} ``` — a fenced block
//     carrying an explicit "tool" field.
//
// A near-miss tool name is returned as-is (Valid=false is NOT set
// here; the registry lookup rejects it). Recovery never guesses a tool
// name that the text does not state.
func RecoverTextCall(text string) (ParsedTextCall, string) {
	text = strings.TrimSpace(text)
	if text == "" {
		return ParsedTextCall{}, ""
	}
	// Shape 1: tagged block with an explicit name.
	if name, args, ok := parseTaggedCall(text); ok {
		return ParsedTextCall{Tool: name, Args: args},
			fmt.Sprintf("recovered text tool call <%s> (tagged block)", name)
	}
	// Shape 2: fenced JSON with an explicit "tool" field.
	if body, ok := parseFencedJSON(text); ok {
		var obj struct {
			Tool string          `json:"tool"`
			Args json.RawMessage `json:"args"`
		}
		if json.Unmarshal(body, &obj) == nil && obj.Tool != "" {
			if len(obj.Args) == 0 {
				obj.Args = json.RawMessage(`{}`)
			}
			return ParsedTextCall{Tool: obj.Tool, Args: obj.Args},
				fmt.Sprintf("recovered text tool call %q (fenced json)", obj.Tool)
		}
	}
	return ParsedTextCall{}, ""
}
