// Package envctx collects the environment context once (OS, shell,
// working directory, git branch and status) and refreshes it when
// it changes (C9). Everything is best-effort: on any error the
// corresponding field is empty and collection never fails.
package envctx

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strings"
	"sync"
	"time"
)

// statusLineCap bounds the git status section of the context block.
const statusLineCap = 20

// Snapshot is one collection of environment facts.
type Snapshot struct {
	OS     string
	Shell  string
	Cwd    string
	Branch string
	Status string // truncated porcelain, "" when clean/unknown
}

// Collect gathers the current facts for cwd (git calls time out in
// 2s so a hung pager or huge repo can never stall a turn).
func Collect(cwd string) Snapshot {
	s := Snapshot{OS: runtime.GOOS, Cwd: cwd}
	s.Shell = os.Getenv("SHELL")
	if s.Shell == "" {
		s.Shell = os.Getenv("COMSPEC")
	}
	if s.Shell == "" {
		s.Shell = "unknown"
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	if out, err := exec.CommandContext(ctx, "git", "-C", cwd, "branch", "--show-current").Output(); err == nil {
		s.Branch = strings.TrimSpace(string(out))
	}
	if out, err := exec.CommandContext(ctx, "git", "-C", cwd, "status", "--porcelain=v1").Output(); err == nil {
		lines := strings.Split(strings.TrimRight(string(out), "\n"), "\n")
		if len(lines) == 1 && lines[0] == "" {
			lines = nil
		}
		if len(lines) > statusLineCap {
			lines = append(lines[:statusLineCap], fmt.Sprintf("... (%d more changed files)", len(lines)-statusLineCap))
		}
		s.Status = strings.Join(lines, "\n")
	}
	return s
}

// Equal reports whether two snapshots carry the same facts.
func (s Snapshot) Equal(o Snapshot) bool { return s == o }

// Block renders the snapshot as a context block for the model.
func (s Snapshot) Block() string {
	var b strings.Builder
	b.WriteString("<environment>\n")
	fmt.Fprintf(&b, "os: %s\nshell: %s\ncwd: %s\n", s.OS, s.Shell, s.Cwd)
	if s.Branch != "" {
		fmt.Fprintf(&b, "git_branch: %s\n", s.Branch)
	}
	if s.Status != "" {
		fmt.Fprintf(&b, "git_status:\n%s\n", s.Status)
	}
	b.WriteString("</environment>")
	return b.String()
}

// Watcher keeps the last snapshot and reports changes.
type Watcher struct {
	mu   sync.Mutex
	last Snapshot
	seen bool
}

// New returns an empty watcher (nothing collected yet).
func New() *Watcher { return &Watcher{} }

// Refresh collects fresh facts and reports the snapshot plus
// whether anything changed since the last refresh. The first call
// always reports changed (collection happened once here).
func (w *Watcher) Refresh(cwd string) (Snapshot, bool) {
	fresh := Collect(cwd)
	w.mu.Lock()
	defer w.mu.Unlock()
	changed := !w.seen || !fresh.Equal(w.last)
	w.last, w.seen = fresh, true
	return fresh, changed
}

// Current returns the last collected snapshot.
func (w *Watcher) Current() Snapshot {
	w.mu.Lock()
	defer w.mu.Unlock()
	return w.last
}
