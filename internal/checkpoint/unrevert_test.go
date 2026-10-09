package checkpoint

import (
	"os"
	"path/filepath"
	"testing"
)

func TestUnrevertRestoresPreRewindState(t *testing.T) {
	tmpDir := t.TempDir()
	mgr := NewManager(filepath.Join(tmpDir, "checkpoints"))

	file1 := filepath.Join(tmpDir, "sample.txt")
	_ = os.WriteFile(file1, []byte("v1: original code"), 0o644)

	// Create checkpoint 1
	cp1, err := mgr.CreateCheckpoint("turn-1", []string{file1}, nil)
	if err != nil {
		t.Fatalf("failed creating checkpoint: %v", err)
	}

	// Modify file to v2
	_ = os.WriteFile(file1, []byte("v2: modified code"), 0o644)

	if mgr.CanUnrevert() {
		t.Fatal("CanUnrevert should be false before any rewind")
	}

	// Rewind to turn-1 (force=true)
	_, err = mgr.RewindCode(cp1.TurnID, true)
	if err != nil {
		t.Fatalf("failed rewinding code: %v", err)
	}

	// Verify file is back to v1
	data, _ := os.ReadFile(file1)
	if string(data) != "v1: original code" {
		t.Fatalf("expected 'v1: original code', got %q", string(data))
	}

	if !mgr.CanUnrevert() {
		t.Fatal("CanUnrevert should be true after rewind")
	}

	// Unrevert back to v2
	res, err := mgr.Unrevert()
	if err != nil {
		t.Fatalf("unrevert failed: %v", err)
	}
	if len(res.Restored) != 1 {
		t.Fatalf("expected 1 restored file, got %d", len(res.Restored))
	}

	// Verify file is back to v2
	data, _ = os.ReadFile(file1)
	if string(data) != "v2: modified code" {
		t.Fatalf("expected 'v2: modified code' after unrevert, got %q", string(data))
	}
}
