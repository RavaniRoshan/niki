package sandbox

import (
	"context"
	"os"
	"strings"
	"testing"
)

func TestPassthroughRun(t *testing.T) {
	s := &Passthrough{}
	if s.Name() != "passthrough" {
		t.Fatal("name")
	}
	stdout, _, err := s.Run(context.Background(), "", "echo", "ok")
	if err != nil || strings.TrimSpace(stdout) != "ok" {
		t.Fatalf("out=%q err=%v", stdout, err)
	}
}

func TestSanitizedEnvStripsSecrets(t *testing.T) {
	t.Setenv("AWS_SECRET_ACCESS_KEY", "x")
	t.Setenv("SSH_AUTH_SOCK", "/tmp/sock")
	t.Setenv("OPENAI_API_KEY", "sk-test")
	t.Setenv("SAFE_VAR", "ok")
	env := SanitizedEnv()
	for _, kv := range env {
		if strings.HasPrefix(kv, "AWS_") || strings.HasPrefix(kv, "SSH_") || strings.HasPrefix(kv, "OPENAI_API_KEY") {
			t.Fatalf("secret leaked: %s", kv)
		}
	}
	found := false
	for _, kv := range env {
		if kv == "SAFE_VAR=ok" {
			found = true
		}
	}
	if !found {
		t.Fatal("safe var stripped")
	}
}

func TestExecIsolatedEnv(t *testing.T) {
	t.Setenv("AWS_SECRET_ACCESS_KEY", "x")
	s := &Passthrough{}
	cmd := s.ExecIsolated(context.Background(), t.TempDir(), "sh", "-c", "env")
	out, err := cmd.Output()
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(out), "AWS_SECRET_ACCESS_KEY") {
		t.Fatal("sandboxed env leaked AWS secret")
	}
}

// TestDetect reports the backend for this platform
// without asserting availability: the binary is an
// optional system dependency.
func TestDetect(t *testing.T) {
	name, ok := Detect()
	switch {
	case !ok:
		if name != "" {
			t.Fatalf("unavailable backend reported name %q", name)
		}
	case name != "bubblewrap" && name != "seatbelt":
		t.Fatalf("unexpected backend name %q", name)
	}
}

// TestBubblewrapArgv pins the containment
// argument vector: read-only root, private /tmp,
// unshared namespaces, dropped capabilities, and
// writable binds shadowing the read-only root.
func TestBubblewrapArgv(t *testing.T) {
	argv := bwrapArgv([]string{"/workspace"}, "/workspace", "bash", []string{"-c", "echo hi"})
	want := []string{
		"bwrap",
		"--ro-bind", "/", "/",
		"--dev", "/dev",
		"--proc", "/proc",
		"--tmpfs", "/tmp",
		"--unshare-all",
		"--die-with-parent",
		"--new-session",
		"--cap-drop", "ALL",
		"--bind", "/workspace", "/workspace",
		"--", "bash", "-c", "echo hi",
	}
	if len(argv) != len(want) {
		t.Fatalf("argv = %v", argv)
	}
	for i := range want {
		if argv[i] != want[i] {
			t.Fatalf("argv[%d] = %q, want %q (full: %v)", i, argv[i], want[i], argv)
		}
	}
}

// TestBubblewrapIsolation exercises the real
// backend when bubblewrap is installed: reads
// work, writes outside the writable binds and the
// private /tmp fail, and credentials are scrubbed.
func TestBubblewrapIsolation(t *testing.T) {
	if _, ok := Detect(); !ok {
		t.Skip("no sandbox backend installed on this system")
	}
	name, _ := Detect()
	if name != "bubblewrap" {
		t.Skipf("backend is %s, not bubblewrap", name)
	}
	t.Setenv("AWS_SECRET_ACCESS_KEY", "should-not-leak")
	dir := t.TempDir()
	s := &Bubblewrap{WritableDirs: []string{dir}}

	stdout, _, err := s.Run(context.Background(), dir, "cat", "/etc/passwd")
	if err != nil {
		t.Fatalf("read outside workspace failed: %v", err)
	}
	if !strings.Contains(stdout, "root:") {
		t.Fatalf("unexpected /etc/passwd content: %q", stdout[:min(len(stdout), 100)])
	}

	// Writing outside the writable binds must
	// fail: the root filesystem is read-only.
	_, stderr, err := s.Run(context.Background(), dir, "sh", "-c", "echo x > /nikicode-probe-ro")
	if err == nil {
		t.Fatal("write to read-only root unexpectedly succeeded")
	}
	if !strings.Contains(stderr, "Permission denied") && !strings.Contains(stderr, "Read-only file system") {
		t.Fatalf("unexpected write error: %v / %q", err, stderr)
	}

	// /tmp is a private writable tmpfs.
	if _, _, err := s.Run(context.Background(), dir, "sh", "-c", "echo x > /tmp/nikicode-probe-tmp"); err != nil {
		t.Fatalf("write to private /tmp failed: %v", err)
	}

	// The workspace bind is writable.
	if _, _, err := s.Run(context.Background(), dir, "sh", "-c", "echo x > "+dir+"/nikicode-probe-ws"); err != nil {
		t.Fatalf("write to workspace failed: %v", err)
	}

	// Credentials are scrubbed even inside the sandbox.
	out, _, err := s.Run(context.Background(), dir, "sh", "-c", "env")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(out, "AWS_SECRET_ACCESS_KEY") {
		t.Fatal("sandboxed env leaked AWS secret")
	}
}

// TestSeatbeltProfileRemovedOnPanic (S3): the
// generated profile file is removed even when the
// command panics, and the panic keeps propagating.
func TestSeatbeltProfileRemovedOnPanic(t *testing.T) {
	var profilePath string
	defer func() {
		if r := recover(); r == nil {
			t.Fatal("expected the panic to propagate")
		}
		if _, err := os.Stat(profilePath); !os.IsNotExist(err) {
			t.Fatalf("profile file survived the panic: %v", err)
		}
	}()
	_, _, _ = withProfile(nil, func(path string) (string, string, error) {
		profilePath = path
		panic("boom")
	})
}

// TestSeatbeltProfileRemovedOnReturn: the profile
// file is removed on the normal exit path too.
func TestSeatbeltProfileRemovedOnReturn(t *testing.T) {
	var profilePath string
	_, _, _ = withProfile(nil, func(path string) (string, string, error) {
		profilePath = path
		if _, err := os.Stat(path); err != nil {
			t.Fatalf("profile not written: %v", err)
		}
		return "", "", nil
	})
	if _, err := os.Stat(profilePath); !os.IsNotExist(err) {
		t.Fatalf("profile file survived a normal return: %v", err)
	}
}

// TestSeatbeltProfileContent: the profile denies
// network and writes globally, then re-allows
// writes to each writable directory.
func TestSeatbeltProfileContent(t *testing.T) {
	got := seatbeltProfile([]string{"/workspace", "/other"})
	for _, want := range []string{
		"(version 1)",
		"(allow default)",
		"(deny network*)",
		"(deny file-write*)",
		`(allow file-write* (subpath "/workspace"))`,
		`(allow file-write* (subpath "/other"))`,
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("profile missing %q:\n%s", want, got)
		}
	}
}
