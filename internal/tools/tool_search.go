package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"sync"
)

type ToolSearchTool struct {
	Base
	registry   *Registry
	mu         sync.RWMutex
	discovered map[string]bool
}

func NewToolSearchTool(r *Registry) *ToolSearchTool {
	return &ToolSearchTool{
		Base: Base{
			SchemaStr: `{"required":["query"],"fields":{"query":"string"}}`,
		},
		registry:   r,
		discovered: make(map[string]bool),
	}
}

func (t *ToolSearchTool) Name() string        { return "tool_search" }
func (t *ToolSearchTool) Description() string { return "Search and discover available tools by exact name, 'select:A,B,C', 'mcp__' prefix, or keyword matching" }

type toolSearchArgs struct {
	Query string `json:"query"`
}

func (t *ToolSearchTool) DiscoveredTools() []string {
	t.mu.RLock()
	defer t.mu.RUnlock()
	var list []string
	for k := range t.discovered {
		list = append(list, k)
	}
	sort.Strings(list)
	return list
}

func (t *ToolSearchTool) recordDiscovered(name string) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.discovered[name] = true
}

func (t *ToolSearchTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a toolSearchArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	q := strings.TrimSpace(a.Query)
	if q == "" {
		return ToolResult{Output: "query cannot be empty", IsError: true}, nil
	}

	if t.registry == nil {
		return ToolResult{Output: "tool registry not available", IsError: true}, nil
	}

	allTools := t.registry.List()

	// 1. Exact-name fast path
	for _, tool := range allTools {
		if strings.EqualFold(tool.Name(), q) {
			t.recordDiscovered(tool.Name())
			return ToolResult{
				Output: fmt.Sprintf("Tool %q found (exact match):\nDescription: %s\nSchema: %s\nReadOnly: %v\nConcurrencySafe: %v",
					tool.Name(), tool.Description(), tool.Schema(), tool.IsReadOnly(), tool.IsConcurrencySafe()),
			}, nil
		}
	}

	// 2. select:A,B,C direct load
	if strings.HasPrefix(strings.ToLower(q), "select:") {
		selectedList := strings.Split(strings.TrimPrefix(q, "select:"), ",")
		var matches []Tool
		for _, name := range selectedList {
			name = strings.TrimSpace(name)
			if tool, ok := t.registry.Get(name); ok {
				matches = append(matches, tool)
				t.recordDiscovered(tool.Name())
			}
		}
		if len(matches) == 0 {
			return ToolResult{Output: fmt.Sprintf("No tools found matching select query: %s", q)}, nil
		}
		var sb strings.Builder
		fmt.Fprintf(&sb, "Selected %d tools:\n\n", len(matches))
		for _, m := range matches {
			fmt.Fprintf(&sb, "- %s: %s (Schema: %s)\n", m.Name(), m.Description(), m.Schema())
		}
		return ToolResult{Output: strings.TrimSpace(sb.String())}, nil
	}

	// 3. mcp__ prefix
	if strings.HasPrefix(strings.ToLower(q), "mcp__") {
		var matches []Tool
		for _, tool := range allTools {
			if strings.HasPrefix(strings.ToLower(tool.Name()), strings.ToLower(q)) || strings.HasPrefix(tool.Name(), "mcp__") {
				matches = append(matches, tool)
				t.recordDiscovered(tool.Name())
			}
		}
		if len(matches) == 0 {
			return ToolResult{Output: fmt.Sprintf("No MCP tools found matching prefix: %s", q)}, nil
		}
		var sb strings.Builder
		fmt.Fprintf(&sb, "MCP tools matching %q (%d):\n\n", q, len(matches))
		for _, m := range matches {
			fmt.Fprintf(&sb, "- %s: %s\n", m.Name(), m.Description())
		}
		return ToolResult{Output: strings.TrimSpace(sb.String())}, nil
	}

	// 4. Keyword / BM25-style relevance fallback
	terms := strings.Fields(strings.ToLower(q))
	type scoredTool struct {
		tool  Tool
		score int
	}
	var scored []scoredTool

	for _, tool := range allTools {
		score := 0
		nameLower := strings.ToLower(tool.Name())
		descLower := strings.ToLower(tool.Description())
		schemaLower := strings.ToLower(tool.Schema())

		for _, term := range terms {
			if strings.Contains(nameLower, term) {
				score += 10
			}
			if strings.Contains(descLower, term) {
				score += 4
			}
			if strings.Contains(schemaLower, term) {
				score += 1
			}
		}
		if score > 0 {
			scored = append(scored, scoredTool{tool: tool, score: score})
			t.recordDiscovered(tool.Name())
		}
	}

	sort.Slice(scored, func(i, j int) bool {
		return scored[i].score > scored[j].score
	})

	if len(scored) == 0 {
		return ToolResult{Output: fmt.Sprintf("No tools found matching query %q", q)}, nil
	}

	var sb strings.Builder
	fmt.Fprintf(&sb, "Discovered %d relevant tools for %q:\n\n", len(scored), q)
	for i, s := range scored {
		fmt.Fprintf(&sb, "%d. %s (relevance: %d)\n   Description: %s\n   ReadOnly: %v\n",
			i+1, s.tool.Name(), s.score, s.tool.Description(), s.tool.IsReadOnly())
	}

	return ToolResult{Output: strings.TrimSpace(sb.String())}, nil
}

func (t *ToolSearchTool) IsConcurrencySafe() bool { return true }
func (t *ToolSearchTool) IsReadOnly() bool        { return true }
