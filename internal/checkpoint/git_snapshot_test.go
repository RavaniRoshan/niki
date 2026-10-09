package checkpoint

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestGitTreeSnapshotAndRewind(t *testing.T) {
	// Create a temporary git repo fixture
	repoDir := t.TempDir()
	cmd := exec.Command("git", "init")
	cmd.Dir = repoDir
	if err := cmd.Run(); err != nil {
		t.Skip("git not available in environment, skipping git tree test")
	}

	testFile := filepath.Join(repoDir, "hello.txt")
	os.WriteFile(testFile, []byte("version 1\n"), 0o644)

	mgr := NewManager(t.TempDir())
	sha, err := mgr.CreateGitTreeSnapshot("turn-1", repoDir)
	if err != nil {
		t.Fatalf("failed to create git tree snapshot: %v", err)
	}

	if len(sha) != 40 {
		t.Fatalf("expected 40-char SHA1 tree hash, got %q", sha)
	}

	// Modify file
	os.WriteFile(testFile, []byte("version 2 modified\n"), 0o644)

	// Rewind to tree SHA
	if err := mgr.RewindGitTree(repoDir, sha); err != nil {
		t.Fatalf("failed to rewind git tree: %v", err)
	}

	// Verify file is restored to version 1
	restored, err := os.ReadFile(testFile)
	if err != nil {
		t.Fatalf("failed reading restored file: %v", err)
	}

	if !strings.Contains(string(restored), "version 1") {
		t.Fatalf("expected restored content 'version 1', got %q", string(restored))
	}
}
