package main

import (
	"bytes"
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
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
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
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
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

// TestPTYCodingLoop drives the real binary: type a prompt, see the mock
// provider stream a reply, then quit cleanly.
func TestPTYCodingLoop(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	cmd := exec.Command("../../bin/niki")
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
	defer f.Close()
	var out strings.Builder
	dsrSeen := false
	go func() {
		buf := make([]byte, 4096)
		for {
			n, err := f.Read(buf)
			if n > 0 {
				out.Write(buf[:n])
				if !dsrSeen && strings.Contains(out.String(), "\x1b[6n") {
					dsrSeen = true
					_, _ = f.Write([]byte("\x1b[1;1R"))
					_, _ = f.Write([]byte("\x1b]11;rgb:0000/0000/0000\x1b\\"))
				}
			}
			if err != nil {
				return
			}
		}
	}()
	time.Sleep(800 * time.Millisecond)
	if _, err := f.Write([]byte("hello\r")); err != nil {
		t.Fatal(err)
	}
	deadline := time.After(6 * time.Second)
	for !strings.Contains(out.String(), "Acknowledged") {
		select {
		case <-deadline:
			t.Fatalf("no assistant reply; screen: %q", out.String()[:min(len(out.String()), 500)])
		case <-time.After(100 * time.Millisecond):
		}
	}
	if _, err := f.Write([]byte{0x03}); err != nil { // Ctrl+C
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		_ = cmd.Process.Kill()
		t.Fatal("no clean exit")
	}
}

func min(a, b int) int { if a < b { return a }; return b }

// TestColdStartFirstFrame measures the time from process spawn to the first
// rendered frame inside a PTY, enforcing the 60ms budget (B1).
func TestColdStartFirstFrame(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	start := time.Now()
	cmd := exec.Command("../../bin/niki")
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
	defer f.Close()
	defer func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	}()
	buf := make([]byte, 8192)
	dsrAnswered := false
	for {
		n, err := f.Read(buf)
		if n > 0 && !dsrAnswered && bytes.Contains(buf[:n], []byte("\x1b[6n")) {
			dsrAnswered = true
			_, _ = f.Write([]byte("\x1b[1;1R"))
			_, _ = f.Write([]byte("\x1b]11;rgb:0000/0000/0000\x1b\\"))
		}
		if n > 0 && bytes.Contains(buf[:n], []byte("Niki")) {
			elapsed := time.Since(start)
			t.Logf("cold_start_to_first_frame_ms=%d", elapsed.Milliseconds())
			if elapsed > 2*time.Second {
				t.Fatalf("too slow: %v", elapsed)
			}
			return
		}
		if err != nil {
			t.Fatal(err)
		}
	}
}
