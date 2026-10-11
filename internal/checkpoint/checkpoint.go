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

// FileSnapshot records a single file's content, permission bits and
// SHA-256 hash at checkpoint time. When a turn's tool later writes
// the file, After/AfterSHA256 record the state the turn left it in,
// so a rewind can tell the agent's own change from an external edit.
// Created marks a file that did not exist at checkpoint time (the
// turn created it; rewind removes it).
type FileSnapshot struct {
	Path        string      `json:"path"`
	SHA256      string      `json:"sha256"`
	Content     []byte      `json:"content"`
	Mode        os.FileMode `json:"mode,omitempty"`
	After       []byte      `json:"after,omitempty"`
	AfterSHA256 string      `json:"after_sha256,omitempty"`
	Created     bool        `json:"created,omitempty"`
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
		mode := os.FileMode(0)
		if fi, serr := os.Stat(p); serr == nil {
			mode = fi.Mode().Perm()
		}
		cp.Files[p] = FileSnapshot{
			Path:    p,
			SHA256:  hashStr,
			Content: data,
			Mode:    mode,
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

// LastTurnID returns the most recently created checkpoint's turn id,
// or "" when no checkpoint exists yet.
func (m *Manager) LastTurnID() string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	if len(m.order) == 0 {
		return ""
	}
	return m.order[len(m.order)-1]
}

// CapturePreImage records the pre-turn state of path in the turn's
// checkpoint at the FIRST touch (before a mutating tool runs), so a
// rewind can restore files the turn changed. Missing files are
// recorded as Created (rewind removes them). Capturing twice is a
// no-op: the earliest pre-image wins.
func (m *Manager) CapturePreImage(turnID, path string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	cp, ok := m.checkpoints[turnID]
	if !ok {
		return fmt.Errorf("checkpoint %s not found", turnID)
	}
	if _, dup := cp.Files[path]; dup {
		return nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			cp.Files[path] = FileSnapshot{Path: path, Created: true}
			return nil
		}
		return fmt.Errorf("failed reading file %s: %w", path, err)
	}
	mode := os.FileMode(0)
	if fi, serr := os.Stat(path); serr == nil {
		mode = fi.Mode().Perm()
	}
	cp.Files[path] = FileSnapshot{
		Path:    path,
		SHA256:  hex.EncodeToString(sha256Sum(data)),
		Content: data,
		Mode:    mode,
	}
	return nil
}

// RecordAfter records the post-write state of a captured file so
// RewindCode can distinguish the agent's own edit (current == After)
// from an external change (current == anything else). Only captured
// files are updated; unknown paths are ignored.
func (m *Manager) RecordAfter(turnID, path string, data []byte) {
	m.mu.Lock()
	defer m.mu.Unlock()
	cp, ok := m.checkpoints[turnID]
	if !ok {
		return
	}
	snap, ok := cp.Files[path]
	if !ok {
		return
	}
	snap.After = data
	snap.AfterSHA256 = hex.EncodeToString(sha256Sum(data))
	cp.Files[path] = snap
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

	// Capture pre-rewind state so user can unrevert/redo (files
	// absent at this point are marked Created so unrevert removes
	// them again; modes are preserved so unrevert restores them).
	preSnap := &Checkpoint{
		TurnID:    "pre_rewind",
		Timestamp: time.Now(),
		Files:     make(map[string]FileSnapshot),
	}
	for path := range cp.Files {
		data, err := os.ReadFile(path)
		if err != nil {
			if os.IsNotExist(err) {
				preSnap.Files[path] = FileSnapshot{Path: path, Created: true}
			}
			continue
		}
		mode := os.FileMode(0)
		if fi, serr := os.Stat(path); serr == nil {
			mode = fi.Mode().Perm()
		}
		preSnap.Files[path] = FileSnapshot{
			Path:    path,
			SHA256:  hex.EncodeToString(sha256Sum(data)),
			Content: data,
			Mode:    mode,
		}
	}
	m.mu.Lock()
	m.preRewindSnapshot = preSnap
	m.mu.Unlock()

	for path, snap := range cp.Files {
		currentData, err := os.ReadFile(path)

		if snap.Created {
			// The file did not exist at checkpoint time: rewind
			// removes it, but only if it still looks like the turn
			// left it (an externally edited leftover is kept).
			if err != nil {
				continue // already absent
			}
			curHash := hex.EncodeToString(sha256Sum(currentData))
			if snap.AfterSHA256 != "" && curHash != snap.AfterSHA256 && !force {
				res.Skipped = append(res.Skipped, fmt.Sprintf("%s (created file changed since: %s != %s)", path, curHash[:8], snap.AfterSHA256[:8]))
				continue
			}
			if rerr := os.Remove(path); rerr != nil {
				return nil, fmt.Errorf("failed removing file %s: %w", path, rerr)
			}
			res.Restored = append(res.Restored, path)
			continue
		}

		if err == nil {
			currentHash := hex.EncodeToString(sha256Sum(currentData))
			switch {
			case snap.AfterSHA256 != "":
				// Turn-touched file: current must be the agent's
				// post-state (or already the pre-state); anything
				// else is an external edit and is protected.
				if currentHash != snap.AfterSHA256 && currentHash != snap.SHA256 && !force {
					res.Skipped = append(res.Skipped, fmt.Sprintf("%s (changed since the turn: %s != %s)", path, currentHash[:8], snap.AfterSHA256[:8]))
					continue
				}
			default:
				// Snapshot-only checkpoint (no turn write recorded):
				// legacy behaviour, hash mismatch needs force.
				if currentHash != snap.SHA256 && !force {
					res.Skipped = append(res.Skipped, fmt.Sprintf("%s (hash mismatch %s != %s)", path, currentHash[:8], snap.SHA256[:8]))
					continue
				}
			}
		}

		if err := os.WriteFile(path, snap.Content, 0o644); err != nil {
			return nil, fmt.Errorf("failed restoring file %s: %w", path, err)
		}
		// Restore the exact permission bits (WriteFile only applies
		// them at create time).
		if snap.Mode != 0 {
			if cerr := os.Chmod(path, snap.Mode.Perm()); cerr != nil {
				return nil, fmt.Errorf("failed restoring mode of %s: %w", path, cerr)
			}
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
		if f.Created {
			// Absent before the rewind: put it back to absent.
			if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
				return nil, fmt.Errorf("failed removing file %s: %w", path, err)
			}
			res.Restored = append(res.Restored, path)
			continue
		}
		if err := os.WriteFile(path, f.Content, 0o644); err != nil {
			return nil, fmt.Errorf("failed restoring file %s: %w", path, err)
		}
		if f.Mode != 0 {
			if err := os.Chmod(path, f.Mode.Perm()); err != nil {
				return nil, fmt.Errorf("failed restoring mode of %s: %w", path, err)
			}
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
	// Shadow ref: persist the tree under refs/niki/… so the
	// checkpoint survives the process — without ever touching the
	// user's branches, index, or worktree (best effort: the SHA in
	// the checkpoint still stands if the ref cannot be written).
	refCmd := exec.Command("git", "update-ref", "refs/niki/checkpoints/"+turnID, treeSHA)
	refCmd.Dir = repoDir
	_ = refCmd.Run()
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
