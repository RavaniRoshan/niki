package tools

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
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

func TestRegistry(t *testing.T) {
	r := DefaultRegistry()
	if len(r.List()) != 6 {
		t.Fatalf("expected 6 tools, got %d", len(r.List()))
	}
	if _, ok := r.Get("shell"); !ok {
		t.Fatal("shell missing")
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
