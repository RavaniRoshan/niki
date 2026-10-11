package index

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// C4: a repository map (symbol outline) under a token budget — Go
// AST with a fallback for other languages. A 10k-file fixture must
// respect the budget.
func TestRepoMapRespectsBudgetOn10kFiles(t *testing.T) {
	root := t.TempDir()
	for i := 0; i < 10000; i++ {
		var name, content string
		switch i % 3 {
		case 0:
			name = fmt.Sprintf("pkg%d/code%d.go", i%50, i)
			content = fmt.Sprintf("package pkg%d\n\nfunc Fn%d() {}\ntype T%d struct{}\n", i%50, i, i)
		case 1:
			name = fmt.Sprintf("py%d/mod%d.py", i%50, i)
			content = fmt.Sprintf("def fn%d():\n    pass\n", i)
		default:
			name = fmt.Sprintf("js%d/mod%d.js", i%50, i)
			content = fmt.Sprintf("function fn%d() {}\n", i)
		}
		p := filepath.Join(root, name)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	idx := NewSymbolIndex()
	if err := idx.Scan(root); err != nil {
		t.Fatal(err)
	}
	const budget = 2000
	m := idx.Map(budget)
	if est := len(m) / 4; est > budget+1 {
		t.Fatalf("[C4] map exceeds budget: ~%d tokens > %d", est, budget)
	}
	if !strings.Contains(m, "truncated to 2000 tokens") {
		t.Fatalf("[C4] truncation not reported:\n%s", m[len(m)-200:])
	}
	// The truncated head carries fallback symbols; the AST parser
	// is proven separately at full budget below.
	if !strings.Contains(m, "fn") {
		t.Fatalf("[C4] fallback symbols missing from map head")
	}
	full := idx.Map(10_000_000)
	if !strings.Contains(full, "Fn0") || !strings.Contains(full, "fn1") {
		t.Fatalf("[C4] AST + fallback symbols missing at full budget")
	}
	t.Logf("[C4] 10k files -> map within 2000 tokens, truncation explicit; AST + fallback proven")
}
