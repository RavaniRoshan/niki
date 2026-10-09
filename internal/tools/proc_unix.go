//go:build !windows

package tools

import (
	"os/exec"
	"syscall"
	"time"
)

func setProcessGroup(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

func killProcessGroup(cmd *exec.Cmd) {
	if cmd.Process == nil {
		return
	}
	pid := cmd.Process.Pid
	pgid, err := syscall.Getpgid(pid)
	target := -pid
	if err == nil && pgid > 0 {
		target = -pgid
	}

	// Send SIGINT first to allow graceful cleanup
	_ = syscall.Kill(target, syscall.SIGINT)

	// Escalate to SIGKILL after 500ms
	time.AfterFunc(500*time.Millisecond, func() {
		_ = syscall.Kill(target, syscall.SIGKILL)
	})
}
