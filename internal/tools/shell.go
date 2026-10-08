package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/sandbox"
)

type ShellTool struct {
	Base
	// Sandbox contains command execution when the
	// sandbox is enabled. nil runs commands
	// directly (sandbox disabled, current
	// behavior).
	Sandbox sandbox.Sandbox
	// ExcludedCommands run unsandboxed (still with
	// a scrubbed environment) because they need
	// access a sandbox cannot provide. Matching
	// is a case-insensitive substring.
	ExcludedCommands []string
	// AllowUnsandboxed permits the
	// dangerously_disable_sandbox argument.
	AllowUnsandboxed bool
}

func NewShellTool() *ShellTool { return &ShellTool{Base: Base{SchemaStr: `{"required":["command"],"fields":{"command":"string","timeout_seconds":"number","dir":"string","dangerously_disable_sandbox":"boolean"}}`}} }

func (t *ShellTool) Name() string        { return "shell" }
func (t *ShellTool) Description() string { return "Run a shell command" }

type shellArgs struct {
	Command                 string `json:"command"`
	Timeout                 int    `json:"timeout_seconds"`
	Dir                     string `json:"dir"`
	DangerouslyDisableSandbox bool `json:"dangerously_disable_sandbox"`
}

const maxInlineOutput = 4096

// summarize keeps the inline output bounded and persists full output to disk.
func summarize(full string) string {
	if len(full) <= maxInlineOutput {
		return full
	}
	dir := filepath.Join(os.TempDir(), "niki-tool-output")
	_ = os.MkdirAll(dir, 0o755)
	path := filepath.Join(dir, "out.log")
	_ = os.WriteFile(path, []byte(full), 0o600)
	return full[:maxInlineOutput] + "\n... [truncated, full output at " + path + "]"
}

func (t *ShellTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a shellArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	if a.Timeout <= 0 {
		a.Timeout = 120
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(a.Timeout)*time.Second)
	defer cancel()

	dir := a.Dir
	if dir == "" {
		dir, _ = os.Getwd()
	}

	if t.Sandbox != nil {
		// Sandbox policy (S1): commands run contained
		// by default. The escape hatch and excluded
		// commands run unsandboxed with a scrubbed
		// environment.
		var stdout, stderr string
		var err error
		switch {
		case a.DangerouslyDisableSandbox:
			if !t.AllowUnsandboxed {
				return ToolResult{Output: "dangerously_disable_sandbox is not permitted by config (sandbox.allow_unsandboxed = false)", IsError: true}, nil
			}
			stdout, stderr, err = (&sandbox.Passthrough{}).Run(ctx, dir, "bash", "-c", a.Command)
		case t.excluded(a.Command):
			stdout, stderr, err = (&sandbox.Passthrough{}).Run(ctx, dir, "bash", "-c", a.Command)
		default:
			stdout, stderr, err = t.Sandbox.Run(ctx, dir, "bash", "-c", a.Command)
		}
		out := stdout
		if stderr != "" {
			out += "\n[stderr]\n" + stderr
		}
		if err != nil {
			return ToolResult{Output: summarize(out + "\n" + err.Error()), IsError: true}, nil
		}
		return ToolResult{Output: summarize(out)}, nil
	}

	cmd := exec.CommandContext(ctx, "bash", "-c", a.Command)
	if a.Dir != "" {
		cmd.Dir = a.Dir
	}
	var stdout, stderr strings.Builder
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	out := stdout.String()
	if stderr.Len() > 0 {
		out += "\n[stderr]\n" + stderr.String()
	}
	if err != nil {
		out = out + "\n" + err.Error()
		return ToolResult{Output: summarize(out), IsError: true}, nil
	}
	return ToolResult{Output: summarize(out)}, nil
}

// excluded reports whether the command matches an
// excluded pattern (case-insensitive substring).
func (t *ShellTool) excluded(command string) bool {
	c := strings.ToLower(command)
	for _, pattern := range t.ExcludedCommands {
		if pattern == "" {
			continue
		}
		if strings.Contains(c, strings.ToLower(pattern)) {
			return true
		}
	}
	return false
}
