package checkpoint

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/paths"
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
	GitTreeSHA   string                  `json:"git_tree_sha,omitempty"`
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
	mu                sync.RWMutex
	baseDir           string
	checkpoints       map[string]*Checkpoint
	order             []string
	preRewindSnapshot *Checkpoint
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

	// Capture pre-rewind state so user can unrevert/redo
	preSnap := &Checkpoint{
		TurnID:    "pre_rewind",
		Timestamp: time.Now(),
		Files:     make(map[string]FileSnapshot),
	}
	for path := range cp.Files {
		if data, err := os.ReadFile(path); err == nil {
			preSnap.Files[path] = FileSnapshot{
				Path:    path,
				SHA256:  hex.EncodeToString(sha256Sum(data)),
				Content: data,
			}
		}
	}
	m.mu.Lock()
	m.preRewindSnapshot = preSnap
	m.mu.Unlock()

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

// CanUnrevert reports whether an unrevert snapshot is available.
func (m *Manager) CanUnrevert() bool {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.preRewindSnapshot != nil
}

// Unrevert restores the workspace files captured immediately prior to the latest rewind.
func (m *Manager) Unrevert() (*RewindResult, error) {
	m.mu.Lock()
	snap := m.preRewindSnapshot
	m.mu.Unlock()
	if snap == nil {
		return nil, fmt.Errorf("no rewind state available to unrevert")
	}

	res := &RewindResult{
		TurnID: "unrevert",
	}
	for path, f := range snap.Files {
		if err := os.WriteFile(path, f.Content, 0o644); err != nil {
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

// CreateGitTreeSnapshot uses git write-tree to capture the entire workspace
// state as a Git tree SHA in ~2ms using an isolated index file.
func (m *Manager) CreateGitTreeSnapshot(turnID string, repoDir string) (string, error) {
	if repoDir == "" {
		repoDir = "."
	}
	cacheDir := filepath.Join(m.baseDir, "cache")
	_ = os.MkdirAll(cacheDir, 0o700)
	indexFile := filepath.Join(cacheDir, fmt.Sprintf("index_snapshot_%s", turnID))
	defer os.Remove(indexFile)

	// git add -A with isolated GIT_INDEX_FILE
	addCmd := exec.Command("git", "add", "-A")
	addCmd.Dir = repoDir
	addCmd.Env = append(os.Environ(), "GIT_INDEX_FILE="+indexFile)
	if err := addCmd.Run(); err != nil {
		return "", fmt.Errorf("git add failed: %w", err)
	}

	// git write-tree
	writeCmd := exec.Command("git", "write-tree")
	writeCmd.Dir = repoDir
	writeCmd.Env = append(os.Environ(), "GIT_INDEX_FILE="+indexFile)
	out, err := writeCmd.Output()
	if err != nil {
		return "", fmt.Errorf("git write-tree failed: %w", err)
	}

	treeSHA := strings.TrimSpace(string(out))
	m.mu.Lock()
	if cp, ok := m.checkpoints[turnID]; ok {
		cp.GitTreeSHA = treeSHA
	}
	m.mu.Unlock()
	return treeSHA, nil
}

// RewindGitTree restores the workspace from a Git tree SHA.
func (m *Manager) RewindGitTree(repoDir, treeSHA string) error {
	if repoDir == "" {
		repoDir = "."
	}
	if treeSHA == "" {
		return fmt.Errorf("empty tree sha")
	}

	// git read-tree <sha>
	readCmd := exec.Command("git", "read-tree", treeSHA)
	readCmd.Dir = repoDir
	if err := readCmd.Run(); err != nil {
		return fmt.Errorf("git read-tree failed: %w", err)
	}

	// git checkout-index -a -f
	checkoutCmd := exec.Command("git", "checkout-index", "-a", "-f")
	checkoutCmd.Dir = repoDir
	if err := checkoutCmd.Run(); err != nil {
		return fmt.Errorf("git checkout-index failed: %w", err)
	}
	return nil
}

func sha256Sum(b []byte) []byte {
	h := sha256.Sum256(b)
	return h[:]
}

