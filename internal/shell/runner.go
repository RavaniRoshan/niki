package shell

import (
	"context"
	"os/exec"
	"time"
)

// Result holds the outcome of a direct shell command execution.
type Result struct {
	Command  string
	Output   string
	ExitCode int
	Err      error
}

// Run executes a shell command directly in the specified working directory with a 2-minute timeout.
func Run(cmdStr, dir string) Result {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	cmd := exec.CommandContext(ctx, "bash", "-c", cmdStr)
	if dir != "" {
		cmd.Dir = dir
	}
	out, err := cmd.CombinedOutput()
	exitCode := 0
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			exitCode = exitErr.ExitCode()
		} else {
			exitCode = 1
		}
	}
	return Result{
		Command:  cmdStr,
		Output:   string(out),
		ExitCode: exitCode,
		Err:      err,
	}
}
