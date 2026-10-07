package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type GlobTool struct {
	Base
}

func NewGlobTool() *GlobTool { return &GlobTool{Base: Base{SchemaStr: `{"required":["pattern"],"fields":{"pattern":"string","root":"string"}}`}} }

func (t *GlobTool) Name() string        { return "glob" }
func (t *GlobTool) Description() string { return "Find files matching a glob pattern" }

type globArgs struct {
	Pattern string `json:"pattern"`
	Root    string `json:"root"`
}

func (t *GlobTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a globArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	root := a.Root
	if root == "" {
		root = "."
	}
	var matches []string
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() && (d.Name() == ".git" || d.Name() == "node_modules") {
			return filepath.SkipDir
		}
		ok, _ := filepath.Match(a.Pattern, d.Name())
		if ok && !d.IsDir() {
			matches = append(matches, path)
		}
		if strings.Contains(a.Pattern, "**") {
			ok, _ := filepath.Match(filepath.Base(a.Pattern), d.Name())
			if ok && !d.IsDir() {
				matches = append(matches, path)
			}
		}
		return nil
	})
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	seen := map[string]bool{}
	var uniq []string
	for _, m := range matches {
		if !seen[m] {
			seen[m] = true
			uniq = append(uniq, m)
		}
	}
	return ToolResult{Output: strings.Join(uniq, "\n")}, nil
}

func (t *GlobTool) IsConcurrencySafe() bool { return true }
func (t *GlobTool) IsReadOnly() bool        { return true }
