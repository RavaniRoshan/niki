package checkpoint

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestCheckpointAndRewind(t *testing.T) {
	tmpDir := t.TempDir()
	filePath := filepath.Join(tmpDir, "app.go")
	initialContent := "package main\n\nfunc Run() int { return 1 }\n"
	if err := os.WriteFile(filePath, []byte(initialContent), 0o644); err != nil {
		t.Fatal(err)
	}

	mgr := NewManager(filepath.Join(tmpDir, "checkpoints"))

	events := []protocol.EngineEvent{
		{Type: protocol.EventTurnStarted, Text: "Turn 1 prompt", Timestamp: time.Now()},
		{Type: protocol.EventAssistantTextDelta, Text: "Turn 1 response", Timestamp: time.Now()},
	}

	// 1. Create checkpoint for turn 1
	cp, err := mgr.CreateCheckpoint("turn-1", []string{filePath}, events)
	if err != nil {
		t.Fatalf("create checkpoint error: %v", err)
	}
	if len(cp.Files) != 1 {
		t.Fatalf("expected 1 file snapshot, got %d", len(cp.Files))
	}
	snap, ok := cp.Files[filePath]
	if !ok || snap.SHA256 == "" {
		t.Fatalf("missing file snapshot or SHA256")
	}

	// 2. Modify file in turn 2
	modifiedContent := "package main\n\nfunc Run() int { return 42 }\n"
	if err := os.WriteFile(filePath, []byte(modifiedContent), 0o644); err != nil {
		t.Fatal(err)
	}

	// 3. Rewind to turn 1 (safe restore when hash matches snapshot)
	// First test clobber prevention: file is modified, so without force, it detects mismatch and skips
	res, err := mgr.RewindCode("turn-1", false)
	if err != nil {
		t.Fatalf("rewind code error: %v", err)
	}
	if len(res.Skipped) != 1 {
		t.Fatalf("expected file to be skipped due to collision/clobber protection, got %v", res.Skipped)
	}

	// 4. Force rewind restores file to original
	res, err = mgr.RewindCode("turn-1", true)
	if err != nil {
		t.Fatalf("force rewind error: %v", err)
	}
	if len(res.Restored) != 1 {
		t.Fatalf("expected 1 restored file, got %d", len(res.Restored))
	}

	restoredData, _ := os.ReadFile(filePath)
	if string(restoredData) != initialContent {
		t.Fatalf("restored content mismatch: got %q, want %q", string(restoredData), initialContent)
	}

	// 5. Rewind conversation
	convEvents, err := mgr.RewindConversation("turn-1")
	if err != nil {
		t.Fatalf("rewind conversation error: %v", err)
	}
	if len(convEvents) != 2 {
		t.Fatalf("expected 2 conversation events, got %d", len(convEvents))
	}

	// 6. RewindAll
	allRes, allEvents, err := mgr.RewindAll("turn-1", true)
	if err != nil || len(allRes.Restored) != 1 || len(allEvents) != 2 {
		t.Fatalf("rewind all failed: %v %v %d", err, allRes, len(allEvents))
	}
}
