package logx

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLogsStructuredLinesToFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sub", "nikicode.log")
	l, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	l.Info("session.start", map[string]string{"provider": "openai"})
	l.Error("engine.stopped", os.ErrClosed, map[string]string{"code": "1"})
	l.Close()

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) != 2 {
		t.Fatalf("expected 2 log lines, got %d: %q", len(lines), string(data))
	}
	var first Entry
	if err := json.Unmarshal([]byte(lines[0]), &first); err != nil {
		t.Fatalf("first line not JSON: %v", err)
	}
	if first.Level != "info" || first.Event != "session.start" {
		t.Errorf("first entry = %+v", first)
	}
	if first.Fields["provider"] != "openai" {
		t.Errorf("fields lost: %+v", first.Fields)
	}
	var second Entry
	if err := json.Unmarshal([]byte(lines[1]), &second); err != nil {
		t.Fatalf("second line not JSON: %v", err)
	}
	if second.Level != "error" || second.Error != os.ErrClosed.Error() {
		t.Errorf("second entry = %+v", second)
	}
}

// TestNilLoggerSafe asserts a nil logger is a no-op
// so callers never need a nil check (P5 fail-open
// for logging must never break the TUI).
func TestNilLoggerSafe(t *testing.T) {
	var l *Logger
	l.Info("event", nil)
	l.Error("event", os.ErrClosed, nil)
	l.Close()
}

// TestAppendAcrossReopens asserts the log grows
// across reopen cycles.
func TestAppendAcrossReopens(t *testing.T) {
	path := filepath.Join(t.TempDir(), "nikicode.log")
	l, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	l.Info("one", nil)
	l.Close()
	l2, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	l2.Info("two", nil)
	l2.Close()
	data, _ := os.ReadFile(path)
	if got := strings.Count(string(data), "\n"); got != 2 {
		t.Errorf("expected 2 appended lines, got %d: %q", got, string(data))
	}
}
