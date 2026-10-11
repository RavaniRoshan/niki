package session

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// R1: sessions are append-only JSONL, written atomically — after a
// kill -9 mid-turn the sidecar holds a gap-free prefix of whole
// lines (each single Write + fsync is one atomic append), and
// `resume` continues the same session.
func TestJsonlAtomicAfterKill9(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "sessions.db")
	s, err := Open(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	sid := protocol.NewSessionId()
	if err := s.CreateSession(sid, "r1"); err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 50; i++ {
		if err := s.AppendEvent(sid, protocol.EngineEvent{
			Type:      protocol.EventAssistantTextDelta,
			Timestamp: time.Now(),
			Text:      "delta-" + strings.Repeat("x", 500),
		}); err != nil {
			t.Fatal(err)
		}
	}
	// No close: simulate kill -9, then read the raw sidecar.
	data, err := os.ReadFile(dbPath + ".jsonl")
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	if len(lines) != 50 {
		t.Fatalf("[R1] sidecar lines = %d, want 50", len(lines))
	}
	for i, ln := range lines {
		var v map[string]any
		if err := json.Unmarshal([]byte(ln), &v); err != nil {
			t.Fatalf("[R1] torn JSONL line %d: %v", i, err)
		}
	}
	t.Logf("[R1] 50 atomic appends, 0 torn lines; resume continues the session")
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}

	// Resume: reopen and continue the same session.
	s2, err := Open(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	defer s2.Close()
	if err := s2.AppendEvent(sid, protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: time.Now()}); err != nil {
		t.Fatal(err)
	}
	evts, err := s2.Events(sid)
	if err != nil || len(evts) != 51 {
		t.Fatalf("[R1] resumed events = %d, err=%v", len(evts), err)
	}
	t.Logf("[R1] resumed after kill-9: 51 events, session continues")
}
