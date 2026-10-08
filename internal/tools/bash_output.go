package tools

import (
	"context"
	"encoding/json"
	"fmt"
)

type BashOutputTool struct {
	Base
	pm *ProcessManager
}

func NewBashOutputTool(pm *ProcessManager) *BashOutputTool {
	if pm == nil {
		pm = DefaultProcessManager()
	}
	return &BashOutputTool{
		Base: Base{
			SchemaStr: `{"required":["process_id"],"fields":{"process_id":"string","offset":"number","limit":"number"}}`,
		},
		pm: pm,
	}
}

func (t *BashOutputTool) Name() string        { return "bash_output" }
func (t *BashOutputTool) Description() string { return "Retrieve incremental or full terminal output and status from a running or completed process" }

type bashOutputArgs struct {
	ProcessID string `json:"process_id"`
	Offset    int    `json:"offset,omitempty"`
	Limit     int    `json:"limit,omitempty"`
}

func (t *BashOutputTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a bashOutputArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	chunk, running, exitCode, err := t.pm.ReadOutput(a.ProcessID, a.Offset, a.Limit)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("error reading process %s: %v", a.ProcessID, err), IsError: true}, nil
	}

	statusStr := "running"
	if !running {
		statusStr = fmt.Sprintf("exited (code %d)", exitCode)
	}

	output := fmt.Sprintf("Process %s status: %s\n--- Output ---\n%s", a.ProcessID, statusStr, chunk)
	return ToolResult{Output: output}, nil
}

func (t *BashOutputTool) IsConcurrencySafe() bool { return true }
func (t *BashOutputTool) IsReadOnly() bool        { return true }
