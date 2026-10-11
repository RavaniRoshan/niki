package tools

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// C2: tool output is truncated head-and-tail, the full output is
// stored on disk, and a reference is returned. A 10 MB output stays
// bounded while the whole thing stays retrievable.
func TestCapOutputHeadAndTail10MB(t *testing.T) {
	tmpDir := t.TempDir()
	head := "HEAD-MARKER line one\nHEAD-MARKER line two\n"
	tail := "TAIL-MARKER final line\nTAIL-MARKER exit code 0\n"
	body := strings.Repeat("0123456789abcdef filler line for bulk\n", 280000) // ~10.6 MB
	huge := head + body + tail
	if len(huge) < 10*1024*1024 {
		t.Fatalf("fixture too small: %d", len(huge))
	}

	preview, capped := CapOutput("shell", huge, tmpDir)
	if !capped {
		t.Fatalf("10 MB output must be capped")
	}
	if len(preview) >= MaxOutputChars {
		t.Fatalf("preview unbounded: %d", len(preview))
	}
	if !strings.Contains(preview, "HEAD-MARKER line one") {
		t.Fatalf("head lost in preview")
	}
	if !strings.Contains(preview, "TAIL-MARKER exit code 0") {
		t.Fatalf("tail lost in preview")
	}
	if !strings.Contains(preview, "output capped:") {
		t.Fatalf("capping indicator missing")
	}

	// The reference must resolve to the full bytes on disk.
	ref := ""
	for _, line := range strings.Split(preview, "\n") {
		if i := strings.Index(line, "Full output saved to: "); i >= 0 {
			ref = strings.TrimSpace(line[i+len("Full output saved to: "):])
			ref = strings.TrimSuffix(ref, "]")
		}
	}
	if ref == "" {
		t.Fatalf("no on-disk reference in preview")
	}
	data, err := os.ReadFile(filepath.Clean(ref))
	if err != nil {
		t.Fatalf("cannot retrieve full output: %v", err)
	}
	if string(data) != huge {
		t.Fatalf("retrieved %d bytes, want full %d", len(data), len(huge))
	}
	t.Logf("[C2] 10 MB output -> %d-byte head+tail preview, full %d bytes retrievable", len(preview), len(data))
}
