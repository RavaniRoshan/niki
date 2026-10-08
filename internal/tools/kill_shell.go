package tools

import (
	"context"
	"encoding/json"
	"fmt"
)

type KillShellTool struct {
	Base
	pm *ProcessManager
}

func NewKillShellTool(pm *ProcessManager) *KillShellTool {
	if pm == nil {
		pm = DefaultProcessManager()
	}
	return &KillShellTool{
		Base: Base{
			SchemaStr: `{"required":["process_id"],"fields":{"process_id":"string","signal":"string"}}`,
		},
		pm: pm,
	}
}

func (t *KillShellTool) Name() string        { return "kill_shell" }
func (t *KillShellTool) Description() string { return "Terminate a running background process or process group using SIGTERM or SIGKILL" }

type killShellArgs struct {
	ProcessID string `json:"process_id"`
	Signal    string `json:"signal,omitempty"` // "SIGTERM" | "SIGKILL"
}

func (t *KillShellTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a killShellArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	sig := a.Signal
	if sig == "" {
		sig = "SIGTERM"
	}

	err := t.pm.Kill(a.ProcessID, sig)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed to kill process %s: %v", a.ProcessID, err), IsError: true}, nil
	}

	return ToolResult{Output: fmt.Sprintf("Process %s termination signal %s sent", a.ProcessID, sig)}, nil
}
