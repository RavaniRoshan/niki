package tools

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"time"
)

type ShellTool struct {
	Base
}

func NewShellTool() *ShellTool { return &ShellTool{} }

func (t *ShellTool) Name() string        { return "shell" }
func (t *ShellTool) Description() string { return "Run a shell command" }

type shellArgs struct {
	Command string `json:"command"`
	Timeout int    `json:"timeout_seconds"`
	Dir     string `json:"dir"`
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
		return ToolResult{Output: out + "\n" + err.Error(), IsError: true}, nil
	}
	return ToolResult{Output: out}, nil
}
