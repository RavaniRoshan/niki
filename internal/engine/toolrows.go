package engine

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Tool row data (C2): the classic layout renders one line per tool
// call — glyph, verb, target, right-aligned result — and never
// invents any of it. The engine extracts the target from the real
// tool arguments and the result from the real tool output, then
// hands them to the frontend as event fields.

// toolTarget extracts the most identifying argument from a tool
// call's JSON arguments: the file path, command, or pattern.
func toolTarget(tool, args string) string {
	var obj map[string]any
	if err := json.Unmarshal([]byte(args), &obj); err != nil {
		return ""
	}
	get := func(keys ...string) string {
		for _, k := range keys {
			if v, ok := obj[k].(string); ok && strings.TrimSpace(v) != "" {
				return strings.TrimSpace(v)
			}
		}
		return ""
	}
	switch tool {
	case "read_file", "write_file", "edit_file", "view_image":
		return get("path", "file_path")
	case "list_dir":
		return get("path", "dir")
	case "grep", "glob":
		return get("pattern", "query", "path")
	case "apply_patch":
		return get("patch", "input")
	case "shell", "exec_command", "bash":
		return get("command", "cmd", "script")
	case "web_fetch":
		return get("url")
	case "update_plan":
		return get("plan", "explanation")
	}
	// MCP and unknown tools: first string arg wins, its key shown
	// when it is not an obvious identifier.
	for _, k := range []string{"query", "input", "path", "command", "url", "text", "prompt"} {
		if v := get(k); v != "" {
			return v
		}
	}
	return ""
}

// toolRow is the classic tool-row payload for one tool event.
type toolRow struct {
	Target  string
	Result  string
	Excerpt string
}

// toolRowResult summarizes a tool outcome for the right-aligned
// result slot. Counts come from the real output; nothing is
// estimated.
func toolRowResult(tool, output string, isErr bool) toolRow {
	row := toolRow{}
	if isErr {
		row.Result = "failed"
		if exit := exitCode(output); exit != "" {
			row.Result = exit
		}
		if fails := failingCount(output); fails > 0 {
			row.Result += fmt.Sprintf(" %s %d failing", sep(), fails)
		}
		row.Excerpt = failureExcerpt(output)
		return row
	}
	switch tool {
	case "read_file", "view_image":
		row.Result = fmt.Sprintf("%d lines", lineCount(output))
	case "grep", "glob":
		if n := matchCount(output); n > 0 {
			row.Result = fmt.Sprintf("%d matches", n)
		} else if n := lineCount(output); n > 0 {
			row.Result = fmt.Sprintf("%d lines", n)
		} else {
			row.Result = "no matches"
		}
	case "list_dir":
		row.Result = fmt.Sprintf("%d entries", lineCount(output))
	case "edit_file", "write_file", "apply_patch":
		if add, del := diffStat(output); add > 0 || del > 0 {
			row.Result = fmt.Sprintf("+%d %s%d", add, sep(), del)
		}
	case "shell", "exec_command", "bash":
		row.Result = "exit 0"
	}
	return row
}

func sep() string { return "\u00b7" }

func lineCount(s string) int {
	s = strings.TrimRight(s, "\n")
	if s == "" {
		return 0
	}
	return strings.Count(s, "\n") + 1
}

// matchCount counts grep-style "file:line:" hits.
func matchCount(s string) int {
	n := 0
	for _, line := range strings.Split(s, "\n") {
		if strings.Count(line, ":") >= 2 && strings.TrimSpace(line) != "" {
			n++
		}
	}
	return n
}

// diffStat sums unified-diff plus/minus lines.
func diffStat(s string) (add, del int) {
	for _, line := range strings.Split(s, "\n") {
		switch {
		case strings.HasPrefix(line, "+++"), strings.HasPrefix(line, "---"):
		case strings.HasPrefix(line, "+"):
			add++
		case strings.HasPrefix(line, "-"):
			del++
		}
	}
	return add, del
}

// exitCode reads "exit N" or "exit status N" from shell output.
func exitCode(s string) string {
	for _, line := range strings.Split(s, "\n") {
		l := strings.ToLower(line)
		if i := strings.Index(l, "exit status "); i != -1 {
			return "exit " + strings.TrimSpace(l[i+len("exit status "):])
		}
		if i := strings.Index(l, "exit "); i != -1 && i+5 < len(l) {
			return strings.TrimSpace(line[i:])
		}
	}
	return ""
}

// failingCount counts "--- FAIL:" / "FAIL" test lines.
func failingCount(s string) int {
	n := 0
	for _, line := range strings.Split(s, "\n") {
		if strings.HasPrefix(strings.TrimSpace(line), "--- FAIL:") || strings.HasPrefix(strings.TrimSpace(line), "FAIL\t") {
			n++
		}
	}
	return n
}

// failureExcerpt keeps up to two non-empty lines of failing output,
// each capped, for the excerpt beneath a failed tool row.
func failureExcerpt(s string) string {
	var kept []string
	for _, line := range strings.Split(s, "\n") {
		t := strings.TrimRight(line, "\r")
		if strings.TrimSpace(t) == "" {
			continue
		}
		if len(t) > 100 {
			t = t[:100] + "..."
		}
		kept = append(kept, t)
		if len(kept) == 2 {
			break
		}
	}
	return strings.Join(kept, "\n")
}

// toolRowEventFields fills the classic tool-row fields of a tool
// event (C2). Called at the engine's tool emission sites; the
// frontend renders exactly what it is given.
func toolRowEventFields(ev *protocol.EngineEvent, tool, args, output string, isErr bool) {
	ev.Target = toolTarget(tool, args)
	row := toolRowResult(tool, output, isErr)
	ev.Result = row.Result
	ev.Excerpt = row.Excerpt
}
