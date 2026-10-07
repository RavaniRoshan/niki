package mcp

import (
	"path/filepath"
	"testing"
)

func TestCatalogCacheRoundTrip(t *testing.T) {
	dir := t.TempDir()
	c := NewCatalogCache(filepath.Join(dir, "catalog.json"))
	if _, ok := c.Load(); ok {
		t.Fatal("expected empty cache")
	}
	if err := c.Save([]string{"read_file", "shell"}); err != nil {
		t.Fatal(err)
	}
	tools, ok := c.Load()
	if !ok || len(tools) != 2 || tools[0] != "read_file" {
		t.Fatalf("tools=%v ok=%v", tools, ok)
	}
}

func TestStateTransitions(t *testing.T) {
	c := NewClient("test", "false")
	if c.StateOf() != StateStarting {
		t.Fatalf("state=%s", c.StateOf())
	}
}
