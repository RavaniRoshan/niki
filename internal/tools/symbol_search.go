package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/index"
)

// SymbolSearchTool searches code declarations across the workspace.
type SymbolSearchTool struct {
	Base
	idx *index.SymbolIndex
}

// NewSymbolSearchTool creates a SymbolSearchTool.
func NewSymbolSearchTool(idx *index.SymbolIndex) *SymbolSearchTool {
	return &SymbolSearchTool{
		Base: Base{
			SchemaStr: `{"required":["query"],"fields":{"query":"string"}}`,
		},
		idx: idx,
	}
}

func (t *SymbolSearchTool) Name() string              { return "symbol_search" }
func (t *SymbolSearchTool) Description() string       { return "Search workspace declarations (functions, types, interfaces, structs) by symbol name" }
func (t *SymbolSearchTool) IsReadOnly() bool          { return true }
func (t *SymbolSearchTool) IsConcurrencySafe() bool   { return true }

type symbolSearchArgs struct {
	Query string `json:"query"`
}

func (t *SymbolSearchTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a symbolSearchArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{IsError: true}, fmt.Errorf("bad args: %w", err)
	}

	if t.idx == nil {
		return ToolResult{Output: "Symbol index not initialized"}, nil
	}

	matches := t.idx.Search(a.Query, 15)
	if len(matches) == 0 {
		return ToolResult{Output: fmt.Sprintf("No symbols found matching %q", a.Query)}, nil
	}

	var sb strings.Builder
	fmt.Fprintf(&sb, "Found %d symbols matching %q:\n", len(matches), a.Query)
	for _, s := range matches {
		fmt.Fprintf(&sb, "  • %s (%s) — %s:%d\n    %s\n", s.Name, s.Kind, s.File, s.Line, s.Signature)
	}
	return ToolResult{Output: sb.String()}, nil
}
