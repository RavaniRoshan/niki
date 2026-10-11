package envctx

import (
	"os"
	"os/exec"
	"strings"
	"testing"
)

// C9: environment context (OS, shell, git state) is collected once
// and refreshed on change.
func TestCollectAndRefreshOnBranchChange(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("init", "-q")
	run("config", "user.email", "c9@test")
	run("config", "user.name", "c9")
	if err := os.WriteFile(dir+"/f.txt", []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-qm", "one")
	run("checkout", "-qb", "feature-c9")

	w := New()
	first, changed := w.Refresh(dir)
	if !changed {
		t.Fatalf("first refresh must report collection")
	}
	if first.OS == "" || first.Shell == "" || first.Cwd != dir {
		t.Fatalf("missing facts: %+v", first)
	}
	if first.Branch != "feature-c9" {
		t.Fatalf("branch = %q, want feature-c9", first.Branch)
	}
	if _, changed := w.Refresh(dir); changed {
		t.Fatalf("second refresh with no change must report unchanged")
	}

	// Change the branch: the watcher must refresh.
	run("checkout", "-qb", "other-c9")
	third, changed := w.Refresh(dir)
	if !changed || third.Branch != "other-c9" {
		t.Fatalf("branch change not picked up: changed=%v %+v", changed, third)
	}
	if !strings.Contains(third.Block(), "git_branch: other-c9") {
		t.Fatalf("block missing new branch:\n%s", third.Block())
	}

	// Dirty state shows up too.
	if err := os.WriteFile(dir+"/f.txt", []byte("two\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	fourth, changed := w.Refresh(dir)
	if !changed || !strings.Contains(fourth.Status, "f.txt") {
		t.Fatalf("dirty status not picked up: changed=%v %+v", changed, fourth)
	}
	t.Logf("[C9] collected once, refreshed on branch change (->other-c9) and dirty state")
}
