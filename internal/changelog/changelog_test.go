package changelog

import (
	"bytes"
	"strings"
	"testing"
)

// U3: release notes rendered from conventional commits, golden output pinned.
func TestRenderMarkdownGolden(t *testing.T) {
	cl := FromCommits([]string{
		"feat(git): register submodules",
		"feat(last): plain-language run summary",
		"refactor(engine): boot phase ordering",
		"refactor(checkpoint): fork boundary fix",
		"docs!: deps: bubbletea v2",
		"fix(trace): boot-mark off-by-one",
		"fix(session): export line count",
		"docs: dogfood log header",
		"not a conventional line at all",
	})
	cl.Version = "v0.12.0"
	cl.Date = "2026-10-11"

	var buf bytes.Buffer
	cl.RenderMarkdown(&buf)
	got := strings.TrimSpace(buf.String())
	want := strings.TrimSpace(readGolden(t))
	if got != want {
		t.Fatalf("[U3] markdown golden mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
	t.Logf("[U3] release notes golden exact")
}

// Breaking (`!`) types land in Changed when they are not feat/fix.
func TestBreakingRefactorGroupsChanged(t *testing.T) {
	cl := FromCommits([]string{"refactor!: rewrite the planner"})
	cl.Version = "v1.0.0"
	cl.Date = "2026-10-11"
	var buf bytes.Buffer
	cl.RenderMarkdown(&buf)
	if !strings.Contains(buf.String(), "### Changed") {
		t.Fatalf("[U3] breaking refactor should group Changed:\n%s", buf.String())
	}
}

// Non-conventional lines keep their text verbatim under Other.
func TestNonConventionalKeptUnderOther(t *testing.T) {
	cl := FromCommits([]string{"random prose without a colon"})
	if len(cl.Entries) != 1 || cl.Entries[0].Type != "other" {
		t.Fatalf("[U3] non-conventional line should be preserved as other, got %+v", cl.Entries)
	}
	if !strings.Contains(cl.Entries[0].Subject, "random prose") {
		t.Fatalf("[U3] subject text lost: %q", cl.Entries[0].Subject)
	}
}

// ScanLog strips `git log --oneline` hash prefixes and parses conventional subjects.
func TestScanLogStripsHashPrefix(t *testing.T) {
	input := "a1b2c3d feat(git): register submodules\n" +
		"e4f5a6b fix(session): export line count\n"
	entries := ScanLog(strings.NewReader(input))
	if len(entries) != 2 || entries[0].Type != "feat" || entries[1].Type != "fix" {
		t.Fatalf("[U3] ScanLog misparsed: %+v", entries)
	}
	if entries[0].Scope != "git" || entries[1].Scope != "session" {
		t.Fatalf("[U3] scopes lost: %+v", entries)
	}
	t.Logf("[U3] ScanLog stripped hash prefix, parsed scopes")
}

func readGolden(t *testing.T) string {
	t.Helper()
	b, err := readFileBytes("testdata/changelog.golden")
	if err != nil {
		t.Fatalf("[U3] cannot read golden: %v", err)
	}
	return string(b)
}
