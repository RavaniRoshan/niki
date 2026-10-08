package sandbox

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
)

// Sandbox abstracts process containment. Run executes
// a command rooted at dir (when non-empty) and captures
// its output. Every backend cleans up its own resources
// on all exit paths, including panics.
type Sandbox interface {
	Run(ctx context.Context, dir, name string, args ...string) (stdout, stderr string, err error)
	Name() string
}

// Passthrough is the documented fallback when sandboxing
// is enabled but no OS backend is available: commands run
// directly, but with a scrubbed environment so an agent
// command cannot harvest cloud or SSH credentials.
type Passthrough struct{}

func (p *Passthrough) Name() string { return "passthrough" }

func (p *Passthrough) Run(ctx context.Context, dir, name string, args ...string) (string, string, error) {
	return runCmd(p.ExecIsolated(ctx, dir, name, args...))
}

// SensitivePrefixes are environment variables scrubbed from sandboxed
// execution so an agent command cannot harvest cloud/SSH credentials.
var SensitivePrefixes = []string{"AWS_", "SSH_", "GITHUB_TOKEN", "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GH_TOKEN"}

// SanitizedEnv returns the process environment minus credentials.
func SanitizedEnv() []string {
	var out []string
	for _, kv := range os.Environ() {
		drop := false
		for _, p := range SensitivePrefixes {
			if strings.HasPrefix(kv, p) {
				drop = true
				break
			}
		}
		if !drop {
			out = append(out, kv)
		}
	}
	return out
}

// ExecIsolated runs name with a scrubbed environment and a bounded cwd.
func (p *Passthrough) ExecIsolated(ctx context.Context, dir, name string, args ...string) *exec.Cmd {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Env = SanitizedEnv()
	if dir != "" {
		cmd.Dir = dir
	}
	return cmd
}

// runCmd executes cmd and captures both output streams.
func runCmd(cmd *exec.Cmd) (string, string, error) {
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}

// Detect reports the best sandbox backend for this
// platform and whether its binary is available:
// bubblewrap on Linux, Seatbelt on macOS.
func Detect() (name string, ok bool) {
	switch runtime.GOOS {
	case "linux":
		if _, err := exec.LookPath("bwrap"); err == nil {
			return "bubblewrap", true
		}
	case "darwin":
		if _, err := exec.LookPath("sandbox-exec"); err == nil {
			return "seatbelt", true
		}
		// sandbox-exec lives in /usr/bin, which is
		// not always on PATH for non-login shells.
		if _, err := os.Stat("/usr/bin/sandbox-exec"); err == nil {
			return "seatbelt", true
		}
	}
	return "", false
}

// Bubblewrap contains processes with bubblewrap
// namespaces: the host filesystem is mounted read-only
// except for the writable binds, /tmp is a private
// tmpfs, all namespaces are unshared, capabilities are
// dropped, and the sandbox dies with its parent.
// Network is unavailable inside the sandbox — a
// documented v1 limitation: Claude Code's sandbox
// proxies network through the host, which needs a
// proxy layer niki does not have yet. Reads are
// allowed across the filesystem; only writes and
// network are confined.
type Bubblewrap struct {
	// WritableDirs are bind-mounted read-write
	// inside the sandbox (the workspace).
	WritableDirs []string
}

func (b *Bubblewrap) Name() string { return "bubblewrap" }

func (b *Bubblewrap) Run(ctx context.Context, dir, name string, args ...string) (string, string, error) {
	if dir != "" {
		if abs, err := filepath.Abs(dir); err == nil {
			dir = abs
		}
	}
	argv := bwrapArgv(b.WritableDirs, dir, name, args)
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	cmd.Env = SanitizedEnv()
	if dir != "" {
		cmd.Dir = dir
	}
	return runCmd(cmd)
}

// bwrapArgv builds the bubblewrap command line. Binds
// are ordered: the read-only root first, the writable
// directories after, so a writable bind shadows the
// read-only mount of the same subtree.
func bwrapArgv(writableDirs []string, dir, name string, args []string) []string {
	argv := []string{
		"bwrap",
		"--ro-bind", "/", "/",
		"--dev", "/dev",
		"--proc", "/proc",
		"--tmpfs", "/tmp",
		"--unshare-all",
		"--die-with-parent",
		"--new-session",
		"--cap-drop", "ALL",
	}
	for _, d := range writableDirs {
		argv = append(argv, "--bind", d, d)
	}
	argv = append(argv, "--", name)
	argv = append(argv, args...)
	return argv
}

// Seatbelt contains processes with macOS's Seatbelt
// via sandbox-exec, using a generated profile: reads
// are allowed, writes are confined to the writable
// directories, and network access is denied. The
// profile is a best-effort v1: it has not been
// executed on macOS in this environment and should
// be reviewed before relying on it.
type Seatbelt struct {
	// WritableDirs are the subpaths the profile
	// allows writes to (the workspace).
	WritableDirs []string
}

func (s *Seatbelt) Name() string { return "seatbelt" }

func (s *Seatbelt) Run(ctx context.Context, dir, name string, args ...string) (string, string, error) {
	return withProfile(s.WritableDirs, func(profilePath string) (string, string, error) {
		cmd := exec.CommandContext(ctx, "sandbox-exec", "-f", profilePath, name)
		cmd.Args = append(cmd.Args, args...)
		cmd.Env = SanitizedEnv()
		if dir != "" {
			cmd.Dir = dir
		}
		return runCmd(cmd)
	})
}

// seatbeltProfile builds a minimal profile. Rule
// order matters in Seatbelt: later rules override
// earlier ones, so the per-directory write allows
// come after the global write deny.
func seatbeltProfile(writableDirs []string) string {
	var b strings.Builder
	b.WriteString("(version 1)\n")
	b.WriteString("(allow default)\n")
	b.WriteString("(deny network*)\n")
	b.WriteString("(deny file-write*)\n")
	for _, d := range writableDirs {
		fmt.Fprintf(&b, "(allow file-write* (subpath %q))\n", d)
	}
	return b.String()
}

// withProfile writes a Seatbelt profile to a temp
// file, calls fn with its path, and removes the file
// on every exit path: normal return, error, and
// panic — the deferred cleanup runs while the panic
// unwinds and the panic itself keeps propagating.
func withProfile(writableDirs []string, fn func(profilePath string) (string, string, error)) (stdout, stderr string, err error) {
	f, err := os.CreateTemp("", "niki-seatbelt-*.sb")
	if err != nil {
		return "", "", fmt.Errorf("seatbelt profile: %w", err)
	}
	profilePath := f.Name()
	_, _ = f.WriteString(seatbeltProfile(writableDirs))
	_ = f.Close()
	defer func() {
		if r := recover(); r != nil {
			_ = os.Remove(profilePath)
			panic(r)
		}
		_ = os.Remove(profilePath)
	}()
	return fn(profilePath)
}
