package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// H1: `nikicode help` topics and `nikicode examples` — every example
// is runnable and checked by this docs-snippets test against the
// fixture (mock) provider.
func TestDocsSnippetsRun(t *testing.T) {
	dir := t.TempDir()
	bin := filepath.Join(dir, "nikicode")
	build := exec.Command("go", "build", "-o", bin, ".")
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build: %v\n%s", err, out)
	}
	if _, ok := HelpTopics["verifier"]; !ok {
		t.Fatalf("[H1] verifier topic missing")
	}
	t.Logf("[H1] %d help topics", len(HelpTopics))

	for _, e := range RunnableExamples() {
		if e.NeedsGo {
			if _, err := exec.LookPath("go"); err != nil {
				t.Logf("[H1] skip %q (no go toolchain)", e.Title)
				continue
			}
		}
		home := filepath.Join(dir, "home-"+strings.ReplaceAll(e.Title, " ", "_"))
		work := filepath.Join(dir, "work-"+strings.ReplaceAll(e.Title, " ", "_"))
		_ = os.MkdirAll(home, 0o755)
		_ = os.MkdirAll(work, 0o755)
		cwd := work
		if e.Fixture != nil {
			cwd = e.Fixture(work)
		}
		cmd := exec.Command(bin, e.Argv...)
		cmd.Dir = cwd
		cmd.Env = append(os.Environ(), "HOME="+home, "NIKI_MOCK=1", "NO_PRECONNECT=1")
		done := make(chan struct{})
		var out []byte
		var runErr error
		go func() {
			out, runErr = cmd.CombinedOutput()
			close(done)
		}()
		select {
		case <-done:
		case <-time.After(90 * time.Second):
			_ = cmd.Process.Kill()
			t.Fatalf("[H1] example %q timed out:\n%s", e.Title, out)
		}
		if runErr != nil {
			t.Fatalf("[H1] example %q exited %v:\n%s", e.Title, runErr, out)
		}
		if !strings.Contains(string(out), e.Expect) {
			t.Fatalf("[H1] example %q missing %q:\n%s", e.Title, e.Expect, out)
		}
		t.Logf("[H1] example %-18q runs (exit 0, contains %q)", e.Title, e.Expect)
	}
}
