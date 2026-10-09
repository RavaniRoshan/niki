package tools

import (
	"context"
	"encoding/json"
	"fmt"
)

type WriteStdinTool struct {
	Base
	pm *ProcessManager
}

func NewWriteStdinTool(pm *ProcessManager) *WriteStdinTool {
	if pm == nil {
		pm = DefaultProcessManager()
	}
	return &WriteStdinTool{
		Base: Base{
			SchemaStr: `{"required":["process_id"],"fields":{"process_id":"string","input":"string","chars":"string","eof":"boolean","interrupt":"boolean"}}`,
		},
		pm: pm,
	}
}

func (t *WriteStdinTool) Name() string { return "write_stdin" }
func (t *WriteStdinTool) Description() string {
	return "Send interactive input, EOF, or interrupt signal to a background process"
}

type writeStdinArgs struct {
	ProcessID string `json:"process_id"`
	Input     string `json:"input,omitempty"`
	Chars     string `json:"chars,omitempty"`
	EOF       bool   `json:"eof,omitempty"`
	Interrupt bool   `json:"interrupt,omitempty"`
}

func (t *WriteStdinTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a writeStdinArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	payload := a.Input
	if payload == "" {
		payload = a.Chars
	}

	err := t.pm.WriteStdin(a.ProcessID, payload, a.Interrupt, a.EOF)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed writing to stdin of %s: %v", a.ProcessID, err), IsError: true}, nil
	}

	status := fmt.Sprintf("Successfully sent input to process %s", a.ProcessID)
	if a.Interrupt {
		status += " (sent SIGINT/interrupt)"
	}
	if a.EOF {
		status += " (sent EOF)"
	}
	return ToolResult{Output: status}, nil
}
