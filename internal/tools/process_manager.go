package tools

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/creack/pty"
)

var globalProcCounter uint64

type ManagedProcess struct {
	ID        string
	Command   string
	Cmd       *exec.Cmd
	PtyFile   *os.File
	StdinPipe io.WriteCloser

	outputMu sync.Mutex
	output   bytes.Buffer

	stateMu  sync.RWMutex
	Exited   bool
	ExitCode int

	Done      chan struct{}
	StartTime time.Time
}

type ProcessManager struct {
	mu        sync.RWMutex
	processes map[string]*ManagedProcess
}

var defaultProcessManager = NewProcessManager()

func DefaultProcessManager() *ProcessManager {
	return defaultProcessManager
}

func NewProcessManager() *ProcessManager {
	return &ProcessManager{
		processes: make(map[string]*ManagedProcess),
	}
}

func (m *ProcessManager) Spawn(ctx context.Context, command string, dir string, background bool, timeout time.Duration) (*ManagedProcess, string, error) {
	procID := fmt.Sprintf("p%d", atomic.AddUint64(&globalProcCounter, 1))

	cmd := exec.Command("bash", "-c", command)
	if dir != "" {
		cmd.Dir = dir
	}
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}

	mp := &ManagedProcess{
		ID:        procID,
		Command:   command,
		Cmd:       cmd,
		Done:      make(chan struct{}),
		StartTime: time.Now(),
		ExitCode:  -1,
	}

	var pw *io.PipeWriter
	// Try pty start first
	ptmx, ptyErr := pty.Start(cmd)
	if ptyErr == nil {
		mp.PtyFile = ptmx
		go func() {
			buf := make([]byte, 4096)
			for {
				n, err := ptmx.Read(buf)
				if n > 0 {
					mp.outputMu.Lock()
					mp.output.Write(buf[:n])
					mp.outputMu.Unlock()
				}
				if err != nil {
					break
				}
			}
		}()
	} else {
		// Fallback to pipe if pty allocation fails. Re-create cmd to avoid "already started"
		cmd = exec.Command("bash", "-c", command)
		if dir != "" {
			cmd.Dir = dir
		}
		cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
		mp.Cmd = cmd

		var pr *io.PipeReader
		pr, pw = io.Pipe()
		cmd.Stdout = pw
		cmd.Stderr = pw
		stdinPipe, _ := cmd.StdinPipe()
		mp.StdinPipe = stdinPipe
		if err := cmd.Start(); err != nil {
			_ = pw.Close()
			return nil, "", fmt.Errorf("failed starting process: %w", err)
		}
		go func() {
			buf := make([]byte, 4096)
			for {
				n, err := pr.Read(buf)
				if n > 0 {
					mp.outputMu.Lock()
					mp.output.Write(buf[:n])
					mp.outputMu.Unlock()
				}
				if err != nil {
					break
				}
			}
		}()
	}

	// Waiter goroutine
	go func() {
		waitErr := cmd.Wait()
		if pw != nil {
			_ = pw.Close()
		}
		mp.stateMu.Lock()
		mp.Exited = true
		if waitErr != nil {
			if exitErr, ok := waitErr.(*exec.ExitError); ok {
				mp.ExitCode = exitErr.ExitCode()
			} else {
				mp.ExitCode = 1
			}
		} else {
			mp.ExitCode = 0
		}
		mp.stateMu.Unlock()
		if mp.PtyFile != nil {
			_ = mp.PtyFile.Close()
		}
		close(mp.Done)
	}()

	m.mu.Lock()
	m.processes[procID] = mp
	m.mu.Unlock()

	if background {
		return mp, fmt.Sprintf("Process %s started in background: %s (PID: %d)", procID, command, cmd.Process.Pid), nil
	}

	// Foreground mode: wait until timeout or completion
	if timeout <= 0 {
		timeout = 30 * time.Second
	}

	select {
	case <-mp.Done:
		mp.outputMu.Lock()
		out := mp.output.String()
		mp.outputMu.Unlock()
		return mp, out, nil
	case <-time.After(timeout):
		// Process is still running; background it
		return mp, fmt.Sprintf("Process %s still running after %v (PID: %d). Switched to background.", procID, timeout, cmd.Process.Pid), nil
	case <-ctx.Done():
		_ = m.Kill(procID, "SIGTERM")
		return mp, "Process cancelled by context", ctx.Err()
	}
}

func (m *ProcessManager) Get(id string) (*ManagedProcess, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	p, ok := m.processes[id]
	return p, ok
}

func (m *ProcessManager) WriteStdin(id string, input string, interrupt bool, eof bool) error {
	p, ok := m.Get(id)
	if !ok {
		return fmt.Errorf("process %s not found", id)
	}

	p.stateMu.RLock()
	exited := p.Exited
	p.stateMu.RUnlock()
	if exited {
		return fmt.Errorf("process %s has already exited", id)
	}

	if interrupt {
		if p.Cmd.Process != nil {
			_ = syscall.Kill(-p.Cmd.Process.Pid, syscall.SIGINT)
		}
		if p.PtyFile != nil {
			_, _ = p.PtyFile.Write([]byte{0x03}) // Ctrl+C
		}
		return nil
	}

	if input != "" {
		if p.PtyFile != nil {
			_, err := p.PtyFile.Write([]byte(input))
			if err != nil {
				return err
			}
		} else if p.StdinPipe != nil {
			_, err := p.StdinPipe.Write([]byte(input))
			if err != nil {
				return err
			}
		}
	}

	if eof {
		if p.PtyFile != nil {
			_, _ = p.PtyFile.Write([]byte{0x04}) // Ctrl+D
		} else if p.StdinPipe != nil {
			_ = p.StdinPipe.Close()
		}
	}

	return nil
}

func (m *ProcessManager) ReadOutput(id string, offset int, limit int) (string, bool, int, error) {
	p, ok := m.Get(id)
	if !ok {
		return "", false, -1, fmt.Errorf("process %s not found", id)
	}

	p.outputMu.Lock()
	all := p.output.String()
	p.outputMu.Unlock()

	p.stateMu.RLock()
	running := !p.Exited
	exitCode := p.ExitCode
	p.stateMu.RUnlock()

	if offset < 0 {
		offset = 0
	}
	if offset > len(all) {
		offset = len(all)
	}

	chunk := all[offset:]
	if limit > 0 && len(chunk) > limit {
		chunk = chunk[:limit]
	}

	return chunk, running, exitCode, nil
}

func (m *ProcessManager) Kill(id string, sig string) error {
	p, ok := m.Get(id)
	if !ok {
		return fmt.Errorf("process %s not found", id)
	}

	p.stateMu.RLock()
	exited := p.Exited
	p.stateMu.RUnlock()
	if exited {
		return nil
	}

	s := syscall.SIGTERM
	if sig == "SIGKILL" || sig == "KILL" {
		s = syscall.SIGKILL
	}

	if p.Cmd.Process != nil {
		pgid, err := syscall.Getpgid(p.Cmd.Process.Pid)
		if err == nil {
			_ = syscall.Kill(-pgid, s)
		} else {
			_ = syscall.Kill(-p.Cmd.Process.Pid, s)
		}
	}

	// Give a grace period if SIGTERM, then escalate to SIGKILL
	if s == syscall.SIGTERM {
		go func() {
			select {
			case <-p.Done:
			case <-time.After(3 * time.Second):
				if p.Cmd.Process != nil {
					_ = syscall.Kill(-p.Cmd.Process.Pid, syscall.SIGKILL)
				}
			}
		}()
	}

	return nil
}
