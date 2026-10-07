package tools

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

const maxReadLines = 2000

type ReadFileTool struct {
	Base
}

func NewReadFileTool() *ReadFileTool { return &ReadFileTool{} }

func (t *ReadFileTool) Name() string        { return "read_file" }
func (t *ReadFileTool) Description() string { return "Read a file with optional line offset/limit" }

type readFileArgs struct {
	Path   string `json:"path"`
	Offset int    `json:"offset"`
	Limit  int    `json:"limit"`
}

func (t *ReadFileTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a readFileArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	f, err := os.Open(a.Path)
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	defer f.Close()

	var b strings.Builder
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1024*1024), 1024*1024)
	line := 0
	written := 0
	for sc.Scan() {
		line++
		if line <= a.Offset {
			continue
		}
		if a.Limit > 0 && written >= a.Limit {
			break
		}
		if written >= maxReadLines {
			break
		}
		b.WriteString(sc.Text())
		b.WriteString("\n")
		written++
	}
	return ToolResult{Output: b.String()}, nil
}

func (t *ReadFileTool) IsConcurrencySafe() bool { return true }
func (t *ReadFileTool) IsReadOnly() bool        { return true }
