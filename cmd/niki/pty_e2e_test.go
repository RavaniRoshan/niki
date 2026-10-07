package main

import (
	"os"
	"os/exec"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/creack/pty"
)

func ptyRun(t *testing.T, args ...string) (restore func()) {
	t.Helper()
	cmd := exec.Command("../../bin/niki", args...)
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	buf := make([]byte, 64*1024)
	go func() {
		for {
			n, err := f.Read(buf)
			if n > 0 {
				t.Logf("pty: %q", buf[:n])
			}
			if err != nil {
				return
			}
		}
	}()
	time.Sleep(500 * time.Millisecond)
	// Send Ctrl+C to exit the TUI.
	f.Write([]byte{0x03})
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		cmd.Process.Kill()
		t.Fatal("niki did not exit within 5s of Ctrl+C")
	}
	f.Close()
	return nil
}

func TestPTYRestoreOnCtrlC(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	ptyRun(t)
}

func TestPTYRestoreOnSIGTERM(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	cmd := exec.Command("../../bin/niki")
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	time.Sleep(500 * time.Millisecond)
	cmd.Process.Signal(syscall.SIGTERM)
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		cmd.Process.Kill()
		t.Fatal("niki did not exit within 5s of SIGTERM")
	}
	f.Close()
}

// TestNonTTYExec asserts plain output with no escapes (L4).
func TestNonTTYExec(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	out, err := exec.Command("../../bin/niki", "exec", "hello").Output()
	if err != nil {
		t.Fatal(err)
	}
	if strings.ContainsAny(string(out), "\x1b") {
		t.Fatalf("escape sequences present in non-TTY output: %q", out)
	}
}
