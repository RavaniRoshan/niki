package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// E5: diff review per hunk before commit — accept one hunk, reject
// another. The staged diff holds only the accepted hunk; the
// worktree keeps both.
func TestStageHunksAcceptOneRejectOne(t *testing.T) {
	if !Available() {
		t.Skip("git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) string {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	run("init", "-q", "-b", "main")
	run("config", "user.email", "e5@test")
	run("config", "user.name", "e5")
	f := filepath.Join(dir, "f.txt")
	if err := os.WriteFile(f, []byte("one\ntwo\nthree\nfour\nfive\nsix\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-qm", "base")

	// Two far-apart edits = two hunks at -U0.
	if err := os.WriteFile(f, []byte("ONE\ntwo\nthree\nfour\nfive\nSIX\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := StageHunks(dir, "f.txt", []bool{true, false}); err != nil {
		t.Fatal(err)
	}
	staged := run("diff", "--cached")
	if !strings.Contains(staged, "ONE") || strings.Contains(staged, "SIX") {
		t.Fatalf("[E5] staged diff wrong:\n%s", staged)
	}
	worktree := run("diff")
	if !strings.Contains(worktree, "SIX") {
		t.Fatalf("[E5] rejected hunk lost from worktree:\n%s", worktree)
	}
	if strings.Contains(worktree, "ONE") {
		t.Fatalf("[E5] accepted hunk still dirty in worktree:\n%s", worktree)
	}
	t.Logf("[E5] hunk 1 staged, hunk 2 worktree-only")

	// Reject-all stages nothing and fails nothing.
	run("checkout", "--", "f.txt")
	if err := os.WriteFile(f, []byte("ONE\ntwo\nthree\nfour\nfive\nSIX\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("reset", "-q")
	if err := StageHunks(dir, "f.txt", []bool{false, false}); err != nil {
		t.Fatal(err)
	}
	if got := run("diff", "--cached"); got != "" {
		t.Fatalf("[E5] reject-all staged something:\n%s", got)
	}
	t.Logf("[E5] reject-all stages nothing")
}
