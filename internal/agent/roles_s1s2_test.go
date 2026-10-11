package agent

import (
	"strings"
	"testing"
)

// S1: each role's tool set is tested — explorer and reviewer are
// read-only (no mutating tool is ever allowed); the implementer may
// write and gets its own worktree; no role may spawn.
func TestRoleScopedToolSets(t *testing.T) {
	explorer, _ := RoleFor("explorer")
	implementer, _ := RoleFor("implementer")
	reviewer, _ := RoleFor("reviewer")

	for _, tool := range []string{"read_file", "grep", "glob", "git_status"} {
		if !explorer.AllowsTool(tool) {
			t.Fatalf("[S1] explorer must allow read tool %q", tool)
		}
	}
	for _, tool := range []string{"write_file", "edit_file", "apply_patch", "shell", "spawn_agent", "git_commit"} {
		if explorer.AllowsTool(tool) {
			t.Fatalf("[S1] explorer must NOT allow %q", tool)
		}
	}
	if !explorer.ReadOnly || explorer.OwnWorktree {
		t.Fatalf("[S1] explorer flags wrong: %+v", explorer)
	}
	t.Logf("[S1] explorer: read-only, no worktree, no spawn (%d tools)", len(explorer.Tools))

	if !implementer.AllowsTool("write_file") || !implementer.AllowsTool("edit_file") {
		t.Fatalf("[S1] implementer must allow write tools")
	}
	if !implementer.OwnWorktree {
		t.Fatalf("[S1] implementer must own a worktree")
	}
	if implementer.AllowsTool("spawn_agent") || implementer.MaySpawn {
		t.Fatalf("[S1] implementer must not spawn")
	}
	t.Logf("[S1] implementer: writes + own worktree, no spawn (%d tools)", len(implementer.Tools))

	if !reviewer.AllowsTool("git_review") || !reviewer.AllowsTool("read_file") {
		t.Fatalf("[S1] reviewer must allow review + read")
	}
	for _, tool := range []string{"write_file", "edit_file", "shell", "git_commit"} {
		if reviewer.AllowsTool(tool) {
			t.Fatalf("[S1] reviewer must NOT allow %q", tool)
		}
	}
	t.Logf("[S1] reviewer: read-only + git review, no write (%d tools)", len(reviewer.Tools))

	if _, err := RoleFor("wizard"); err == nil {
		t.Fatalf("[S1] unknown role must be refused")
	}
	t.Logf("[S1] unknown role refused")
}

// S2: a subagent returns a bounded summary with evidence references,
// not its full transcript — the size stays under a fixed cap no matter
// how long the raw output is.
func TestSubagentReturnsBoundedSummary(t *testing.T) {
	huge := strings.Repeat("this is a very long line of agent reasoning that should never be returned in full. ", 2000)
	evidence := []string{}
	for i := 0; i < 50; i++ {
		evidence = append(evidence, "file.go:123")
	}
	s := Summarize("agent-1", RoleImplementer, huge, evidence)
	if len([]rune(s.Text)) > MaxSummaryRunes+1 { // +1 for the ellipsis
		t.Fatalf("[S2] summary text not bounded: %d runes", len([]rune(s.Text)))
	}
	if len(s.Evidence) > MaxEvidenceRefs {
		t.Fatalf("[S2] evidence not bounded: %d refs", len(s.Evidence))
	}
	if !s.Truncated {
		t.Fatalf("[S2] huge transcript should be marked truncated")
	}
	if s.Size() > 4096 {
		t.Fatalf("[S2] summary too large: %d bytes", s.Size())
	}
	t.Logf("[S2] 100k-rune transcript -> %d-byte summary, %d evidence refs, truncated=%v",
		s.Size(), len(s.Evidence), s.Truncated)

	small := Summarize("agent-2", RoleExplorer, "found the bug in auth.go", []string{"auth.go:42"})
	if small.Truncated || len(small.Evidence) != 1 {
		t.Fatalf("[S2] short summary mangled: %+v", small)
	}
	t.Logf("[S2] short output intact: %q", small.Text)
}
