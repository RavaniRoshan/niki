package editor

import (
	"strings"
	"testing"
)

func TestEditorPrepareAndReadDraft(t *testing.T) {
	initial := "Hello from NikiCode external editor test"
	cmd, path, err := PrepareEditorDraft(initial)
	if err != nil {
		t.Fatalf("PrepareEditorDraft failed: %v", err)
	}
	if cmd == nil || path == "" {
		t.Fatalf("expected non-nil cmd and path, got cmd=%v path=%s", cmd, path)
	}

	content, err := ReadAndCleanupDraft(path)
	if err != nil {
		t.Fatalf("ReadAndCleanupDraft failed: %v", err)
	}
	if strings.TrimSpace(content) != strings.TrimSpace(initial) {
		t.Fatalf("expected content %q, got %q", initial, content)
	}
}
