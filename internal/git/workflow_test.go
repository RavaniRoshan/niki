package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestSmartCommitAndPRSummary(t *testing.T) {
	dir := t.TempDir()

	// Initialize throwaway git repo
	if err := exec.Command("git", "-C", dir, "init", "-b", "main").Run(); err != nil {
		t.Skip("git init failed")
	}
	_ = exec.Command("git", "-C", dir, "config", "user.email", "test@test.local").Run()
	_ = exec.Command("git", "-C", dir, "config", "user.name", "Tester").Run()

	file1 := filepath.Join(dir, "README.md")
	_ = os.WriteFile(file1, []byte("# Test Repo\n"), 0o644)
	_ = exec.Command("git", "-C", dir, "add", ".").Run()
	_ = exec.Command("git", "-C", dir, "commit", "-m", "init").Run()

	// Create new branch and modify code file
	_ = exec.Command("git", "-C", dir, "checkout", "-b", "feature-tui").Run()
	codeFile := filepath.Join(dir, "tui_view.go")
	_ = os.WriteFile(codeFile, []byte("package main\n"), 0o644)
	_ = exec.Command("git", "-C", dir, "add", ".").Run()

	msg, err := SmartCommitMessage(dir)
	if err != nil {
		t.Fatalf("SmartCommitMessage failed: %v", err)
	}
	if !strings.Contains(msg, "tui") || !strings.Contains(msg, "tui_view.go") {
		t.Fatalf("unexpected commit message: %s", msg)
	}

	_ = exec.Command("git", "-C", dir, "commit", "-m", msg).Run()

	pr, err := PRSummary(dir, "main")
	if err != nil {
		t.Fatalf("PRSummary failed: %v", err)
	}
	if !strings.Contains(pr, "Pull Request: feature-tui -> main") || !strings.Contains(pr, "tui_view.go") {
		t.Fatalf("unexpected PR summary: %s", pr)
	}
}
