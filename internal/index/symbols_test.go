package index

import (
	"os"
	"path/filepath"
	"testing"
)

func TestSymbolIndexScanAndSearch(t *testing.T) {
	dir := t.TempDir()

	goCode := `package testpkg

type Greeter interface {
	Greet() string
}

type User struct {
	Name string
}

func NewUser(name string) *User {
	return &User{Name: name}
}
`
	if err := os.WriteFile(filepath.Join(dir, "user.go"), []byte(goCode), 0o644); err != nil {
		t.Fatalf("failed to write go test file: %v", err)
	}

	idx := NewSymbolIndex()
	if err := idx.Scan(dir); err != nil {
		t.Fatalf("Scan failed: %v", err)
	}

	syms := idx.Search("Greeter", 10)
	if len(syms) == 0 || syms[0].Name != "Greeter" || syms[0].Kind != KindInterface {
		t.Fatalf("expected Greeter interface, got: %v", syms)
	}

	userSyms := idx.Search("User", 10)
	if len(userSyms) == 0 {
		t.Fatal("expected User struct to match")
	}

	fnSyms := idx.Search("NewUser", 10)
	if len(fnSyms) == 0 || fnSyms[0].Kind != KindFunc {
		t.Fatalf("expected NewUser func, got: %v", fnSyms)
	}
}
