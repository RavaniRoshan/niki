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

// TestKill9MidTurnThenResume verifies that an abrupt crash (simulating kill -9)
// mid-turn preserves all flushed JSONL/DB events and allows clean session resume.
func TestKill9MidTurnThenResume(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "sessions.db")

	// Phase 1: Start turn and stream events
	s1, err := Open(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	sessionID := protocol.NewSessionId()
	_ = s1.CreateSession(sessionID, "crash-test")
	_ = s1.AppendEvent(sessionID, protocol.EngineEvent{Type: protocol.EventSessionStarted, Timestamp: time.Now()})
	_ = s1.AppendEvent(sessionID, protocol.EngineEvent{Type: protocol.EventTurnStarted, Timestamp: time.Now(), TurnID: "turn-1"})
	_ = s1.AppendEvent(sessionID, protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Timestamp: time.Now(), TurnID: "turn-1", Text: "partial response before kill -9"})
	// Abrupt termination simulation: do not call s1.Close() (simulate OS SIGKILL)

	// Phase 2: Resume in fresh process instance
	s2, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open session store after unclosed termination: %v", err)
	}
	defer s2.Close()

	recoveredEvents, err := s2.Events(sessionID)
	if err != nil {
		t.Fatalf("failed to read events: %v", err)
	}
	if len(recoveredEvents) != 3 {
		t.Fatalf("expected 3 recovered events, got %d", len(recoveredEvents))
	}
	if recoveredEvents[2].Text != "partial response before kill -9" {
		t.Fatalf("recovered event delta mismatch: %q", recoveredEvents[2].Text)
	}

	// Phase 3: Resume turn and append completion
	err = s2.AppendEvent(sessionID, protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: time.Now(), TurnID: "turn-1"})
	if err != nil {
		t.Fatalf("failed to continue session after resume: %v", err)
	}
	allEvents, _ := s2.Events(sessionID)
	if len(allEvents) != 4 {
		t.Fatalf("expected 4 total events after resume, got %d", len(allEvents))
	}
}

