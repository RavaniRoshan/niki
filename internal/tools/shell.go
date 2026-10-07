package tools

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"
)

type ShellTool struct {
	Base
}

func NewShellTool() *ShellTool { return &ShellTool{Base: Base{SchemaStr: `{"required":["command"],"fields":{"command":"string","timeout_seconds":"number","dir":"string"}}`}} }

func (t *ShellTool) Name() string        { return "shell" }
func (t *ShellTool) Description() string { return "Run a shell command" }

type shellArgs struct {
	Command string `json:"command"`
	Timeout int    `json:"timeout_seconds"`
	Dir     string `json:"dir"`
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
	cmd := exec.CommandContext(ctx, "bash", "-c", a.Command)
	if a.Dir != "" {
		cmd.Dir = a.Dir
	}
	var stdout, stderr bytes.Buffer
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
