package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// P2: works inside tmux — exec runs, the TUI paints, and a resize
// re-renders. (SSH has no server in this environment; the SSH limit
// is documented in the row evidence instead of tested.)
func TestWorksInsideTmux(t *testing.T) {
	if _, err := exec.LookPath("tmux"); err != nil {
		t.Skip("tmux not available")
	}
	dir := t.TempDir()
	bin := filepath.Join(dir, "nikicode")
	build := exec.Command("go", "build", "-o", bin, ".")
	build.Dir = "."
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build: %v\n%s", err, out)
	}
	sock := "niki-p2-" + fmt.Sprint(os.Getpid())
	tmux := func(args ...string) string {
		cmd := exec.Command("tmux", append([]string{"-L", sock}, args...)...)
		out, _ := cmd.CombinedOutput()
		return string(out)
	}
	defer exec.Command("tmux", "-L", sock, "kill-server").Run()
	home := filepath.Join(dir, "home")
	work := filepath.Join(dir, "work")
	if err := os.MkdirAll(home, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(work, 0o755); err != nil {
		t.Fatal(err)
	}

	// 1. Headless exec under tmux completes. Cwd is an empty dir so
	// the verifier concludes fast (no allowlisted checks there).
	execCmd := fmt.Sprintf("cd %s && NIKI_MOCK=1 HOME=%s %s exec 'say ok' 2>&1; echo TMUX_EXIT:$?; sleep 60", work, home, bin)
	tmux("new-session", "-d", "-s", "exec", "-x", "100", "-y", "30", execCmd)
	deadline := time.Now().Add(90 * time.Second)
	var pane string
	for {
		pane = tmux("capture-pane", "-p", "-t", "exec")
		if strings.Contains(pane, "TMUX_EXIT:") || time.Now().After(deadline) {
			break
		}
		time.Sleep(500 * time.Millisecond)
	}
	if !strings.Contains(pane, "TMUX_EXIT:") {
		t.Fatalf("[P2] exec under tmux never finished:\n%s", pane)
	}
	t.Logf("[P2] headless exec completes inside tmux")

	// 2. Interactive TUI paints inside tmux, and survives a resize.
	tuiCmd := fmt.Sprintf("cd %s && NIKI_MOCK=1 HOME=%s %s 2>&1", work, home, bin)
	tmux("new-session", "-d", "-s", "tui", "-x", "100", "-y", "30", tuiCmd)
	deadline = time.Now().Add(30 * time.Second)
	var painted string
	for {
		painted = tmux("capture-pane", "-p", "-t", "tui")
		if strings.Contains(painted, "NikiCode") || time.Now().After(deadline) {
			break
		}
		time.Sleep(500 * time.Millisecond)
	}
	if !strings.Contains(painted, "NikiCode") {
		t.Fatalf("[P2] TUI did not paint inside tmux:\n%s", painted)
	}
	tmux("resize-pane", "-t", "tui", "-x", "120", "-y", "40")
	time.Sleep(2 * time.Second)
	resized := tmux("capture-pane", "-p", "-t", "tui")
	if !strings.Contains(resized, "NikiCode") {
		t.Fatalf("[P2] TUI did not survive resize:\n%s", resized)
	}
	t.Logf("[P2] TUI paints inside tmux and re-renders after resize")
	tmux("kill-session", "-t", "tui")
	tmux("kill-session", "-t", "exec")
}
