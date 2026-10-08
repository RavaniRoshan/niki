package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
)

type TodoItem struct {
	ID     string `json:"id"`
	Text   string `json:"text"`
	Status string `json:"status"` // "todo" | "in_progress" | "done" | "cancelled"
}

type TodoWriteTool struct {
	Base
	mu    sync.RWMutex
	todos []TodoItem
}

func NewTodoWriteTool() *TodoWriteTool {
	return &TodoWriteTool{
		Base: Base{
			SchemaStr: `{"required":["todos"]}`,
		},
	}
}

func (t *TodoWriteTool) Name() string        { return "todo_write" }
func (t *TodoWriteTool) Description() string { return "Rewrite the session-scoped todo list atomically" }

type todoWriteArgs struct {
	Todos []TodoItem `json:"todos"`
}

func (t *TodoWriteTool) GetTodos() []TodoItem {
	t.mu.RLock()
	defer t.mu.RUnlock()
	res := make([]TodoItem, len(t.todos))
	copy(res, t.todos)
	return res
}

func (t *TodoWriteTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a todoWriteArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	for i, item := range a.Todos {
		if strings.TrimSpace(item.Text) == "" {
			return ToolResult{Output: fmt.Sprintf("todo %d missing text", i+1), IsError: true}, nil
		}
		status := strings.ToLower(strings.TrimSpace(item.Status))
		switch status {
		case "todo", "in_progress", "done", "cancelled":
		default:
			return ToolResult{Output: fmt.Sprintf("invalid status %q for todo %d (must be todo, in_progress, done, cancelled)", item.Status, i+1), IsError: true}, nil
		}
	}

	t.mu.Lock()
	t.todos = a.Todos
	t.mu.Unlock()

	var sb strings.Builder
	fmt.Fprintf(&sb, "Todo list updated (%d items):\n", len(a.Todos))
	for i, item := range a.Todos {
		icon := "[ ]"
		switch strings.ToLower(item.Status) {
		case "in_progress":
			icon = "[>]"
		case "done":
			icon = "[x]"
		case "cancelled":
			icon = "[-]"
		}
		idStr := ""
		if item.ID != "" {
			idStr = fmt.Sprintf(" (#%s)", item.ID)
		}
		fmt.Fprintf(&sb, "%d. %s %s%s\n", i+1, icon, item.Text, idStr)
	}

	return ToolResult{Output: strings.TrimSpace(sb.String())}, nil
}
