package format

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFormatFileGo(t *testing.T) {
	tmpDir := t.TempDir()
	goFile := filepath.Join(tmpDir, "test.go")

	unformatted := "package main\n\nfunc main() {\nvar x = 1\nprintln(x)\n}\n"
	if err := os.WriteFile(goFile, []byte(unformatted), 0644); err != nil {
		t.Fatalf("failed to write file: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()

	res := FormatFile(ctx, goFile)
	if !res.Formatted && res.Error != "" {
		t.Fatalf("unexpected formatter error: %s", res.Error)
	}

	data, err := os.ReadFile(goFile)
	if err != nil {
		t.Fatalf("failed to read file: %v", err)
	}
	t.Logf("Formatted output:\n%s", string(data))
}

func TestFormatFileUnknownExtension(t *testing.T) {
	tmpDir := t.TempDir()
	txtFile := filepath.Join(tmpDir, "notes.xyz")
	if err := os.WriteFile(txtFile, []byte("hello world"), 0644); err != nil {
		t.Fatalf("failed to write file: %v", err)
	}

	res := FormatFile(context.Background(), txtFile)
	if res.Formatted {
		t.Fatalf("expected unformatted for unknown extension")
	}
}
