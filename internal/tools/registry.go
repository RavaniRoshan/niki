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

// Base embeds fail-closed tool metadata: concurrency is unsafe and the tool
// is not read-only unless the tool opts in.
type Base struct{}

func (Base) IsConcurrencySafe() bool { return false }
func (Base) IsReadOnly() bool        { return false }

type Tool interface {
	Name() string
	Description() string
	Run(ctx context.Context, args json.RawMessage) (ToolResult, error)
	IsConcurrencySafe() bool
	IsReadOnly() bool
}

// Call is one tool invocation in a batch.
type Call struct {
	ID   string
	Name string
	Args json.RawMessage
}

// Batch runs calls concurrently iff every tool reports IsConcurrencySafe;
// otherwise calls run sequentially. Concurrency is capped; results return in
// the original call order (A2).
func (r *Registry) Batch(ctx context.Context, calls []Call) []ToolResult {
	allSafe := len(calls) > 0
	for _, c := range calls {
		t, ok := r.Get(c.Name)
		if !ok || !t.IsConcurrencySafe() {
			allSafe = false
			break
		}
	}
	results := make([]ToolResult, len(calls))
	if !allSafe {
		for i, c := range calls {
			res, err := r.Run(ctx, c.Name, c.Args)
			if err != nil {
				res = ToolResult{Output: err.Error(), IsError: true}
			}
			results[i] = res
		}
		return results
	}
	sem := make(chan struct{}, 10)
	var wg sync.WaitGroup
	for i, c := range calls {
		wg.Add(1)
		sem <- struct{}{}
		go func(i int, c Call) {
			defer wg.Done()
			defer func() { <-sem }()
			res, err := r.Run(ctx, c.Name, c.Args)
			if err != nil {
				res = ToolResult{Output: err.Error(), IsError: true}
			}
			results[i] = res
		}(i, c)
	}
	wg.Wait()
	return results
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
