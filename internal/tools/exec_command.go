package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"time"
)

type ExecCommandTool struct {
	Base
	pm *ProcessManager
}

func NewExecCommandTool(pm *ProcessManager) *ExecCommandTool {
	if pm == nil {
		pm = DefaultProcessManager()
	}
	return &ExecCommandTool{
		Base: Base{
			SchemaStr: `{"required":["command"],"fields":{"command":"string","background":"boolean","dir":"string","timeout_seconds":"number"}}`,
		},
		pm: pm,
	}
}

func (t *ExecCommandTool) Name() string        { return "exec_command" }
func (t *ExecCommandTool) Description() string { return "Spawn an interactive or background command with PTY session support" }

type execCommandArgs struct {
	Command        string `json:"command"`
	Background     bool   `json:"background"`
	Dir            string `json:"dir,omitempty"`
	TimeoutSeconds int    `json:"timeout_seconds,omitempty"`
}

func (t *ExecCommandTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a execCommandArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	timeout := 30 * time.Second
	if a.TimeoutSeconds > 0 {
		timeout = time.Duration(a.TimeoutSeconds) * time.Second
	}

	proc, out, err := t.pm.Spawn(ctx, a.Command, a.Dir, a.Background, timeout)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed running command: %v\nOutput: %s", err, out), IsError: true}, nil
	}

	res := fmt.Sprintf("Process ID: %s\n%s", proc.ID, out)
	return ToolResult{Output: res}, nil
}
