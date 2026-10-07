package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

type EditFileTool struct{}

func NewEditFileTool() *EditFileTool { return &EditFileTool{} }

func (t *EditFileTool) Name() string        { return "edit_file" }
func (t *EditFileTool) Description() string { return "Replace a unique string in a file" }

type editFileArgs struct {
	Path      string `json:"path"`
	OldString string `json:"old_string"`
	NewString string `json:"new_string"`
}

func (t *EditFileTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a editFileArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	data, err := os.ReadFile(a.Path)
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	n := strings.Count(string(data), a.OldString)
	if n != 1 {
		return ToolResult{Output: fmt.Sprintf("expected exactly 1 occurrence, found %d", n), IsError: true}, nil
	}
	updated := strings.Replace(string(data), a.OldString, a.NewString, 1)
	if err := os.WriteFile(a.Path, []byte(updated), 0o644); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	return ToolResult{Output: "edit applied"}, nil
}
