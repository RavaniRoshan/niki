package mention

import (
	"os"
	"path/filepath"
	"testing"
)

func seedTree(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	for _, rel := range []string{
		"cmd/nikicode/main.go",
		"internal/engine/engine.go",
		"internal/engine/turn.go",
		"README.md",
	} {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return dir
}

func TestExtract(t *testing.T) {
	got := Extract("review @main.go and @engine/turn.go please @main.go")
	if len(got) != 2 || got[0] != "main.go" || got[1] != "engine/turn.go" {
		t.Fatalf("mentions = %v", got)
	}
	if len(Extract("no mentions here")) != 0 {
		t.Fatal("false mentions")
	}
}

func TestResolveExactAndFuzzy(t *testing.T) {
	dir := seedTree(t)
	c, err := Resolve(dir, "engine/turn.go")
	if err != nil || c.Path != "internal/engine/turn.go" {
		t.Fatalf("exact = %+v %v", c, err)
	}
	// Basename without extension still finds the file.
	c, err = Resolve(dir, "turn")
	if err != nil || c.Path != "internal/engine/turn.go" {
		t.Fatalf("fuzzy = %+v %v", c, err)
	}
	// Ambiguous basename ranks deterministically (shortest path first on tie).
	cands, err := Picker(dir, "engine.go")
	if err != nil || len(cands) == 0 {
		t.Fatalf("picker = %v %v", cands, err)
	}
	if cands[0].Path != "internal/engine/engine.go" {
		t.Fatalf("top pick = %v", cands)
	}
}

func TestResolveRefuses(t *testing.T) {
	dir := seedTree(t)
	if _, err := Resolve(dir, "no-such-file-xyz"); err == nil {
		t.Fatal("missing mention should refuse")
	} else if _, ok := err.(*NoMatchError); !ok {
		t.Fatalf("wrong error type: %T", err)
	}
}
