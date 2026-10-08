package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"time"
)

// SpawnAgentTool creates a new subagent in the execution hierarchy.
type SpawnAgentTool struct {
	Base
	ctrl AgentController
}

func NewSpawnAgentTool(ctrl AgentController) *SpawnAgentTool {
	return &SpawnAgentTool{
		Base: Base{
			SchemaStr: `{"required":["name","prompt"],"fields":{"name":"string","prompt":"string","parent_id":"string","context_mode":"string","worktree":"boolean","budget_tokens":"number"}}`,
		},
		ctrl: ctrl,
	}
}

func (t *SpawnAgentTool) Name() string        { return "spawn_agent" }
func (t *SpawnAgentTool) Description() string { return "Spawn an isolated subagent with hierarchical pathing, runaway controls, and optional worktree isolation" }

type spawnAgentArgs struct {
	Name         string `json:"name"`
	Prompt       string `json:"prompt"`
	ParentID     string `json:"parent_id,omitempty"`
	ContextMode  string `json:"context_mode,omitempty"` // "none" | "all" | "recent_N"
	Worktree     bool   `json:"worktree,omitempty"`
	BudgetTokens int    `json:"budget_tokens,omitempty"`
}

func (t *SpawnAgentTool) getCtrl() AgentController {
	if t.ctrl != nil {
		return t.ctrl
	}
	return GetDefaultAgentController()
}

func (t *SpawnAgentTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a spawnAgentArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	mode := a.ContextMode
	if mode == "" {
		mode = "none"
	}

	agentID, canonPath, err := t.getCtrl().Spawn(ctx, a.ParentID, a.Name, a.Prompt, mode, a.Worktree, a.BudgetTokens)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed to spawn subagent %s: %v", a.Name, err), IsError: true}, nil
	}

	out := fmt.Sprintf("Subagent spawned successfully:\n- Agent ID: %s\n- Canonical Path: %s\n- Context Mode: %s\n- Worktree Isolation: %v",
		agentID, canonPath, mode, a.Worktree)
	return ToolResult{Output: out}, nil
}

// SendInputTool sends messages to an active subagent.
type SendInputTool struct {
	Base
	ctrl AgentController
}

func NewSendInputTool(ctrl AgentController) *SendInputTool {
	return &SendInputTool{
		Base: Base{
			SchemaStr: `{"required":["agent_id","message"],"fields":{"agent_id":"string","message":"string"}}`,
		},
		ctrl: ctrl,
	}
}

func (t *SendInputTool) Name() string        { return "send_input" }
func (t *SendInputTool) Description() string { return "Send a message or follow-up instruction to a running subagent" }

type sendInputArgs struct {
	AgentID string `json:"agent_id"`
	Message string `json:"message"`
}

func (t *SendInputTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a sendInputArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	ctrl := t.ctrl
	if ctrl == nil {
		ctrl = GetDefaultAgentController()
	}

	res, err := ctrl.SendInput(ctx, a.AgentID, a.Message)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed sending input to %s: %v", a.AgentID, err), IsError: true}, nil
	}
	return ToolResult{Output: res}, nil
}

// WaitAgentTool waits for subagent completion and retrieves its summary.
type WaitAgentTool struct {
	Base
	ctrl AgentController
}

func NewWaitAgentTool(ctrl AgentController) *WaitAgentTool {
	return &WaitAgentTool{
		Base: Base{
			SchemaStr: `{"required":["agent_id"],"fields":{"agent_id":"string","timeout_seconds":"number"}}`,
		},
		ctrl: ctrl,
	}
}

func (t *WaitAgentTool) Name() string        { return "wait_agent" }
func (t *WaitAgentTool) Description() string { return "Wait for a subagent turn to finish and retrieve its condensed summary" }

type waitAgentArgs struct {
	AgentID        string `json:"agent_id"`
	TimeoutSeconds int    `json:"timeout_seconds,omitempty"`
}

func (t *WaitAgentTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a waitAgentArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	timeout := 30 * time.Second
	if a.TimeoutSeconds > 0 {
		timeout = time.Duration(a.TimeoutSeconds) * time.Second
	}

	ctrl := t.ctrl
	if ctrl == nil {
		ctrl = GetDefaultAgentController()
	}

	status, output, tokens, err := ctrl.Wait(ctx, a.AgentID, timeout)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("wait error for %s: %v", a.AgentID, err), IsError: true}, nil
	}

	res := fmt.Sprintf("Subagent %s status: %s (tokens: %d)\n\nSummary:\n%s", a.AgentID, status, tokens, output)
	return ToolResult{Output: res}, nil
}

// CloseAgentTool terminates a subagent and cleans up isolated worktree resources.
type CloseAgentTool struct {
	Base
	ctrl AgentController
}

func NewCloseAgentTool(ctrl AgentController) *CloseAgentTool {
	return &CloseAgentTool{
		Base: Base{
			SchemaStr: `{"required":["agent_id"],"fields":{"agent_id":"string"}}`,
		},
		ctrl: ctrl,
	}
}

func (t *CloseAgentTool) Name() string        { return "close_agent" }
func (t *CloseAgentTool) Description() string { return "Terminate an active subagent and clean up ephemeral worktree storage" }

type closeAgentArgs struct {
	AgentID string `json:"agent_id"`
}

func (t *CloseAgentTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a closeAgentArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	ctrl := t.ctrl
	if ctrl == nil {
		ctrl = GetDefaultAgentController()
	}

	if err := ctrl.Close(ctx, a.AgentID); err != nil {
		return ToolResult{Output: fmt.Sprintf("failed closing %s: %v", a.AgentID, err), IsError: true}, nil
	}
	return ToolResult{Output: fmt.Sprintf("Subagent %s closed and cleaned up", a.AgentID)}, nil
}

// ResumeAgentTool restarts or unpauses a subagent.
type ResumeAgentTool struct {
	Base
	ctrl AgentController
}

func NewResumeAgentTool(ctrl AgentController) *ResumeAgentTool {
	return &ResumeAgentTool{
		Base: Base{
			SchemaStr: `{"required":["agent_id"],"fields":{"agent_id":"string"}}`,
		},
		ctrl: ctrl,
	}
}

func (t *ResumeAgentTool) Name() string        { return "resume_agent" }
func (t *ResumeAgentTool) Description() string { return "Resume execution of a paused or waiting subagent" }

type resumeAgentArgs struct {
	AgentID string `json:"agent_id"`
}

func (t *ResumeAgentTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a resumeAgentArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	ctrl := t.ctrl
	if ctrl == nil {
		ctrl = GetDefaultAgentController()
	}

	if err := ctrl.Resume(ctx, a.AgentID); err != nil {
		return ToolResult{Output: fmt.Sprintf("failed resuming %s: %v", a.AgentID, err), IsError: true}, nil
	}
	return ToolResult{Output: fmt.Sprintf("Subagent %s resumed", a.AgentID)}, nil
}
