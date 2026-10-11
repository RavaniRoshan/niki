package tools

import (
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Q6 (path guard half): the fundamental safety property of
// ResolveSafe is "success implies inside": whenever it returns a path
// without error, that path — after resolving every symlink — is within
// the real workspace root. The property is checked over randomly
// generated adversarial paths (traversal segments, absolute outside
// paths, symlink components that point out of the workspace).
func TestPropertyPathGuardSuccessImpliesInside(t *testing.T) {
	rnd := rand.New(rand.NewSource(606)) // deterministic seed

	// Fixture: workspace with real dirs, an outward symlink, and an
	// inward symlink; an outside dir holding a secret.
	ws := t.TempDir()
	outside := t.TempDir()
	if err := os.MkdirAll(filepath.Join(ws, "a"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(ws, "b"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(outside, "secret.txt"), []byte("s"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(ws, "a", "ok.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(ws, "outlink")); err != nil {
		t.Skipf("symlinks unsupported: %v", err)
	}
	if err := os.Symlink(filepath.Join(ws, "a"), filepath.Join(ws, "inlink")); err != nil {
		t.Fatal(err)
	}
	realRoot, err := filepath.EvalSymlinks(ws)
	if err != nil {
		t.Fatal(err)
	}

	segments := []string{"a", "b", "..", ".", "outlink", "inlink", "ok.txt", "missing.txt", "secret.txt", ".."}
	inside, refused := 0, 0
	for i := 0; i < 300; i++ {
		// Build a random relative path from 1..6 segments.
		n := 1 + rnd.Intn(6)
		parts := make([]string, n)
		for j := range parts {
			parts[j] = segments[rnd.Intn(len(segments))]
		}
		cand := strings.Join(parts, "/")
		if i%17 == 0 {
			// Sprinkle absolute-outside candidates too.
			cand = "/etc/passwd"
		}
		resolved, err := ResolveSafe(ws, cand)
		if err != nil {
			refused++
			continue
		}
		// PROPERTY: success implies the resolved path is inside the
		// real root once every symlink on it is expanded.
		real, rerr := filepath.EvalSymlinks(resolved)
		if rerr != nil {
			real = resolved // target may not exist; use cleaned path
		}
		real = filepath.Clean(real)
		if real != realRoot && !strings.HasPrefix(real, realRoot+"/") {
			t.Fatalf("[Q6] safety violation: ResolveSafe(%q) = %q (real %q) outside root %q", cand, resolved, real, realRoot)
		}
		inside++
	}
	if refused == 0 || inside == 0 {
		t.Fatalf("[Q6] property must exercise both outcomes: inside=%d refused=%d", inside, refused)
	}
	t.Logf("[Q6 path] 300 adversarial paths: %d resolved inside (property held), %d refused", inside, refused)
}

// Q6 (patch applier half): round-trip property — applying a generated
// hunk and then reversing it restores the original bytes exactly; a
// corrupted hunk never emits partial output (error ⇒ empty result).
func TestPropertyPatchApplierRoundTrip(t *testing.T) {
	rnd := rand.New(rand.NewSource(616))
	for iter := 0; iter < 200; iter++ {
		n := 3 + rnd.Intn(20)
		lines := make([]string, n)
		for i := range lines {
			lines[i] = fmt.Sprintf("line-%d-%d", iter, i)
		}
		content := strings.Join(lines, "\n") + "\n"
		i := rnd.Intn(n)
		edited := make([]string, n)
		copy(edited, lines)
		edited[i] = fmt.Sprintf("EDITED-%d-%d", iter, i)
		mutated := strings.Join(edited, "\n") + "\n"

		// Build a unified hunk replacing lines[i], one line of context.
		var hunkLines []string
		start := i
		if i > 0 {
			hunkLines = append(hunkLines, " "+lines[i-1])
			start = i - 1
		}
		hunkLines = append(hunkLines, "-"+lines[i], "+"+edited[i])
		if i+1 < n {
			hunkLines = append(hunkLines, " "+lines[i+1])
		}
		oldLen, newLen := 0, 0
		for _, l := range hunkLines {
			if l[0] != '+' {
				oldLen++
			}
			if l[0] != '-' {
				newLen++
			}
		}
		h := DiffHunk{OldStart: start + 1, OldLength: oldLen, NewStart: start + 1, NewLength: newLen, Lines: hunkLines}

		got, err := ApplyPatch(content, []DiffHunk{h})
		if err != nil {
			t.Fatalf("[Q6] iter %d: forward apply failed: %v", iter, err)
		}
		if got != mutated {
			t.Fatalf("[Q6] iter %d: forward apply wrong:\n got %q\nwant %q", iter, got, mutated)
		}
		// Reverse round-trip restores the original exactly.
		back, err := ApplyPatchWithOpts(got, []DiffHunk{h}, true, false)
		if err != nil {
			t.Fatalf("[Q6] iter %d: reverse apply failed: %v", iter, err)
		}
		if back != content {
			t.Fatalf("[Q6] iter %d: reverse round-trip wrong:\n got %q\nwant %q", iter, back, content)
		}
	}
	t.Logf("[Q6 patch] 200 generated hunks: forward + reverse round-trip exact")

	// Corrupted-hunk property: on context mismatch the applier errors
	// and returns an EMPTY string — never partial/corrupted output.
	for iter := 0; iter < 50; iter++ {
		n := 4 + rnd.Intn(10)
		lines := make([]string, n)
		for i := range lines {
			lines[i] = fmt.Sprintf("orig-%d", i)
		}
		content := strings.Join(lines, "\n") + "\n"
		// Hunk whose removed-line content does not exist in the file.
		bogus := DiffHunk{OldStart: 1, OldLength: 1, NewStart: 1, NewLength: 1,
			Lines: []string{"-this-line-does-not-exist"}}
		out, err := ApplyPatch(content, []DiffHunk{bogus})
		if err == nil {
			t.Fatalf("[Q6] iter %d: corrupted hunk applied without error", iter)
		}
		if out != "" {
			t.Fatalf("[Q6] iter %d: error must yield empty output, got %q", iter, out)
		}
	}
	t.Logf("[Q6 patch] 50 corrupted hunks: refused with empty output (never partial)")
}
