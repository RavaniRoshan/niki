package session

import (
	"path/filepath"
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// ForkAtCheckpoint preserves events up to and including the Nth
// checkpoint marker (an EventContextCompacted) into a new session;
// events after that marker stay with the source only.
func (s *Store) ForkAtCheckpoint(srcID protocol.SessionId, checkpointsSeen int, title string) (protocol.SessionId, error) {
	evts, err := s.Events(srcID)
	if err != nil {
		return "", err
	}
	marker := string(protocol.EventContextCompacted)
	markerCount := 0
	cut := len(evts)
	for i, e := range evts {
		if string(e.Type) == marker {
			markerCount++
			if markerCount == checkpointsSeen {
				// Keep everything through this marker; anything after
				// it stays with the source only.
				cut = i + 1
				break
			}
		}
	}
	keep := evts[:cut]
	newID := protocol.NewSessionId()
	if err := s.CreateSession(newID, title); err != nil {
		return "", err
	}
	for _, e := range keep {
		if err := s.AppendEvent(newID, e); err != nil {
			return "", err
		}
	}
	return newID, nil
}

func seedForkFixture(t *testing.T) (*Store, protocol.SessionId) {
	t.Helper()
	store, err := Open(filepath.Join(t.TempDir(), "fork.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = store.Close() })
	src := protocol.SessionId("src-fork-1")
	if err := store.CreateSession(src, "src"); err != nil {
		t.Fatal(err)
	}
	mk := func(typ protocol.EventType, text string) protocol.EngineEvent {
		return protocol.EngineEvent{Type: typ, Text: text}
	}
	for _, e := range []protocol.EngineEvent{
		mk(protocol.EventTurnStarted, "task one"),
		mk(protocol.EventToolStarted, "edit"),
		mk(protocol.EventContextCompacted, "summary one"),
		mk(protocol.EventTurnStarted, "task two"),
		mk(protocol.EventContextCompacted, "summary two"),
		mk(protocol.EventTurnStarted, "task three"),
	} {
		if err := store.AppendEvent(src, e); err != nil {
			t.Fatal(err)
		}
	}
	return store, src
}

// R2: fork a session at a checkpoint. The fork keeps every event up to
// and including the checkpoint marker; the source session is untouched.
func TestForkAtCheckpoint(t *testing.T) {
	store, src := seedForkFixture(t)

	newID, err := store.ForkAtCheckpoint(src, 1, "forked@1")
	if err != nil {
		t.Fatal(err)
	}
	fe, _ := store.Events(newID)
	// Events 0..3 (turn, tool, marker1) — marker 1 is the cut point.
	got := forkGolden(fe)
	want := "turn_started: task one\ntool_started: edit\ncontext_compacted: summary one"
	if got != want {
		t.Fatalf("[R2] fork golden mismatch:\n got:\n%s\nwant:\n%s", got, want)
	}
	t.Logf("[R2] fork at marker 1 keeps %d events (through the marker)", len(fe))

	// Fork past the last marker keeps everything.
	newID2, err := store.ForkAtCheckpoint(src, 9, "fork-all")
	if err != nil {
		t.Fatal(err)
	}
	fe2, _ := store.Events(newID2)
	if len(fe2) != 6 {
		t.Fatalf("[R2] fork past last marker keeps all: got %d events", len(fe2))
	}
	t.Logf("[R2] fork past last marker keeps all %d events", len(fe2))

	// The source session is untouched.
	se, _ := store.Events(src)
	if len(se) != 6 {
		t.Fatalf("[R2] source session touched: %d events", len(se))
	}
	t.Logf("[R2] source untouched (%d events)", len(se))
}
