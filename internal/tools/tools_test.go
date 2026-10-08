package tools

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func TestWriteAndReadFile(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "a.txt")
	w := NewWriteFileTool()
	r, err := w.Run(context.Background(), json.RawMessage(`{"path":"`+p+`","content":"hello\nworld\n"}`))
	if err != nil || r.IsError {
		t.Fatalf("write: %v %v", err, r)
	}
	rd := NewReadFileTool()
	res, _ := rd.Run(context.Background(), json.RawMessage(`{"path":"`+p+`"}`))
	if !strings.Contains(res.Output, "hello") {
		t.Fatalf("read: %q", res.Output)
	}
	res, _ = rd.Run(context.Background(), json.RawMessage(`{"path":"`+p+`","offset":1,"limit":1}`))
	if res.Output != "world\n" {
		t.Fatalf("offset read: %q", res.Output)
	}
}

func TestEditFile(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "b.txt")
	os.WriteFile(p, []byte("foo bar baz"), 0o644)
	e := NewEditFileTool()
	res, _ := e.Run(context.Background(), json.RawMessage(`{"path":"`+p+`","old_string":"bar","new_string":"qux"}`))
	if res.IsError {
		t.Fatal(res.Output)
	}
	data, _ := os.ReadFile(p)
	if string(data) != "foo qux baz" {
		t.Fatalf("got %q", data)
	}
}

func TestGlobAndGrep(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(filepath.Join(dir, "x.go"), []byte("package main\nfunc main() {}\n"), 0o644)
	os.MkdirAll(filepath.Join(dir, "sub"), 0o755)
	os.WriteFile(filepath.Join(dir, "sub", "y.go"), []byte("package sub\n"), 0o644)
	g := NewGlobTool()
	res, _ := g.Run(context.Background(), json.RawMessage(`{"pattern":"*.go","root":"`+dir+`"}`))
	if !strings.Contains(res.Output, "x.go") || !strings.Contains(res.Output, "y.go") {
		t.Fatalf("glob: %q", res.Output)
	}
	gr := NewGrepTool()
	res, _ = gr.Run(context.Background(), json.RawMessage(`{"pattern":"func main","root":"`+dir+`","glob":"*.go"}`))
	if !strings.Contains(res.Output, "func main") {
		t.Fatalf("grep: %q", res.Output)
	}
}

func TestShell(t *testing.T) {
	s := NewShellTool()
	res, _ := s.Run(context.Background(), json.RawMessage(`{"command":"echo hi"}`))
	if res.IsError || !strings.Contains(res.Output, "hi") {
		t.Fatalf("shell: %v %q", res, res.Output)
	}
}

// recordingSandbox records the commands routed
// through it without executing anything.
type recordingSandbox struct {
	mu    sync.Mutex
	calls []string
}

func (r *recordingSandbox) Name() string { return "recording" }

func (r *recordingSandbox) Run(ctx context.Context, dir, name string, args ...string) (string, string, error) {
	r.mu.Lock()
	r.calls = append(r.calls, name+" "+strings.Join(args, " "))
	r.mu.Unlock()
	return "sandboxed", "", nil
}

func (r *recordingSandbox) recorded() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]string(nil), r.calls...)
}

// TestShellSandboxRouting (S1): with a sandbox
// configured, commands run through the backend.
func TestShellSandboxRouting(t *testing.T) {
	rec := &recordingSandbox{}
	s := NewShellTool()
	s.Sandbox = rec
	res, err := s.Run(context.Background(), json.RawMessage(`{"command":"echo hi"}`))
	if err != nil || res.IsError {
		t.Fatalf("run: %v %v", err, res)
	}
	if res.Output != "sandboxed" {
		t.Fatalf("output = %q, want sandboxed", res.Output)
	}
	calls := rec.recorded()
	if len(calls) != 1 || !strings.Contains(calls[0], "bash -c echo hi") {
		t.Fatalf("backend calls = %v", calls)
	}
}

// TestShellExcludedCommand: excluded commands
// bypass the sandbox (and still execute).
func TestShellExcludedCommand(t *testing.T) {
	rec := &recordingSandbox{}
	s := NewShellTool()
	s.Sandbox = rec
	s.ExcludedCommands = []string{"echo"}
	res, err := s.Run(context.Background(), json.RawMessage(`{"command":"echo excluded-run"}`))
	if err != nil || res.IsError {
		t.Fatalf("run: %v %v", err, res)
	}
	if !strings.Contains(res.Output, "excluded-run") {
		t.Fatalf("excluded command did not execute: %q", res.Output)
	}
	if len(rec.recorded()) != 0 {
		t.Fatalf("excluded command reached the backend: %v", rec.recorded())
	}
}

// TestShellDisableSandboxArg: the
// dangerously_disable_sandbox escape hatch is
// honored only when the config allows it.
func TestShellDisableSandboxArg(t *testing.T) {
	rec := &recordingSandbox{}
	allowed := NewShellTool()
	allowed.Sandbox = rec
	allowed.AllowUnsandboxed = true
	res, err := allowed.Run(context.Background(), json.RawMessage(`{"command":"echo hi","dangerously_disable_sandbox":true}`))
	if err != nil || res.IsError {
		t.Fatalf("run: %v %v", err, res)
	}
	if len(rec.recorded()) != 0 {
		t.Fatalf("escape hatch was sandboxed: %v", rec.recorded())
	}

	denied := NewShellTool()
	denied.Sandbox = rec
	denied.AllowUnsandboxed = false
	res, err = denied.Run(context.Background(), json.RawMessage(`{"command":"echo hi","dangerously_disable_sandbox":true}`))
	if err != nil {
		t.Fatal(err)
	}
	if !res.IsError || !strings.Contains(res.Output, "not permitted") {
		t.Fatalf("escape hatch should be refused: %v %q", res, res.Output)
	}
	if len(rec.recorded()) != 0 {
		t.Fatalf("refused command reached the backend: %v", rec.recorded())
	}
}

func TestRegistry(t *testing.T) {
	r := DefaultRegistry()
	if len(r.List()) != 32 {
		t.Fatalf("expected 32 tools, got %d", len(r.List()))
	}
	for _, expected := range []string{
		"shell", "apply_patch", "edit_file", "read_file", "write_file",
		"glob", "grep", "web_search", "web_fetch", "view_image",
		"notebook_edit", "update_plan", "todo_write", "exec_command",
		"write_stdin", "bash_output", "kill_shell", "ask_user_question",
		"tool_search", "spawn_agent", "send_input", "wait_agent",
		"close_agent", "resume_agent",
		"git_status", "git_commit", "git_branch", "git_rebase",
		"git_blame", "git_log", "git_review", "git_changelog",
	} {
		if _, ok := r.Get(expected); !ok {
			t.Fatalf("tool %q missing from registry", expected)
		}
	}
}

func TestBatchConcurrencyGate(t *testing.T) {
	r := DefaultRegistry()
	dir := t.TempDir()
	p := filepath.Join(dir, "a.txt")
	os.WriteFile(p, []byte("hi"), 0o644)
	// All read-only/safe tools -> concurrent batch OK.
	res := r.Batch(context.Background(), []Call{
		{ID: "1", Name: "read_file", Args: json.RawMessage(`{"path":"` + p + `"}`)},
		{ID: "2", Name: "glob", Args: json.RawMessage(`{"pattern":"*.txt","root":"` + dir + `"}`)},
	})
	if len(res) != 2 || res[0].IsError || res[1].IsError {
		t.Fatalf("res=%v", res)
	}
	// Mixed batch with unsafe tool -> sequential path, order preserved.
	res = r.Batch(context.Background(), []Call{
		{ID: "1", Name: "read_file", Args: json.RawMessage(`{"path":"` + p + `"}`)},
		{ID: "2", Name: "write_file", Args: json.RawMessage(`{"path":"` + filepath.Join(dir, "b.txt") + `","content":"x"}`)},
	})
	if len(res) != 2 || res[0].IsError || res[1].IsError {
		t.Fatalf("res=%v", res)
	}
}

func TestSchemaValidation(t *testing.T) {
	r := DefaultRegistry()
	_, err := r.Get("read_file")
	if !err {
		t.Fatal("expected tool")
	}
	_, runErr := r.Run(context.Background(), "read_file", json.RawMessage(`{}`))
	if runErr == nil {
		t.Fatal("expected validation error for missing path")
	}
	res, runErr := r.Run(context.Background(), "read_file", json.RawMessage(`{"path":123}`))
	if runErr == nil || res.IsError {
		t.Fatalf("expected validation error for wrong type: %v %v", err, res)
	}
}
