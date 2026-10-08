package checkpoint

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"

	"github.com/RavaniRoshan/niki/internal/paths"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// FileSnapshot records a single file's content and SHA-256 hash at checkpoint time.
type FileSnapshot struct {
	Path    string `json:"path"`
	SHA256  string `json:"sha256"`
	Content []byte `json:"content"`
}

// Checkpoint records file states and conversation history keyed by turn ID.
type Checkpoint struct {
	TurnID       string                  `json:"turn_id"`
	TurnNumber   int                     `json:"turn_number"`
	Timestamp    time.Time               `json:"timestamp"`
	Files        map[string]FileSnapshot `json:"files"`
	Conversation []protocol.EngineEvent  `json:"conversation,omitempty"`
}

type RewindTarget string

const (
	RewindCode         RewindTarget = "code"
	RewindConversation RewindTarget = "conversation"
	RewindBoth         RewindTarget = "both"
)

type RewindResult struct {
	TurnID   string   `json:"turn_id"`
	Restored []string `json:"restored_files"`
	Skipped  []string `json:"skipped_files"`
	Events   int      `json:"restored_events"`
}

type Manager struct {
	mu          sync.RWMutex
	baseDir     string
	checkpoints map[string]*Checkpoint
	order       []string
}

func NewManager(baseDir string) *Manager {
	if baseDir == "" {
		if paths.Home() == "" {
			baseDir = filepath.Join(os.TempDir(), "nikicode-checkpoints")
		} else {
			baseDir = filepath.Join(paths.Dir(), "checkpoints")
		}
	}
	_ = os.MkdirAll(baseDir, 0o700)
	return &Manager{
		baseDir:     baseDir,
		checkpoints: make(map[string]*Checkpoint),
	}
}

// CreateCheckpoint captures files before edits and attaches conversation events.
func (m *Manager) CreateCheckpoint(turnID string, filePaths []string, events []protocol.EngineEvent) (*Checkpoint, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	cp := &Checkpoint{
		TurnID:       turnID,
		TurnNumber:   len(m.order) + 1,
		Timestamp:    time.Now(),
		Files:        make(map[string]FileSnapshot),
		Conversation: events,
	}

	for _, p := range filePaths {
		data, err := os.ReadFile(p)
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return nil, fmt.Errorf("failed reading file %s: %w", p, err)
		}
		h := sha256.Sum256(data)
		hashStr := hex.EncodeToString(h[:])
		cp.Files[p] = FileSnapshot{
			Path:    p,
			SHA256:  hashStr,
			Content: data,
		}
	}

	m.checkpoints[turnID] = cp
	m.order = append(m.order, turnID)
	return cp, nil
}

// GetCheckpoint retrieves a checkpoint by turn ID.
func (m *Manager) GetCheckpoint(turnID string) (*Checkpoint, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	cp, ok := m.checkpoints[turnID]
	return cp, ok
}

// ListCheckpoints returns all checkpoints in chronological order.
func (m *Manager) ListCheckpoints() []*Checkpoint {
	m.mu.RLock()
	defer m.mu.RUnlock()
	list := make([]*Checkpoint, 0, len(m.order))
	for _, id := range m.order {
		if cp, ok := m.checkpoints[id]; ok {
			list = append(list, cp)
		}
	}
	return list
}

// RewindCode restores snapshot files, verifying hashes to prevent clobbering external changes.
func (m *Manager) RewindCode(turnID string, force bool) (*RewindResult, error) {
	m.mu.RLock()
	cp, ok := m.checkpoints[turnID]
	m.mu.RUnlock()
	if !ok {
		return nil, fmt.Errorf("checkpoint %s not found", turnID)
	}

	res := &RewindResult{
		TurnID: turnID,
	}

	for path, snap := range cp.Files {
		currentData, err := os.ReadFile(path)
		if err == nil {
			currentHash := hex.EncodeToString(sha256Sum(currentData))
			// If file was modified externally after snapshot and force is false,
			// prevent accidental collision/clobbering!
			if currentHash != snap.SHA256 && !force {
				res.Skipped = append(res.Skipped, fmt.Sprintf("%s (hash mismatch %s != %s)", path, currentHash[:8], snap.SHA256[:8]))
				continue
			}
		}

		if err := os.WriteFile(path, snap.Content, 0o644); err != nil {
			return nil, fmt.Errorf("failed restoring file %s: %w", path, err)
		}
		res.Restored = append(res.Restored, path)
	}

	return res, nil
}

// RewindConversation returns the event history captured at the checkpoint.
func (m *Manager) RewindConversation(turnID string) ([]protocol.EngineEvent, error) {
	m.mu.RLock()
	cp, ok := m.checkpoints[turnID]
	m.mu.RUnlock()
	if !ok {
		return nil, fmt.Errorf("checkpoint %s not found", turnID)
	}
	return cp.Conversation, nil
}

// RewindAll restores both code files and conversation.
func (m *Manager) RewindAll(turnID string, force bool) (*RewindResult, []protocol.EngineEvent, error) {
	codeRes, err := m.RewindCode(turnID, force)
	if err != nil {
		return nil, nil, err
	}
	events, err := m.RewindConversation(turnID)
	if err != nil {
		return nil, nil, err
	}
	codeRes.Events = len(events)
	return codeRes, events, nil
}

func sha256Sum(b []byte) []byte {
	h := sha256.Sum256(b)
	return h[:]
}
