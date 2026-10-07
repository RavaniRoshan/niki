package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

type WriteFileTool struct{}

func NewWriteFileTool() *WriteFileTool { return &WriteFileTool{} }

func (t *WriteFileTool) Name() string        { return "write_file" }
func (t *WriteFileTool) Description() string { return "Write a file atomically" }

type writeFileArgs struct {
	Path    string `json:"path"`
	Content string `json:"content"`
}

func (t *WriteFileTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a writeFileArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(a.Path), 0o755); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	tmp, err := os.CreateTemp(filepath.Dir(a.Path), ".niki-write-*")
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.WriteString(a.Content); err != nil {
		tmp.Close()
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	tmp.Close()
	if err := os.Rename(tmp.Name(), a.Path); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	return ToolResult{Output: fmt.Sprintf("wrote %d bytes to %s", len(a.Content), a.Path)}, nil
}
