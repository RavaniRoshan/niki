package logx

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// O5: logs are local, redacted, rotated, and bounded in size.
func TestLogRotationBoundsSize(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "nikicode.log")
	l, err := OpenRotated(path, 4096, 2)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 300; i++ {
		l.Info("event-"+strings.Repeat("x", 100), nil)
	}
	l.Close()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 3 {
		names := []string{}
		for _, e := range entries {
			names = append(names, e.Name())
		}
		t.Fatalf("[O5] files = %v, want live + 2 backups", names)
	}
	var total int64
	for _, e := range entries {
		fi, _ := e.Info()
		if fi.Size() > 4096 {
			t.Fatalf("[O5] %s unbounded: %d", e.Name(), fi.Size())
		}
		total += fi.Size()
	}
	if total > 3*4096 {
		t.Fatalf("[O5] total log size unbounded: %d", total)
	}
	// Every rotated line is still intact JSON (rotation never tears).
	data, _ := os.ReadFile(path + ".1")
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	for i, ln := range lines {
		if !strings.HasSuffix(ln, "}") {
			t.Fatalf("[O5] torn line %d in rotated file", i)
		}
	}
	t.Logf("[O5] 300 entries -> 3 bounded files (live + 2 backups), no torn lines")
}
