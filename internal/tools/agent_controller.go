package tools

import (
	"context"
	"fmt"
	"sync"
	"time"
)

type AgentController interface {
	Spawn(ctx context.Context, parentID, name, prompt, contextMode string, worktree bool, budget int) (agentID string, canonicalPath string, err error)
	SendInput(ctx context.Context, agentID, message string) (string, error)
	Wait(ctx context.Context, agentID string, timeout time.Duration) (status string, output string, tokens int, err error)
	Close(ctx context.Context, agentID string) error
	Resume(ctx context.Context, agentID string) error
}

var (
	defaultAgentControllerMu sync.RWMutex
	defaultAgentController   AgentController
)

func SetDefaultAgentController(c AgentController) {
	defaultAgentControllerMu.Lock()
	defer defaultAgentControllerMu.Unlock()
	defaultAgentController = c
}

func GetDefaultAgentController() AgentController {
	defaultAgentControllerMu.RLock()
	defer defaultAgentControllerMu.RUnlock()
	if defaultAgentController != nil {
		return defaultAgentController
	}
	// Return a stub fallback if none configured
	return &fallbackAgentController{}
}

type fallbackAgentController struct{}

func (f *fallbackAgentController) Spawn(ctx context.Context, parentID, name, prompt, contextMode string, worktree bool, budget int) (string, string, error) {
	return "agent-stub-1", "/root/" + name, nil
}
func (f *fallbackAgentController) SendInput(ctx context.Context, agentID, message string) (string, error) {
	return fmt.Sprintf("Delivered to %s", agentID), nil
}
func (f *fallbackAgentController) Wait(ctx context.Context, agentID string, timeout time.Duration) (string, string, int, error) {
	return "completed", "Task finished", 50, nil
}
func (f *fallbackAgentController) Close(ctx context.Context, agentID string) error  { return nil }
func (f *fallbackAgentController) Resume(ctx context.Context, agentID string) error { return nil }
