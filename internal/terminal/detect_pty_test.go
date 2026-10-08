package terminal

import (
	"os"
	"os/exec"
	"testing"
	"time"

	"github.com/creack/pty"
)

// TestDetectUnderPTY runs Detect in a child process with
// a real controlling terminal and bounds the total time.
func TestDetectUnderPTY(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	cmd := exec.Command(os.Args[0], "-test.run=TestDetectChildHelper")
	cmd.Env = append(os.Environ(), "NIKI_PTY_CHILD=1")
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("child failed: %v", err)
		}
	case <-time.After(5 * time.Second):
		_ = cmd.Process.Kill()
		t.Fatal("Detect hung for over 5s under a PTY")
	}
}

// TestDetectChildHelper is the child side: it runs Detect
// and reports success via exit code.
func TestDetectChildHelper(t *testing.T) {
	if os.Getenv("NIKI_PTY_CHILD") == "" {
		t.Skip("helper")
	}
	start := time.Now()
	_, err := Detect(400 * time.Millisecond)
	t.Logf("detect took %s err=%v", time.Since(start), err)
	if time.Since(start) > 3*time.Second {
		t.Fatalf("Detect took %s, want < 3s", time.Since(start))
	}
}
