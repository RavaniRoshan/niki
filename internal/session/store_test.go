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
