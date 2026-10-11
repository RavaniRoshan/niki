package tools

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// C3: agentic search works with no index dependency — grep, glob and
// read run on stdlib directory walks even with PATH stripped of every
// helper binary, and the optional symbol index degrades gracefully.
func TestAgenticSearchWithoutIndex(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "sub", "note.txt")
	if err := os.MkdirAll(filepath.Join(dir, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("the qqqx7beacon signal is here\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Strip PATH to an empty directory: anything shelling out to rg
	// or another indexer binary fails from here on.
	emptyBin := t.TempDir()
	t.Setenv("PATH", emptyBin)

	ctx := context.Background()
	grepArgs, _ := json.Marshal(map[string]any{"pattern": "qqqx7beacon", "root": dir})
	res, err := NewGrepTool().Run(ctx, grepArgs)
	if err != nil || res.IsError || !strings.Contains(res.Output, "qqqx7beacon") {
		t.Fatalf("grep without index: err=%v out=%q", err, res.Output)
	}
	globArgs, _ := json.Marshal(map[string]any{"pattern": "*.txt", "root": dir})
	res, err = NewGlobTool().Run(ctx, globArgs)
	if err != nil || res.IsError || !strings.Contains(res.Output, "note.txt") {
		t.Fatalf("glob without index: err=%v out=%q", err, res.Output)
	}
	readArgs, _ := json.Marshal(map[string]any{"path": target})
	res, err = NewReadFileTool().Run(ctx, readArgs)
	if err != nil || res.IsError || !strings.Contains(res.Output, "qqqx7beacon") {
		t.Fatalf("read without index: err=%v out=%q", err, res.Output)
	}
	t.Logf("[C3] grep+glob+read work with PATH stripped to an empty dir")

	// The default registry's symbol search has no index wired: it
	// must say so instead of failing.
	reg := DefaultRegistry()
	tool, ok := reg.Get("symbol_search")
	if !ok {
		t.Fatalf("symbol_search missing from default toolset")
	}
	symArgs, _ := json.Marshal(map[string]any{"query": "beacon"})
	sres, serr := tool.Run(ctx, symArgs)
	if serr != nil {
		t.Fatalf("symbol_search without index must not error: %v", serr)
	}
	if !strings.Contains(strings.ToLower(sres.Output), "not initialized") {
		t.Fatalf("symbol_search should report uninitialized index, got %q", sres.Output)
	}
	t.Logf("[C3] default toolset has no index dependency (symbol_search degrades: %q)", sres.Output)
}
