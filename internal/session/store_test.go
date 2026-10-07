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
