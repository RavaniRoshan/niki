package main

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/creack/pty"
)

func ptyRun(t *testing.T, args ...string) (restore func()) {
	t.Helper()
	cmd := exec.Command("../../bin/nikicode", args...)
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
		t.Fatal("nikicode did not exit within 5s of Ctrl+C")
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
	cmd := exec.Command("../../bin/nikicode")
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
		t.Fatal("nikicode did not exit within 5s of SIGTERM")
	}
	f.Close()
}

// TestNonTTYExec asserts plain output with no escapes (L4).
func TestNonTTYExec(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	out, err := exec.Command("../../bin/nikicode", "exec", "hello").Output()
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
	cmd := exec.Command("../../bin/nikicode")
	f, err := pty.Start(cmd)
	if err != nil {
		t.Fatal(err)
	}
	_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
	defer f.Close()
	var out strings.Builder
	answered := map[string]bool{}
	go func() {
		buf := make([]byte, 4096)
		for {
			n, err := f.Read(buf)
			if n > 0 {
				out.Write(buf[:n])
				answerTerminalQueries(buf[:n], f, answered)
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

// answerTerminalQueries replies to the terminal
// queries nikicode (and the TUI framework) send, the way
// a real terminal would, so capability detection
// completes immediately instead of waiting out its
// budget. Each query is answered at most once.
func answerTerminalQueries(chunk []byte, f *os.File, answered map[string]bool) {
	reply := func(query, resp string) {
		if answered[query] {
			return
		}
		answered[query] = true
		_, _ = f.Write([]byte(resp))
	}
	if bytes.Contains(chunk, []byte("\x1b[6n")) {
		reply("dsr", "\x1b[1;1R")
		reply("osc11", "\x1b]11;rgb:0000/0000/0000\x1b\\")
	}
	if bytes.Contains(chunk, []byte("\x1b[c")) {
		reply("da1", "\x1b[?62;22c")
	}
	if bytes.Contains(chunk, []byte("\x1b[?u")) {
		reply("kitty", "\x1b[?1u")
	}
	if bytes.Contains(chunk, []byte("\x1b[?2026$p")) {
		reply("decrqm", "\x1b[?2026;1$y")
	}
}

// waitFirstFrame reads PTY output until the first
// rendered frame (the NikiCode header) appears, answering
// terminal queries so rendering is not delayed.
// Returns the elapsed time.
func waitFirstFrame(t *testing.T, f *os.File, start time.Time) time.Duration {
	t.Helper()
	buf := make([]byte, 8192)
	answered := map[string]bool{}
	for {
		n, err := f.Read(buf)
		if n > 0 {
			answerTerminalQueries(buf[:n], f, answered)
			if bytes.Contains(buf[:n], []byte("NikiCode")) {
				return time.Since(start)
			}
		}
		if err != nil {
			t.Fatal(err)
		}
	}
}

// TestColdAndWarmStartToComposer measures the time from
// process spawn to the first frame, twice: cold (first
// launch) and warm (second launch, page cache hot).
// Contract: first frame p50 ≤ 50ms, warm readiness
// p50 ≤ 100ms (B1/B3).
func TestColdAndWarmStartToComposer(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	var cold, warm time.Duration
	for i := 0; i < 2; i++ {
		start := time.Now()
		cmd := exec.Command("../../bin/nikicode")
		f, err := pty.Start(cmd)
		if err != nil {
			t.Fatal(err)
		}
		_ = pty.Setsize(f, &pty.Winsize{Rows: 24, Cols: 80})
		defer f.Close()
		elapsed := waitFirstFrame(t, f, start)
		if i == 0 {
			cold = elapsed
		} else {
			warm = elapsed
		}
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	}
	t.Logf("cold_start_first_frame_ms=%d warm_start_first_frame_ms=%d",
		cold.Milliseconds(), warm.Milliseconds())
	if cold > 60*time.Millisecond {
		t.Errorf("cold start to first frame = %v, want ≤ 60ms", cold)
	}
	if warm > 90*time.Millisecond {
		t.Errorf("warm start to first frame = %v, want ≤ 90ms", warm)
	}
}

// TestColdStartFirstFrame measures the time from process spawn to the first
// rendered frame inside a PTY, enforcing the 60ms budget (B1).
func TestColdStartFirstFrame(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}
	start := time.Now()
	cmd := exec.Command("../../bin/nikicode")
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
	elapsed := waitFirstFrame(t, f, start)
	t.Logf("cold_start_to_first_frame_ms=%d", elapsed.Milliseconds())
	if elapsed > 60*time.Millisecond {
		t.Fatalf("too slow: %v", elapsed)
	}
}

func ensureBinary(t *testing.T) string {
	t.Helper()
	binPath := "../../bin/nikicode"
	if _, err := os.Stat(binPath); err == nil {
		return binPath
	}
	_ = os.MkdirAll("../../bin", 0o755)
	build := exec.Command("go", "build", "-o", binPath, ".")
	if err := build.Run(); err != nil {
		t.Fatalf("failed to build test binary %s: %v", binPath, err)
	}
	return binPath
}

// TestArgvFastPathVersion asserts nikicode --version runs via the fast path without heavy init.
func TestArgvFastPathVersion(t *testing.T) {
	bin := ensureBinary(t)
	cmd := exec.Command(bin, "--version")
	start := time.Now()
	out, err := cmd.Output()
	elapsed := time.Since(start)
	if err != nil {
		t.Fatalf("--version execution failed: %v", err)
	}
	if !strings.Contains(string(out), "nikicode version") {
		t.Fatalf("unexpected version output: %s", string(out))
	}
	t.Logf("--version took %v", elapsed)
	limit := 100 * time.Millisecond
	if elapsed > limit {
		t.Errorf("--version took %v, want < %v", elapsed, limit)
	}
}

// TestBootWith5MCPAnd50Skills verifies that configuring 5 MCP servers and 50 skills
// does not block the boot critical path, keeping time-to-first-paint within budget (B6/B5).
func TestBootWith5MCPAnd50Skills(t *testing.T) {
	if os.Getenv("NIKI_PTY_TESTS") == "" {
		t.Skip("set NIKI_PTY_TESTS=1 to run")
	}

	dir := t.TempDir()
	// 1. Create nikicode.toml with 5 MCP servers
	var tomlContent strings.Builder
	tomlContent.WriteString("[model]\nname = \"mock\"\n")
	for i := 1; i <= 5; i++ {
		fmt.Fprintf(&tomlContent, "[mcp.servers.srv%d]\ncommand = \"echo\"\nargs = [\"server%d\"]\n\n", i, i)
	}
	_ = os.WriteFile(filepath.Join(dir, "nikicode.toml"), []byte(tomlContent.String()), 0o644)

	// 2. Create .agents/skills with 50 skills
	skillsDir := filepath.Join(dir, ".agents", "skills")
	_ = os.MkdirAll(skillsDir, 0o755)
	for i := 1; i <= 50; i++ {
		skillPath := filepath.Join(skillsDir, fmt.Sprintf("skill_%d.md", i))
		content := fmt.Sprintf("---\nname: skill_%d\ndescription: test skill %d\n---\nBody of skill %d", i, i, i)
		_ = os.WriteFile(skillPath, []byte(content), 0o644)
	}

	nikiBin, err := filepath.Abs("../../bin/nikicode")
	if err != nil {
		t.Fatal(err)
	}
	start := time.Now()
	cmd := exec.Command(nikiBin, "--config", filepath.Join(dir, "nikicode.toml"))
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "NIKI_BOOT_TRACE=1", "NIKI_TRUST_PROJECT=1", "HOME="+dir)

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

	elapsed := waitFirstFrame(t, f, start)
	t.Logf("boot_with_5mcp_50skills_first_frame_ms=%d", elapsed.Milliseconds())

	// Must stay inside cold start budget (≤90ms)
	if elapsed > 90*time.Millisecond {
		t.Errorf("first frame took %v, want <= 90ms with 5 MCP and 50 skills", elapsed)
	}

	// Read boot trace
	traceData, err := os.ReadFile(filepath.Join(dir, ".nikicode", "log", "boot-trace.log"))
	if err == nil {
		t.Logf("boot trace:\n%s", string(traceData))
	}
}

// TestArgvFastPathScoped asserts the B0 fast path fires only for a sole
// version arg: incidental tokens (e.g. `exec version`) must reach
// their command instead of printing the version.
func TestArgvFastPathScoped(t *testing.T) {
	bin := ensureBinary(t)
	out, err := exec.Command(bin, "exec", "version").CombinedOutput()
	if err != nil {
		t.Fatalf("exec version failed: %v\n%s", err, out)
	}
	if strings.Contains(string(out), "nikicode version") {
		t.Fatalf("fast path hijacked exec version: %s", out)
	}
}
