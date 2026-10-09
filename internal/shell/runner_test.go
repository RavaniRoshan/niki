package shell

import (
	"strings"
	"testing"
)

func TestShellRunSuccess(t *testing.T) {
	res := Run("echo hello_runner", "")
	if res.ExitCode != 0 {
		t.Fatalf("expected exit code 0, got %d (err: %v)", res.ExitCode, res.Err)
	}
	if !strings.Contains(res.Output, "hello_runner") {
		t.Fatalf("expected output to contain hello_runner, got %q", res.Output)
	}
}

func TestShellRunNonZeroExit(t *testing.T) {
	res := Run("exit 7", "")
	if res.ExitCode != 7 {
		t.Fatalf("expected exit code 7, got %d", res.ExitCode)
	}
}
