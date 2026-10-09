package tui

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

// StashEntry represents a stashed composer draft.
type StashEntry struct {
	ID        string    `json:"id"`
	Content   string    `json:"content"`
	CreatedAt time.Time `json:"created_at"`
	LineCount int       `json:"line_count"`
}

// StashManager manages stashed prompt drafts in a LIFO stack.
type StashManager struct {
	entries []StashEntry
	mu      sync.RWMutex
}

// NewStashManager creates a new prompt stash manager.
func NewStashManager() *StashManager {
	return &StashManager{
		entries: make([]StashEntry, 0),
	}
}

// Push stashes a prompt draft and returns the entry.
func (s *StashManager) Push(content string) StashEntry {
	s.mu.Lock()
	defer s.mu.Unlock()

	trimmed := strings.TrimSpace(content)
	lines := strings.Split(content, "\n")
	entry := StashEntry{
		ID:        fmt.Sprintf("stash-%d", time.Now().UnixNano()),
		Content:   trimmed,
		CreatedAt: time.Now(),
		LineCount: len(lines),
	}
	s.entries = append(s.entries, entry)
	return entry
}

// Pop removes and returns the most recently stashed prompt.
func (s *StashManager) Pop() (StashEntry, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()

	n := len(s.entries)
	if n == 0 {
		return StashEntry{}, false
	}
	last := s.entries[n-1]
	s.entries = s.entries[:n-1]
	return last, true
}

// Peek returns the most recently stashed prompt without removing it.
func (s *StashManager) Peek() (StashEntry, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	n := len(s.entries)
	if n == 0 {
		return StashEntry{}, false
	}
	return s.entries[n-1], true
}

// List returns a copy of all stashed prompt entries.
func (s *StashManager) List() []StashEntry {
	s.mu.RLock()
	defer s.mu.RUnlock()

	out := make([]StashEntry, len(s.entries))
	copy(out, s.entries)
	return out
}

// Delete removes an entry by ID.
func (s *StashManager) Delete(id string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()

	for i, e := range s.entries {
		if e.ID == id {
			s.entries = append(s.entries[:i], s.entries[i+1:]...)
			return true
		}
	}
	return false
}

// Clear empties the stash.
func (s *StashManager) Clear() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.entries = s.entries[:0]
}
