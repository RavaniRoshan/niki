package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"sync"
)

type ToolResult struct {
	Output string
	IsError bool
}

type Tool interface {
	Name() string
	Description() string
	Run(ctx context.Context, args json.RawMessage) (ToolResult, error)
}

type Registry struct {
	mu    sync.RWMutex
	tools map[string]Tool
}

func NewRegistry() *Registry {
	return &Registry{tools: map[string]Tool{}}
}

func (r *Registry) Register(t Tool) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.tools[t.Name()] = t
}

func (r *Registry) Get(name string) (Tool, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	t, ok := r.tools[name]
	return t, ok
}

func (r *Registry) List() []Tool {
	r.mu.RLock()
	defer r.mu.RUnlock()
	out := make([]Tool, 0, len(r.tools))
	for _, t := range r.tools {
		out = append(out, t)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name() < out[j].Name() })
	return out
}

func (r *Registry) Run(ctx context.Context, name string, args json.RawMessage) (ToolResult, error) {
	t, ok := r.Get(name)
	if !ok {
		return ToolResult{}, fmt.Errorf("unknown tool: %s", name)
	}
	return t.Run(ctx, args)
}

func DefaultRegistry() *Registry {
	r := NewRegistry()
	r.Register(NewReadFileTool())
	r.Register(NewWriteFileTool())
	r.Register(NewEditFileTool())
	r.Register(NewGlobTool())
	r.Register(NewGrepTool())
	r.Register(NewShellTool())
	return r
}
