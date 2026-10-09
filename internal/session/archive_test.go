package session

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestArchiveColdSessions(t *testing.T) {
	tmpDir := t.TempDir()

	// Create fresh session file
	fresh := filepath.Join(tmpDir, "fresh.jsonl")
	_ = os.WriteFile(fresh, []byte("fresh session content\n"), 0644)

	// Create old session file
	old := filepath.Join(tmpDir, "old.jsonl")
	_ = os.WriteFile(old, []byte("old session content to be compressed\n"), 0644)
	oldTime := time.Now().Add(-40 * 24 * time.Hour)
	_ = os.Chtimes(old, oldTime, oldTime)

	summary, err := ArchiveColdSessions(tmpDir, 30*24*time.Hour)
	if err != nil {
		t.Fatalf("unexpected error archiving sessions: %v", err)
	}

	if summary.ArchivedCount != 1 {
		t.Fatalf("expected 1 archived session, got %d", summary.ArchivedCount)
	}

	// Verify old.jsonl replaced by old.jsonl.gz
	if _, err := os.Stat(old); !os.IsNotExist(err) {
		t.Fatalf("expected old uncompressed file to be removed")
	}
	if _, err := os.Stat(old + ".gz"); err != nil {
		t.Fatalf("expected old.jsonl.gz to exist: %v", err)
	}

	// Verify fresh remained uncompressed
	if _, err := os.Stat(fresh); err != nil {
		t.Fatalf("expected fresh file to remain uncompressed")
	}
}
