package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/intent"
)

func TestParseBranchTarget(t *testing.T) {
	cases := []struct{ in, action, name string }{
		{"create a branch called feat/login", "create", "feat/login"},
		{"create branch hotfix", "create", "hotfix"},
		{"new branch experiment", "create", "experiment"},
		{"switch to main", "switch", "main"},
		{"checkout feature", "switch", "feature"},
	}
	for _, c := range cases {
		action, name, err := parseBranchTarget(c.in)
		if err != nil || action != c.action || name != c.name {
			t.Fatalf("%q -> %q %q %v, want %q %q", c.in, action, name, err, c.action, c.name)
		}
	}
	if _, _, err := parseBranchTarget("create a branch"); err == nil {
		t.Fatal("nameless branch should refuse")
	}
}

func TestParseRebaseOnto(t *testing.T) {
	onto, err := parseRebaseOnto("rebase my work onto main")
	if err != nil || onto != "main" {
		t.Fatalf("onto = %q %v", onto, err)
	}
	if _, err := parseRebaseOnto("rebase please"); err == nil {
		t.Fatal("missing onto should refuse")
	}
}

func TestParseBlameTarget(t *testing.T) {
	file, line, err := parseBlameTarget("blame main.go:42")
	if err != nil || file != "main.go" || line != 42 {
		t.Fatalf("got %q:%d %v", file, line, err)
	}
	file, line, err = parseBlameTarget("blame line 10 of worker.go")
	if err != nil || file != "worker.go" || line != 10 {
		t.Fatalf("got %q:%d %v", file, line, err)
	}
	if _, _, err := parseBlameTarget("blame everything"); err == nil {
		t.Fatal("vague blame should refuse")
	}
}

func TestParseCountAndBase(t *testing.T) {
	if n := parseCount("show the last 5 commits", 10); n != 5 {
		t.Fatalf("count = %d", n)
	}
	if n := parseCount("show history", 10); n != 10 {
		t.Fatalf("default count = %d", n)
	}
	if b := parsePRBase("draft a PR against develop"); b != "develop" {
		t.Fatalf("base = %q", b)
	}
	if b := parsePRBase("draft a PR"); b != "main" {
		t.Fatalf("default base = %q", b)
	}
}

// TestRunDoGitOp exercises the full dispatch (except commit/rebase,
// covered at package level) against a throwaway repo.
func TestRunDoGitOp(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("system git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("init", "-b", "main")
	run("config", "user.email", "t@example.com")
	run("config", "user.name", "T")
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", "a.txt")
	run("commit", "-m", "seed")

	out, err := runDoGitOp(dir, intent.Action{Op: intent.GitStatus, Text: "status"})
	if err != nil || out != "clean" {
		t.Fatalf("status = %q %v", out, err)
	}
	out, err = runDoGitOp(dir, intent.Action{Op: intent.GitLog, Text: "show history"})
	if err != nil || !strings.Contains(out, "seed") {
		t.Fatalf("log = %q %v", out, err)
	}
	out, err = runDoGitOp(dir, intent.Action{Op: intent.GitBlame, Text: "blame a.txt:1"})
	if err != nil || !strings.Contains(out, "a.txt:1") {
		t.Fatalf("blame = %q %v", out, err)
	}
	out, err = runDoGitOp(dir, intent.Action{Op: intent.GitChangelog, Text: "changelog"})
	if err != nil || !strings.Contains(out, "seed") {
		t.Fatalf("changelog = %q %v", out, err)
	}
}
