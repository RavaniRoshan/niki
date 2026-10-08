package session

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestStoreRoundTrip(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(filepath.Join(dir, "sessions.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	id := protocol.NewSessionId()
	if err := s.CreateSession(id, "test"); err != nil {
		t.Fatal(err)
	}
	evt := protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: time.Now()}
	if err := s.AppendEvent(id, evt); err != nil {
		t.Fatal(err)
	}
	ids, _ := s.ListSessions()
	if len(ids) != 1 || ids[0] != string(id) {
		t.Fatalf("ids=%v", ids)
	}
	evts, _ := s.Events(id)
	if len(evts) != 1 || evts[0].Type != protocol.EventTurnCompleted {
		t.Fatalf("evts=%v", evts)
	}
}

func TestReopenPreservesEvents(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sessions.db")
	s, _ := Open(path)
	id := protocol.NewSessionId()
	s.CreateSession(id, "x")
	s.AppendEvent(id, protocol.EngineEvent{Type: protocol.EventSessionStarted, Timestamp: time.Now()})
	s.Close()
	// Simulate a crash: reopen without explicit close of prior (already closed),
	// then write more and read everything back.
	s2, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer s2.Close()
	evs, _ := s2.Events(id)
	if len(evs) != 1 {
		t.Fatalf("evts=%d", len(evs))
	}
}

// TestForkSession (P1): forking copies the source
// session's events into a new session id and leaves
// the source untouched.
func TestForkSession(t *testing.T) {
	dir := t.TempDir()
	s, err := Open(filepath.Join(dir, "sessions.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()

	src := protocol.NewSessionId()
	if err := s.CreateSession(src, "original"); err != nil {
		t.Fatal(err)
	}
	for _, et := range []protocol.EventType{protocol.EventSessionStarted, protocol.EventTurnStarted, protocol.EventTurnCompleted} {
		if err := s.AppendEvent(src, protocol.EngineEvent{Type: et, Timestamp: time.Now()}); err != nil {
			t.Fatal(err)
		}
	}

	forked, err := s.Fork(src, "forked copy")
	if err != nil {
		t.Fatal(err)
	}
	if forked == src {
		t.Fatal("fork must get a fresh session id")
	}

	srcEvents, _ := s.Events(src)
	forkEvents, _ := s.Events(forked)
	if len(forkEvents) != len(srcEvents) {
		t.Fatalf("fork has %d events, source has %d", len(forkEvents), len(srcEvents))
	}
	for i := range srcEvents {
		if srcEvents[i].Type != forkEvents[i].Type {
			t.Fatalf("fork event %d mismatch: %v vs %v", i, srcEvents[i].Type, forkEvents[i].Type)
		}
	}

	// The fork is independent: appending to it does not
	// touch the source.
	if err := s.AppendEvent(forked, protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now()}); err != nil {
		t.Fatal(err)
	}
	srcEvents, _ = s.Events(src)
	if len(srcEvents) != 3 {
		t.Fatalf("source session mutated by fork append: %d events", len(srcEvents))
	}

	// Both sessions are listed.
	ids, _ := s.ListSessions()
	if len(ids) != 2 {
		t.Fatalf("expected 2 listed sessions, got %v", ids)
	}
}

// TestDeleteSession removes a session and its events.
func TestDeleteSession(t *testing.T) {
	dir := t.TempDir()
	s, _ := Open(filepath.Join(dir, "sessions.db"))
	defer s.Close()
	id := protocol.NewSessionId()
	s.CreateSession(id, "doomed")
	s.AppendEvent(id, protocol.EngineEvent{Type: protocol.EventSessionStarted, Timestamp: time.Now()})
	if err := s.Delete(id); err != nil {
		t.Fatal(err)
	}
	evs, _ := s.Events(id)
	if len(evs) != 0 {
		t.Fatalf("events survived delete: %d", len(evs))
	}
	ids, _ := s.ListSessions()
	if len(ids) != 0 {
		t.Fatalf("session survived delete: %v", ids)
	}
}
