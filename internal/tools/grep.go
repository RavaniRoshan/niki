package tools

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

type GrepTool struct {
	Base
}

func NewGrepTool() *GrepTool { return &GrepTool{Base: Base{SchemaStr: `{"required":["pattern"],"fields":{"pattern":"string","root":"string","glob":"string"}}`}} }

func (t *GrepTool) Name() string        { return "grep" }
func (t *GrepTool) Description() string { return "Regex search across files" }

type grepArgs struct {
	Pattern string `json:"pattern"`
	Root    string `json:"root"`
	Glob    string `json:"glob"`
}

func (t *GrepTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a grepArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	re, err := regexp.Compile(a.Pattern)
	if err != nil {
		return ToolResult{Output: err.Error(), IsError: true}, nil
	}
	root := a.Root
	if root == "" {
		root = "."
	}
	var out []string
	_ = filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() && (d.Name() == ".git" || d.Name() == "node_modules") {
			return filepath.SkipDir
		}
		if d.IsDir() {
			return nil
		}
		if a.Glob != "" {
			ok, _ := filepath.Match(a.Glob, d.Name())
			if !ok {
				return nil
			}
		}
		f, err := os.Open(path)
		if err != nil {
			return nil
		}
		defer f.Close()
		sc := bufio.NewScanner(f)
		sc.Buffer(make([]byte, 1024*1024), 1024*1024)
		ln := 0
		for sc.Scan() {
			ln++
			if re.MatchString(sc.Text()) {
				out = append(out, fmt.Sprintf("%s:%d: %s", path, ln, sc.Text()))
			}
		}
		return nil
	})
	if len(out) > 200 {
		out = out[:200]
	}
	return ToolResult{Output: strings.Join(out, "\n")}, nil
}

func (t *GrepTool) IsConcurrencySafe() bool { return true }
func (t *GrepTool) IsReadOnly() bool        { return true }
