// Package undo keeps an append-only stack of the agent's file
// changes so the last change can be reverted without touching the
// user's own edits: every restore verifies the file still looks
// exactly as the change left it and refuses otherwise (E4).
package undo

import (
	"bytes"
	"fmt"
	"os"
	"sync"
	"time"
)

// Effect is one file change made by an agent tool.
type Effect struct {
	Path    string
	TurnID  string
	Tool    string
	Time    time.Time
	Existed bool        // file existed before the change
	Before  []byte      // pre-image (nil when !Existed)
	After   []byte      // post-image as the tool left it
	Mode    os.FileMode // pre-change permission bits
}

// Stack records effects newest-last.
type Stack struct {
	mu    sync.Mutex
	items []Effect
	seq   uint64
	ids   []uint64
}

func NewStack() *Stack { return &Stack{} }

func (s *Stack) Push(e Effect) {
	if e.Time.IsZero() {
		e.Time = time.Now()
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.seq++
	s.items = append(s.items, e)
	s.ids = append(s.ids, s.seq)
}

func (s *Stack) Len() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.items)
}

// UndoLast reverts the newest effect: the file must still match the
// post-image (else the user's edits are kept and an error is
// returned with the effect still on the stack). Restores the
// pre-image bytes and permission bits; removes files the change
// created.
func (s *Stack) UndoLast() (Effect, error) {
	s.mu.Lock()
	if len(s.items) == 0 {
		s.mu.Unlock()
		return Effect{}, fmt.Errorf("nothing to undo")
	}
	idx := len(s.items) - 1
	id := s.ids[idx]
	e := s.items[idx]
	s.mu.Unlock()

	if err := Restore(e); err != nil {
		return e, err
	}

	s.mu.Lock()
	// Pop only if it is still the same effect (a concurrent push
	// appends, so the id check keeps the stack consistent).
	if n := len(s.items); n > 0 && s.ids[n-1] == id {
		s.items = s.items[:n-1]
		s.ids = s.ids[:n-1]
	}
	s.mu.Unlock()
	return e, nil
}

// Restore verifies the file still equals the post-image, then writes
// the pre-image back (or removes a created file) with the original
// mode.
func Restore(e Effect) error {
	current, err := os.ReadFile(e.Path)
	if err != nil {
		if os.IsNotExist(err) && !e.Existed {
			return nil // already gone
		}
		return fmt.Errorf("cannot undo %s: %v", e.Path, err)
	}
	if !bytes.Equal(current, e.After) {
		return fmt.Errorf("cannot undo %s: file changed since the agent edit (your changes are kept)", e.Path)
	}
	if !e.Existed {
		if err := os.Remove(e.Path); err != nil {
			return fmt.Errorf("cannot undo %s: %v", e.Path, err)
		}
		return nil
	}
	mode := e.Mode.Perm()
	if mode == 0 {
		mode = 0o644
	}
	if err := os.WriteFile(e.Path, e.Before, mode); err != nil {
		return fmt.Errorf("cannot undo %s: %v", e.Path, err)
	}
	// WriteFile only applies the mode on create; enforce it on
	// overwrite too.
	if err := os.Chmod(e.Path, mode); err != nil {
		return fmt.Errorf("cannot undo %s: %v", e.Path, err)
	}
	return nil
}
