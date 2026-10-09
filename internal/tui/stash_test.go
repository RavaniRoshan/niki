package tui

import (
	"testing"
)

func TestStashManagerPushPop(t *testing.T) {
	sm := NewStashManager()
	if _, ok := sm.Pop(); ok {
		t.Fatalf("expected pop on empty stash to fail")
	}

	entry1 := sm.Push("prompt line 1\nprompt line 2")
	if entry1.LineCount != 2 {
		t.Fatalf("expected line count 2, got %d", entry1.LineCount)
	}
	if entry1.Content != "prompt line 1\nprompt line 2" {
		t.Fatalf("unexpected content: %s", entry1.Content)
	}

	_ = sm.Push("single line prompt")
	if len(sm.List()) != 2 {
		t.Fatalf("expected 2 entries in list, got %d", len(sm.List()))
	}

	popped, ok := sm.Pop()
	if !ok || popped.Content != "single line prompt" {
		t.Fatalf("expected LIFO pop of second prompt, got %v", popped)
	}

	popped1, ok := sm.Pop()
	if !ok || popped1.Content != "prompt line 1\nprompt line 2" {
		t.Fatalf("expected pop of first prompt, got %v", popped1)
	}

	if _, ok := sm.Pop(); ok {
		t.Fatalf("expected stash to be empty after popping all")
	}
}

func TestStashManagerDelete(t *testing.T) {
	sm := NewStashManager()
	e1 := sm.Push("keep this")
	e2 := sm.Push("delete this")

	if !sm.Delete(e2.ID) {
		t.Fatalf("failed to delete entry %s", e2.ID)
	}
	if len(sm.List()) != 1 {
		t.Fatalf("expected 1 entry, got %d", len(sm.List()))
	}
	if sm.List()[0].ID != e1.ID {
		t.Fatalf("expected entry 1 to remain")
	}
}
