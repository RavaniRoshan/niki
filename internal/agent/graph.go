package agent

import (
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"
)

type AgentStatus string

const (
	StatusActive    AgentStatus = "active"
	StatusWaiting   AgentStatus = "waiting"
	StatusCompleted AgentStatus = "completed"
	StatusFailed    AgentStatus = "failed"
	StatusClosed    AgentStatus = "closed"
)

// AgentNode represents a single agent in the execution hierarchy.
type AgentNode struct {
	ID            string      `json:"id"`
	CanonicalPath string      `json:"canonical_path"` // e.g. "/root/worker-1"
	ParentID      string      `json:"parent_id"`
	Name          string      `json:"name"`
	Status        AgentStatus `json:"status"`
	Depth         int         `json:"depth"`
	TokensUsed    int         `json:"tokens_used"`
	BudgetTokens  int         `json:"budget_tokens"`
	WorktreeDir   string      `json:"worktree_dir,omitempty"`
	CreatedAt     time.Time   `json:"created_at"`
	UpdatedAt     time.Time   `json:"updated_at"`
}

type AgentGraphStore interface {
	AddNode(node *AgentNode) error
	GetNode(id string) (*AgentNode, bool)
	ListNodes() []*AgentNode
	UpdateStatus(id string, status AgentStatus, tokensUsed int) error
	CloseNode(id string) error
}

type MemoryGraphStore struct {
	mu    sync.RWMutex
	nodes map[string]*AgentNode
}

func NewMemoryGraphStore() *MemoryGraphStore {
	return &MemoryGraphStore{
		nodes: make(map[string]*AgentNode),
	}
}

func (s *MemoryGraphStore) AddNode(node *AgentNode) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, exists := s.nodes[node.ID]; exists {
		return fmt.Errorf("agent %s already exists in graph", node.ID)
	}
	node.CreatedAt = time.Now()
	node.UpdatedAt = time.Now()
	s.nodes[node.ID] = node
	return nil
}

func (s *MemoryGraphStore) GetNode(id string) (*AgentNode, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	node, ok := s.nodes[id]
	if !ok {
		return nil, false
	}
	cp := *node
	return &cp, true
}

func (s *MemoryGraphStore) ListNodes() []*AgentNode {
	s.mu.RLock()
	defer s.mu.RUnlock()
	list := make([]*AgentNode, 0, len(s.nodes))
	for _, n := range s.nodes {
		cp := *n
		list = append(list, &cp)
	}
	sort.Slice(list, func(i, j int) bool {
		return list[i].CanonicalPath < list[j].CanonicalPath
	})
	return list
}

func (s *MemoryGraphStore) UpdateStatus(id string, status AgentStatus, tokensUsed int) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	node, ok := s.nodes[id]
	if !ok {
		return fmt.Errorf("agent %s not found", id)
	}
	node.Status = status
	node.TokensUsed += tokensUsed
	node.UpdatedAt = time.Now()
	return nil
}

func (s *MemoryGraphStore) CloseNode(id string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	node, ok := s.nodes[id]
	if !ok {
		return fmt.Errorf("agent %s not found", id)
	}
	node.Status = StatusClosed
	node.UpdatedAt = time.Now()
	return nil
}

// ComputeCanonicalPath calculates the hierarchy path for a node.
func ComputeCanonicalPath(parentPath, name string) string {
	cleanName := strings.TrimSpace(name)
	if cleanName == "" {
		cleanName = "agent"
	}
	if parentPath == "" || parentPath == "/" {
		return "/" + cleanName
	}
	return strings.TrimRight(parentPath, "/") + "/" + cleanName
}
