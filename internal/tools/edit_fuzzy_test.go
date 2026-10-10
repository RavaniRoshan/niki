package tools

import (
	"context"
	"strings"
	"testing"
)

func runEdit(t *testing.T, path string, old, new string, replaceAll bool) ToolResult {
	t.Helper()
	tool := NewEditFileTool()
	args, _ := jsonMarshal(map[string]any{"path": path, "old_string": old, "new_string": new, "replace_all": replaceAll})
	res, err := tool.Run(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	return res
}

// Exact ambiguity stays rejected (never guessed, no fuzzy escape).
func TestEditExactAmbiguousStillRejected(t *testing.T) {
	p := t.TempDir() + "/a.txt"
	write(t, p, "line one\nline two\nline one again\nline one\n")
	res := runEdit(t, p, "line one", "LINE ONE", false)
	if !res.IsError || !strings.Contains(res.Output, "found 3") {
		t.Fatalf("[E1] exact ambiguity not rejected: %+v", res)
	}
	t.Logf("[E1 exact-amb] %s", res.Output)
}

// Exact text absent; a unique whitespace-normalized match exists:
// apply it and log the confidence.
func TestEditFuzzyUniqueAppliesAndLogs(t *testing.T) {
	p := t.TempDir() + "/u.txt"
	write(t, p, "package main\n\nfunc main() {\n\treturn  1\n}\n")
	res := runEdit(t, p, "return 1", "return 2", false)
	if res.IsError {
		t.Fatalf("[E1] fuzzy unique edit failed: %s", res.Output)
	}
	if !strings.Contains(res.Output, "fuzzy") || !strings.Contains(res.Output, "confidence=") {
		t.Fatalf("[E1] confidence not logged: %s", res.Output)
	}
	// The replacement text is used verbatim, exactly as an exact match
	// would apply it; only the MATCHING is normalized.
	data := read(t, p)
	if !strings.Contains(data, "return 2") {
		t.Fatalf("[E1] fuzzy edit not applied: %q", data)
	}
	if !strings.Contains(data, "func main() {\nreturn 2\n}") {
		t.Fatalf("[E1] file structure mangled: %q", data)
	}
	t.Logf("[E1 fuzzy-unique] %s", strings.SplitN(res.Output, "\n", 2)[0])
}

// Two normalized matches with no exact match: ambiguous — rejected,
// file untouched.
func TestEditFuzzyAmbiguousRejected(t *testing.T) {
	p := t.TempDir() + "/amb.txt"
	write(t, p, "x  = 1\ncode\ny  = 2\n  x  = 1\n")
	before := read(t, p)
	res := runEdit(t, p, "x = 1", "x = 99", false)
	if !res.IsError || !strings.Contains(res.Output, "ambiguous") {
		t.Fatalf("[E1] fuzzy ambiguity not rejected: %+v", res)
	}
	if read(t, p) != before {
		t.Fatalf("[E1] file changed despite rejection")
	}
	t.Logf("[E1 fuzzy-amb] %s", res.Output)
}

// No exact and no normalized match: still the not-found error.
func TestEditFuzzyNoMatchStillNotFound(t *testing.T) {
	p := t.TempDir() + "/n.txt"
	write(t, p, "alpha\nbeta\n")
	res := runEdit(t, p, "gamma", "delta", false)
	if !res.IsError || !strings.Contains(res.Output, "not found") {
		t.Fatalf("[E1] want not-found, got: %+v", res)
	}
	t.Logf("[E1 fuzzy-none] %s", res.Output)
}
