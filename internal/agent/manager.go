package agent

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

var (
	ErrDepthLimit       = errors.New("subagent depth limit reached (max depth exceeded)")
	ErrConcurrencyLimit = errors.New("subagent concurrency limit reached (max 6 active agents)")
	ErrBudgetExceeded   = errors.New("subagent token budget exceeded")
	ErrAgentNotFound    = errors.New("subagent not found")
	ErrDelegationDenied = errors.New("agent delegation not permitted by allowlist")
)

const (
	DefaultMaxDepth      = 3
	DefaultMaxConcurrent = 6
)

type AgentInstance struct {
	mu          sync.RWMutex
	Node        *AgentNode
	Prompt      string
	ContextMode string
	History     []string
	Output      string
	Tokens      int
	Done        chan struct{}
	Cancel      context.CancelFunc
}

type Manager struct {
	mu             sync.RWMutex
	store          AgentGraphStore
	maxDepth       int
	sem            chan struct{}
	counter        uint64
	allowlist      map[string]bool
	instances      map[string]*AgentInstance
	eventEmitter   func(protocol.EngineEvent)
	secondaryModel string
}

func NewManager(store AgentGraphStore, maxDepth, maxConcurrent int, emitter func(protocol.EngineEvent)) *Manager {
	if store == nil {
		store = NewMemoryGraphStore()
	}
	if maxDepth <= 0 {
		maxDepth = DefaultMaxDepth
	}
	if maxConcurrent <= 0 {
		maxConcurrent = DefaultMaxConcurrent
	}
	return &Manager{
		store:        store,
		maxDepth:     maxDepth,
		sem:          make(chan struct{}, maxConcurrent),
		instances:    make(map[string]*AgentInstance),
		allowlist:    make(map[string]bool),
		eventEmitter: emitter,
	}
}

func (m *Manager) SetSecondaryModel(model string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.secondaryModel = model
}

func (m *Manager) SecondaryModel() string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.secondaryModel
}

func (m *Manager) SetAllowlist(names []string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.allowlist = make(map[string]bool)
	for _, n := range names {
		m.allowlist[strings.ToLower(strings.TrimSpace(n))] = true
	}
}

func (m *Manager) Spawn(ctx context.Context, parentID, name, prompt, contextMode string, worktree bool, budget int) (string, string, error) {
	m.mu.RLock()
	if len(m.allowlist) > 0 && !m.allowlist[strings.ToLower(strings.TrimSpace(name))] {
		m.mu.RUnlock()
		return "", "", ErrDelegationDenied
	}
	m.mu.RUnlock()

	depth := 1
	parentPath := "/root"
	if parentID != "" && parentID != "root" {
		parentNode, ok := m.store.GetNode(parentID)
		if !ok {
			return "", "", fmt.Errorf("parent agent %s not found", parentID)
		}
		depth = parentNode.Depth + 1
		parentPath = parentNode.CanonicalPath
	}

	if depth > m.maxDepth {
		return "", "", ErrDepthLimit
	}

	// Acquire concurrency semaphore slot
	select {
	case m.sem <- struct{}{}:
	default:
		return "", "", ErrConcurrencyLimit
	}

	agentID := fmt.Sprintf("agent-%d", atomic.AddUint64(&m.counter, 1))
	canonPath := ComputeCanonicalPath(parentPath, name)

	var worktreeDir string
	if worktree {
		tmpDir, err := os.MkdirTemp("", "nikicode-worktree-*")
		if err == nil {
			worktreeDir = tmpDir
			// Try git worktree add if inside a git repo
			cmd := exec.CommandContext(ctx, "git", "worktree", "add", "-d", worktreeDir, "HEAD")
			_ = cmd.Run()
		}
	}

	node := &AgentNode{
		ID:            agentID,
		CanonicalPath: canonPath,
		ParentID:      parentID,
		Name:          name,
		Model:         m.SecondaryModel(),
		Status:        StatusActive,
		Depth:         depth,
		BudgetTokens:  budget,
		WorktreeDir:   worktreeDir,
	}

	if err := m.store.AddNode(node); err != nil {
		<-m.sem
		return "", "", err
	}

	subCtx, cancel := context.WithCancel(context.Background())
	inst := &AgentInstance{
		Node:        node,
		Prompt:      prompt,
		ContextMode: contextMode,
		History:     []string{prompt},
		Done:        make(chan struct{}),
		Cancel:      cancel,
	}

	m.mu.Lock()
	m.instances[agentID] = inst
	m.mu.Unlock()

	if m.eventEmitter != nil {
		m.eventEmitter(protocol.EngineEvent{
			Type:      protocol.EventSubagentStarted,
			Timestamp: time.Now(),
			Text:      fmt.Sprintf("%s (%s)", agentID, canonPath),
		})
	}

	// Run agent in background goroutine
	go func() {
		defer func() {
			close(inst.Done)
			<-m.sem
		}()

		// Simulate execution turn or perform child task
		tokens := len(prompt) / 4
		if tokens < 10 {
			tokens = 10
		}
		if budget > 0 && tokens > budget {
			_ = m.store.UpdateStatus(agentID, StatusFailed, tokens)
			inst.mu.Lock()
			inst.Output = fmt.Sprintf("Error: %v", ErrBudgetExceeded)
			inst.Tokens = tokens
			inst.mu.Unlock()
			return
		}

		select {
		case <-subCtx.Done():
			_ = m.store.UpdateStatus(agentID, StatusClosed, tokens)
			inst.mu.Lock()
			inst.Output = "Cancelled"
			inst.Tokens = tokens
			inst.mu.Unlock()
			return
		case <-time.After(20 * time.Millisecond):
			// Simulated completed run
			inst.mu.Lock()
			inst.Output = fmt.Sprintf("Acknowledged and completed subagent task for %q (canonical: %s)", prompt, canonPath)
			inst.Tokens = tokens
			inst.mu.Unlock()
			_ = m.store.UpdateStatus(agentID, StatusCompleted, tokens)
		}

		if m.eventEmitter != nil {
			m.eventEmitter(protocol.EngineEvent{
				Type:      protocol.EventSubagentCompleted,
				Timestamp: time.Now(),
				Text:      fmt.Sprintf("%s (%s)", agentID, canonPath),
			})
		}
	}()

	return agentID, canonPath, nil
}

func (m *Manager) SendInput(ctx context.Context, agentID, message string) (string, error) {
	m.mu.RLock()
	inst, ok := m.instances[agentID]
	m.mu.RUnlock()
	if !ok {
		return "", ErrAgentNotFound
	}

	inst.mu.Lock()
	inst.History = append(inst.History, message)
	inst.Output = fmt.Sprintf("Received input: %s. Continuing execution.", message)
	inst.mu.Unlock()

	tokens := len(message) / 4
	_ = m.store.UpdateStatus(agentID, StatusActive, tokens)
	return fmt.Sprintf("Message delivered to agent %s (%s)", agentID, inst.Node.CanonicalPath), nil
}

func (m *Manager) Wait(ctx context.Context, agentID string, timeout time.Duration) (string, string, int, error) {
	m.mu.RLock()
	inst, ok := m.instances[agentID]
	m.mu.RUnlock()
	if !ok {
		return "", "", 0, ErrAgentNotFound
	}

	if timeout <= 0 {
		timeout = 30 * time.Second
	}

	select {
	case <-inst.Done:
		node, _ := m.store.GetNode(agentID)
		inst.mu.RLock()
		out := inst.Output
		toks := inst.Tokens
		inst.mu.RUnlock()
		return string(node.Status), out, toks, nil
	case <-time.After(timeout):
		inst.mu.RLock()
		toks := inst.Tokens
		inst.mu.RUnlock()
		return string(StatusActive), "Still running (timeout reached)", toks, nil
	case <-ctx.Done():
		inst.mu.RLock()
		toks := inst.Tokens
		inst.mu.RUnlock()
		return string(StatusClosed), "Context cancelled", toks, ctx.Err()
	}
}

func (m *Manager) Close(ctx context.Context, agentID string) error {
	m.mu.Lock()
	inst, ok := m.instances[agentID]
	m.mu.Unlock()
	if !ok {
		return ErrAgentNotFound
	}

	if inst.Cancel != nil {
		inst.Cancel()
	}

	// Clean up worktree directory if one was provisioned
	if inst.Node.WorktreeDir != "" {
		_ = os.RemoveAll(inst.Node.WorktreeDir)
	}

	return m.store.CloseNode(agentID)
}

func (m *Manager) Resume(ctx context.Context, agentID string) error {
	_, ok := m.store.GetNode(agentID)
	if !ok {
		return ErrAgentNotFound
	}
	return m.store.UpdateStatus(agentID, StatusActive, 0)
}

func (m *Manager) List() []*AgentNode {
	return m.store.ListNodes()
}
