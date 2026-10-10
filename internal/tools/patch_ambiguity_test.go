package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"
	"strings"
	"testing"
)

// buildShiftedHunk constructs a hunk whose stated position is stale
// (content was shifted down by `shift` lines), context = prefix.
func hunkAt(oldStart int, lines ...string) DiffHunk {
	return DiffHunk{
		OldStart:  oldStart,
		OldLength: len(lines),
		NewStart:  oldStart,
		NewLength: len(lines),
		Lines:     lines,
	}
}

// Two identical context blocks sit within the fuzzy window while the
// hunk's stated position matches neither: ambiguous — must be rejected.
func TestAmbiguousFuzzyContextRejected(t *testing.T) {
	block := []string{"    result := compute(x)", "    fmt.Println(result)", "    return result"}
	var sb strings.Builder
	sb.WriteString("package main\n")
	for i := 0; i < 5; i++ {
		fmt.Fprintf(&sb, "filler-%d\n", i)
	}
	// candidate A at line 7 (1-based), candidate B 8 lines later
	sb.WriteString(strings.Join(block, "\n") + "\n")
	for i := 0; i < 5; i++ {
		fmt.Fprintf(&sb, "mid-%d\n", i)
	}
	sb.WriteString(strings.Join(block, "\n") + "\n")
	for i := 0; i < 5; i++ {
		fmt.Fprintf(&sb, "tail-%d\n", i)
	}
	content := sb.String()

	// Stated position (line 12) matches neither candidate.
	hunk := hunkAt(12, " "+block[0], " "+block[1], " "+block[2])
	_, err := ApplyPatchWithOpts(content, []DiffHunk{hunk}, false, true)
	if err == nil {
		t.Fatalf("[E1] ambiguous fuzzy match was applied instead of rejected")
	}
	if !strings.Contains(err.Error(), "ambiguous") {
		t.Fatalf("[E1] want ambiguity error, got: %v", err)
	}
	t.Logf("[E1 ambiguous] rejected: %v", err)
}

// Exactly one shifted candidate: the patch applies and the applied
// report carries the offset + confidence for logging.
func TestUniqueFuzzyMatchAppliesWithConfidence(t *testing.T) {
	var sb strings.Builder
	sb.WriteString("package main\n")
	for i := 0; i < 10; i++ {
		fmt.Fprintf(&sb, "filler-%d\n", i)
	}
	sb.WriteString("    alpha := 1\n    beta := 2\n    gamma := 3\n")
	content := sb.String()

	// Stated position is 3 lines off (shifted); unique block exists.
	hunk := hunkAt(6, "    alpha := 1", "    beta := 2", "    gamma := 3")
	// The hunk rewrites beta's line (diff indicators: ' ', '-', '+').
	hunk.Lines = []string{" " + "    alpha := 1", "-    beta := 2", "+    beta := 22", " " + "    gamma := 3"}
	hunk.OldLength, hunk.NewLength = 4, 4

	out, report, err := ApplyPatchReport(content, []DiffHunk{hunk}, false, true)
	if err != nil {
		t.Fatalf("[E1] unique fuzzy apply failed: %v", err)
	}
	if !strings.Contains(out, "beta := 22") {
		t.Fatalf("[E1] edit not applied:\n%s", out)
	}
	if !report.Fuzzy || report.Offset == 0 {
		t.Fatalf("[E1] want fuzzy report with nonzero offset, got %+v", report)
	}
	if report.Confidence <= 0 || report.Confidence > 1 {
		t.Fatalf("[E1] confidence out of range: %+v", report)
	}
	t.Logf("[E1 unique-fuzzy] offset=%+d confidence=%.2f applied", report.Offset, report.Confidence)
}

// A perfect match at the stated position always wins — duplicates
// elsewhere in the window are irrelevant.
func TestExactPositionNotAmbiguous(t *testing.T) {
	block := []string{"    x := 1", "    y := 2", "    z := 3"}
	var sb strings.Builder
	sb.WriteString("package main\n")
	sb.WriteString(strings.Join(block, "\n") + "\n")
	sb.WriteString("sep\n")
	sb.WriteString(strings.Join(block, "\n") + "\n")

	hunk := hunkAt(3, "-    y := 2", "+    y := 22")
	hunk.OldLength, hunk.NewLength = 1, 1
	out, report, err := ApplyPatchReport(sb.String(), []DiffHunk{hunk}, false, true)
	if err != nil {
		t.Fatalf("[E1] exact-position apply failed: %v", err)
	}
	if report.Fuzzy {
		t.Fatalf("[E1] exact match misreported as fuzzy: %+v", report)
	}
	if strings.Count(out, "y := 22") != 1 || !strings.Contains(out, "    y := 2\n") {
		t.Fatalf("[E1] wrong line replaced (want exactly one y := 22 and the second y := 2 kept):\n%s", out)
	}
	t.Logf("[E1 exact] matched at stated position, confidence=1.00")
}

// Context that exists nowhere: reject, never guess at the stated offset.
func TestStaleContextRejectedNotGuessed(t *testing.T) {
	content := "package main\nalpha\nbeta\ngamma\n"
	hunk := hunkAt(2, "-    NOT_IN_FILE := 1", "+    replacement := 1")
	hunk.OldLength, hunk.NewLength = 1, 1
	out, err := ApplyPatchWithOpts(content, []DiffHunk{hunk}, false, true)
	if err == nil {
		t.Fatalf("[E1] stale context was applied silently (out=%q)", out)
	}
	if !strings.Contains(err.Error(), "not found") {
		t.Fatalf("[E1] want not-found error, got: %v", err)
	}
	t.Logf("[E1 stale] rejected: %v", err)
}

// Property: applying a generated hunk to its source content always
// yields exactly the constructed target content (unique contexts).
func TestPropertyGeneratedHunkRoundTrips(t *testing.T) {
	rng := rand.New(rand.NewSource(42))
	for i := 0; i < 300; i++ {
		n := 5 + rng.Intn(40)
		lines := make([]string, n)
		for j := range lines {
			lines[j] = fmt.Sprintf("line-%d-%d", i, j) // globally unique => no ambiguity
		}
		target := rng.Intn(n)
		oldLine := lines[target]
		newLine := fmt.Sprintf("line-%d-%d-mod", i, target)

		hunkLines := []string{}
		if target > 0 {
			hunkLines = append(hunkLines, " "+lines[target-1])
		}
		hunkLines = append(hunkLines, "-"+oldLine, "+"+newLine)
		if target+1 < n {
			hunkLines = append(hunkLines, " "+lines[target+1])
		}
		hunk := hunkAt(target+1, hunkLines...)

		got, _, err := ApplyPatchReport(strings.Join(lines, "\n")+"\n", []DiffHunk{hunk}, false, true)
		if err != nil {
			t.Fatalf("[E1 property] seed=%d apply error: %v", i, err)
		}
		want := append([]string{}, lines...)
		want[target] = newLine
		if got != strings.Join(want, "\n")+"\n" {
			t.Fatalf("[E1 property] seed=%d round-trip mismatch:\ngot:\n%s\nwant:\n%s", i, got, strings.Join(want, "\n")+"\n")
		}
	}
	t.Logf("[E1 property] 300 generated hunks round-tripped exactly")
}

// The apply_patch tool logs the fuzzy confidence when a fallback was used.
func TestApplyPatchToolLogsConfidence(t *testing.T) {
	var sb strings.Builder
	for i := 0; i < 8; i++ {
		fmt.Fprintf(&sb, "filler-%d\n", i)
	}
	sb.WriteString("    unique := true\n")
	path := t.TempDir() + "/conf.log"
	if err := os.WriteFile(path, []byte(sb.String()), 0o644); err != nil {
		t.Fatal(err)
	}
	patch := fmt.Sprintf("--- a/%s\n+++ b/%s\n@@ -3,1 +3,1 @@\n-    unique := true\n+    unique := false\n", path, path)
	tool := NewApplyPatchTool()
	res, err := tool.Run(context.Background(), mustJSON(t, map[string]any{"path": path, "patch": patch}))
	if err != nil {
		t.Fatal(err)
	}
	if res.IsError {
		t.Fatalf("[E1] tool failed: %s", res.Output)
	}
	if !strings.Contains(res.Output, "confidence=") || !strings.Contains(res.Output, "fuzzy") {
		t.Fatalf("[E1] confidence not logged: %s", res.Output)
	}
	t.Logf("[E1 tool] %s", strings.SplitN(res.Output, "\n", 2)[0])
}

func mustJSON(t *testing.T, v any) json.RawMessage {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	return b
}
