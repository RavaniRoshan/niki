package tools

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/index"
)

func TestSymbolSearchTool(t *testing.T) {
	dir := t.TempDir()
	code := `package sample
type SampleWorker struct{}
func (s *SampleWorker) Work() {}
`
	if err := os.WriteFile(filepath.Join(dir, "worker.go"), []byte(code), 0o644); err != nil {
		t.Fatalf("failed to write test code: %v", err)
	}

	idx := index.NewSymbolIndex()
	if err := idx.Scan(dir); err != nil {
		t.Fatalf("failed to scan symbols: %v", err)
	}

	tool := NewSymbolSearchTool(idx)
	args, _ := json.Marshal(map[string]string{"query": "SampleWorker"})
	res, err := tool.Run(context.Background(), args)
	if err != nil {
		t.Fatalf("tool run failed: %v", err)
	}
	if !strings.Contains(res.Output, "SampleWorker") {
		t.Fatalf("expected result to contain SampleWorker, got: %s", res.Output)
	}
	if !tool.IsReadOnly() || !tool.IsConcurrencySafe() {
		t.Fatal("expected tool to be read-only and concurrency safe")
	}
}
