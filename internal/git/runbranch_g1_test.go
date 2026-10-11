package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// G1: each run works on its own branch; a dirty user checkout is
// preserved and the user's branch gains no commits.
func TestRunBranchKeepsUserBranchClean(t *testing.T) {
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
	run("config", "user.email", "g1@test")
	run("config", "user.name", "g1")
	if err := os.WriteFile(filepath.Join(dir, "f.txt"), []byte("clean\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-qm", "base")
	commitsBefore := run("rev-list", "--count", "main")

	// Dirty the checkout: modified tracked file + untracked file.
	if err := os.WriteFile(filepath.Join(dir, "f.txt"), []byte("dirty-edits\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "scratch.txt"), []byte("wip\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	prior, err := EnsureRunBranch(dir, "abc123")
	if err != nil {
		t.Fatal(err)
	}
	if prior != "main" {
		t.Fatalf("[G1] prior branch = %q, want main", prior)
	}
	if cur, _ := CurrentBranch(dir); cur != "niki/run-abc123" {
		t.Fatalf("[G1] now on %q, want the run branch", cur)
	}
	// Dirty state intact.
	if data, _ := os.ReadFile(filepath.Join(dir, "f.txt")); string(data) != "dirty-edits\n" {
		t.Fatalf("[G1] dirty edits lost: %q", data)
	}
	if _, err := os.Stat(filepath.Join(dir, "scratch.txt")); err != nil {
		t.Fatalf("[G1] untracked file lost: %v", err)
	}
	// The user's branch gained nothing.
	if got := run("rev-list", "--count", "main"); got != commitsBefore {
		t.Fatalf("[G1] user branch moved: %s -> %s", commitsBefore, got)
	}
	// Retry reuses the branch instead of failing.
	if _, err := EnsureRunBranch(dir, "abc123"); err != nil {
		t.Fatalf("[G1] run-branch retry failed: %v", err)
	}
	t.Logf("[G1] run on niki/run-abc123; dirty checkout intact; main still at %s commit(s)", commitsBefore)
}
