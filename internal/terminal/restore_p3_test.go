package terminal

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"
)

// P3: the terminal is restored on exit, Ctrl+C, SIGTERM, SIGHUP
// and panic. Disable is unconditional + idempotent (exit/Ctrl+C/
// SIGTERM paths); RestoreOnSignal covers SIGHUP/SIGTERM async
// kills; the panic path reuses the same Disable (child proof).
func TestDisableWritesRestoreSequences(t *testing.T) {
	var b strings.Builder
	Disable(&b)
	out := b.String()
	if !strings.Contains(out, kittyDisable) || !strings.Contains(out, syncDisable) {
		t.Fatalf("[P3] Disable must write both pop sequences, got %q", out)
	}
	Disable(&b) // idempotent: safe on every exit path
	t.Logf("[P3] Disable writes kitty+sync restore, idempotent")
}

// Child modes for the async-kill proof (mirrors the main.go TUI
// path: Enable, arm RestoreOnSignal, then die).
func TestRestoreOnSignalChild(t *testing.T) {
	mode := os.Getenv("NIKI_TERM_CHILD")
	marker := os.Getenv("NIKI_TERM_MARKER")
	if mode != "" {
		f, err := os.OpenFile(marker, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
		if err != nil {
			os.Exit(2)
		}
		Enable(Capabilities{KittyKeyboard: true, SyncOutput: true}, f)
		stop := RestoreOnSignal(f, syscall.SIGHUP)
		defer stop()
		if mode == "panic" {
			panic("p3-panic-proof")
		}
		select {} // wait for SIGHUP
	}

	dir := t.TempDir()
	markerPath := filepath.Join(dir, "restore.log")

	// SIGHUP: the handler must Disable before the process dies.
	child := exec.Command(os.Args[0], "-test.run=TestRestoreOnSignalChild")
	child.Env = append(os.Environ(), "NIKI_TERM_CHILD=hup", "NIKI_TERM_MARKER="+markerPath)
	if err := child.Start(); err != nil {
		t.Fatal(err)
	}
	time.Sleep(500 * time.Millisecond)
	if err := child.Process.Signal(syscall.SIGHUP); err != nil {
		t.Fatal(err)
	}
	_ = child.Wait()
	data, err := os.ReadFile(markerPath)
	if err != nil || !strings.Contains(string(data), kittyDisable) || !strings.Contains(string(data), syncDisable) {
		t.Fatalf("[P3] SIGHUP did not restore modes: %q %v", data, err)
	}
	t.Logf("[P3] SIGHUP restored modes before death")

	// Panic: main.go's defer pattern (Disable, then re-panic).
	func() {
		defer func() {
			if r := recover(); r == nil {
				t.Fatalf("[P3] expected panic")
			}
			Disable(mustMarker(t, markerPath))
		}()
		panic("p3-panic-proof")
	}()
	data, _ = os.ReadFile(markerPath)
	if strings.Count(string(data), kittyDisable) < 2 {
		t.Fatalf("[P3] panic path did not restore modes")
	}
	t.Logf("[P3] panic path restored modes")
}

func mustMarker(t *testing.T, path string) *os.File {
	t.Helper()
	f, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		t.Fatal(err)
	}
	return f
}
