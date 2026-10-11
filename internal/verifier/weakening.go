package verifier

import (
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

// Test-weakening detection (V6): an agent that "makes the suite pass"
// by deleting assertions, commenting them out, or adding skips is
// caught the same way a file deletion is (V4). We compare the test
// file set before and after the run and flag three signals:
//   - removed assertions (an assert/require/expect/if-lines count drops)
//   - new skips (t.Skip / pytest.mark.skip / it.skip / describe.skip)
//   - commented-out assertions (an assertion moved onto a // or # line)
//
// Any signal is a weakening event; a weakening event blocks T2/T3 and
// forces the run below "verified" — a green suite over weakened tests
// is not evidence.

var (
	// assertion-ish lines: language-agnostic markers for a check.
	assertionRe = regexp.MustCompile(`(?i)\b(assert|require|expect|should|t\.Error|t\.Fatal|panic\(|assertEqual|assertTrue)\b`)
	// skip markers across common test frameworks.
	skipRe = regexp.MustCompile(`(?i)\b(t\.Skip\w*|\.skip\s*\(|@pytest\.mark\.skip|pytest\.mark\.skipif|skipIf|describe\.skip|it\.skip|xdescribe|xit|test\.skip)\b`)
	// a comment line that still contains an assertion (commented-out test).
	commentedAssertRe = regexp.MustCompile(`^\s*(//|#|\*|/\*)\s*.*\b(assert|require|expect|t\.Error|t\.Fatal)\b`)
)

// countAssertions and countSkips measure a test file's strength.
func countAssertions(content string) int {
	n := 0
	for _, ln := range strings.Split(content, "\n") {
		if assertionRe.MatchString(ln) && !commentedAssertRe.MatchString(ln) {
			n++
		}
	}
	return n
}

func countSkips(content string) int {
	n := 0
	for _, ln := range strings.Split(content, "\n") {
		if skipRe.MatchString(ln) && !commentedAssertRe.MatchString(ln) {
			n++
		}
	}
	return n
}

func countCommentedAssertions(content string) int {
	n := 0
	for _, ln := range strings.Split(content, "\n") {
		if commentedAssertRe.MatchString(ln) {
			n++
		}
	}
	return n
}

// isTestFile reports whether a path looks like a test source file
// whose weakening matters (V6). Reuses the integrity test-file set.
func isTestFile(rel string) bool {
	base := filepath.Base(rel)
	switch {
	case strings.HasSuffix(base, "_test.go"),
		strings.HasSuffix(base, "_test.py"),
		strings.HasPrefix(base, "test_"),
		strings.HasSuffix(base, ".test.js"), strings.HasSuffix(base, ".test.ts"),
		strings.HasSuffix(base, ".spec.js"), strings.HasSuffix(base, ".spec.ts"),
		strings.HasSuffix(base, "_test.rs"):
		return true
	}
	return false
}

// TestStrength is the measured strength of one test file.
type TestStrength struct {
	Assertions int
	Skips      int
	Commented  int
}

// MeasureTests hashes the strength of every test file under dir.
func MeasureTests(dir string) map[string]TestStrength {
	out := map[string]TestStrength{}
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			switch d.Name() {
			case ".git", "node_modules", "target", "__pycache__", ".venv", "dist", "build":
				return filepath.SkipDir
			}
			return nil
		}
		rel, _ := filepath.Rel(dir, path)
		if !isTestFile(rel) {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return nil
		}
		content := string(data)
		out[rel] = TestStrength{
			Assertions: countAssertions(content),
			Skips:      countSkips(content),
			Commented:  countCommentedAssertions(content),
		}
		return nil
	})
	return out
}

// WeakeningEvent is one detected weakening of the test suite (V6).
type WeakeningEvent struct {
	File   string
	Signal string
	Detail string
}

// DetectWeakening compares test strength before and after the run and
// reports every weakening signal. Empty means the suite was not
// weakened. Newly-added skip lines, dropped assertions, and
// commented-out assertions all count; an added assertion or a removed
// skip strengthens the suite and is never flagged.
func DetectWeakening(before, after map[string]TestStrength) []WeakeningEvent {
	var out []WeakeningEvent
	for f, b := range before {
		a, ok := after[f]
		if !ok {
			// A whole test file vanished: that is deletion, reported by
			// the integrity diff (V4); not re-reported here as weakening.
			continue
		}
		if a.Assertions < b.Assertions {
			out = append(out, WeakeningEvent{f, "removed-assertions",
				fmtIntDelta(b.Assertions, a.Assertions)})
		}
		if a.Skips > b.Skips {
			out = append(out, WeakeningEvent{f, "new-skips",
				fmtIntDelta(b.Skips, a.Skips)})
		}
		if a.Commented > b.Commented {
			out = append(out, WeakeningEvent{f, "commented-out-assertions",
				fmtIntDelta(b.Commented, a.Commented)})
		}
	}
	// A brand-new test file that only adds skips is also a weakening.
	for f, a := range after {
		if _, ok := before[f]; ok {
			continue
		}
		if a.Skips > 0 {
			out = append(out, WeakeningEvent{f, "new-skips",
				fmtIntDelta(0, a.Skips)})
		}
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].File == out[j].File {
			return out[i].Signal < out[j].Signal
		}
		return out[i].File < out[j].File
	})
	return out
}

func fmtIntDelta(before, after int) string {
	return strconv.Itoa(before) + " -> " + strconv.Itoa(after)
}

// String renders a weakening event for a verdict reason.
func (w WeakeningEvent) String() string {
	return w.Signal + " in " + w.File + " (" + w.Detail + ")"
}
