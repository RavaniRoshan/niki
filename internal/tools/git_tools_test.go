package tools

import (
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func initGitRepo(t *testing.T) string {
	t.Helper()
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
	return dir
}

func runTool(t *testing.T, r *Registry, name, args string) ToolResult {
	t.Helper()
	res, err := r.Run(context.Background(), name, json.RawMessage(args))
	if err != nil {
		t.Fatalf("%s error: %v", name, err)
	}
	if res.IsError {
		t.Fatalf("%s refused: %s", name, res.Output)
	}
	return res
}

func TestGitToolsEndToEnd(t *testing.T) {
	dir := initGitRepo(t)
	r := DefaultRegistry()
	ctx := context.Background()

	res, err := r.Run(ctx, "git_status", json.RawMessage(`{"dir":"`+dir+`"}`))
	if err != nil || res.IsError || res.Output != "clean" {
		t.Fatalf("status = %v %v", res, err)
	}
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("one\ntwo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runTool(t, r, "git_commit", `{"dir":"`+dir+`","stage":"a.txt"}`)
	if res := runTool(t, r, "git_log", `{"dir":"`+dir+`","n":2}`); !strings.Contains(res.Output, "update 1 file") {
		t.Fatalf("log missing derived commit:\n%s", res.Output)
	}
	runTool(t, r, "git_branch", `{"dir":"`+dir+`","action":"create","name":"feat"}`)
	if res := runTool(t, r, "git_blame", `{"dir":"`+dir+`","file":"a.txt","line":1}`); !strings.Contains(res.Output, "a.txt:1") {
		t.Fatalf("blame wrong:\n%s", res.Output)
	}
	if res := runTool(t, r, "git_changelog", `{"dir":"`+dir+`","n":3}`); !strings.Contains(res.Output, "# Changelog") {
		t.Fatalf("changelog wrong:\n%s", res.Output)
	}

	// Review path: stage a change, review it.
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("one\ntwo\nthree\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("git", "add", "a.txt")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	if res := runTool(t, r, "git_review", `{"dir":"`+dir+`"}`); !strings.Contains(res.Output, "a.txt") {
		t.Fatalf("review wrong:\n%s", res.Output)
	}

	// Rebase the feature branch onto main (clean).
	runTool(t, r, "git_rebase", `{"dir":"`+dir+`","onto":"main"}`)

	// Outside a repo every git tool refuses with a hint.
	bare := t.TempDir()
	bad, err := r.Run(ctx, "git_status", json.RawMessage(`{"dir":"`+bare+`"}`))
	if err != nil || !bad.IsError {
		t.Fatalf("status outside repo should refuse: %v %v", bad, err)
	}
}

func TestGitToolsReadOnlyFlags(t *testing.T) {
	for _, name := range []string{"git_status", "git_blame", "git_log", "git_review", "git_changelog"} {
		tool, ok := DefaultRegistry().Get(name)
		if !ok {
			t.Fatalf("%s not registered", name)
		}
		if !tool.IsReadOnly() || !tool.IsConcurrencySafe() {
			t.Fatalf("%s should be read-only and concurrency-safe", name)
		}
	}
	for _, name := range []string{"git_commit", "git_branch", "git_rebase"} {
		tool, ok := DefaultRegistry().Get(name)
		if !ok {
			t.Fatalf("%s not registered", name)
		}
		if tool.IsReadOnly() {
			t.Fatalf("%s must NOT be read-only", name)
		}
	}
}
