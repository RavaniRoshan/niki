package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/RavaniRoshan/niki/internal/format"
)

type WriteFileTool struct {
	Base
}

func NewWriteFileTool() *WriteFileTool {
	return &WriteFileTool{Base: Base{SchemaStr: `{"version":"1", "required":["path","content"],"fields":{"path":"string","content":"string"}}`}}
}

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
	// E6: text tools never clobber binary/oversize targets; creating
	// a new file is always allowed.
	// N: keep the existing target's permission bits; new files get
	// the umask-normal default 0644 (never CreateTemp's 0600).
	targetMode := os.FileMode(0o644)
	if existing, rerr := os.ReadFile(a.Path); rerr == nil {
		if reason := nonTextReason(existing); reason != "" {
			return ToolResult{Output: "refusing to overwrite: " + reason, IsError: true}, nil
		}
		if fi, serr := os.Stat(a.Path); serr == nil {
			targetMode = fi.Mode().Perm()
		}
	}
	tmp, err := os.CreateTemp(filepath.Dir(a.Path), ".nikicode-write-*")
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.WriteString(a.Content); err != nil {
		tmp.Close()
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	tmp.Close()
	if err := os.Chmod(tmp.Name(), targetMode); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	if err := os.Rename(tmp.Name(), a.Path); err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	_ = format.FormatFile(ctx, a.Path)
	return ToolResult{Output: fmt.Sprintf("wrote %d bytes to %s", len(a.Content), a.Path)}, nil
}
