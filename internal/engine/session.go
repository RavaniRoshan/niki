package engine

import (
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Session tracks events of a single run. Record and Snapshot are
// safe for concurrent use: the command loop records while a turn
// reads history for a new checkpoint (E3).
type Session struct {
	ID    protocol.SessionId
	Start time.Time

	mu     sync.Mutex
	Events []protocol.EngineEvent
}

func NewSession() *Session {
	return &Session{ID: protocol.NewSessionId(), Start: time.Now()}
}

func (s *Session) Record(e protocol.EngineEvent) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.Events = append(s.Events, e)
}

// Snapshot returns a copy of the recorded events for checkpoint
// history without holding locks during later use.
func (s *Session) Snapshot() []protocol.EngineEvent {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]protocol.EngineEvent, len(s.Events))
	copy(out, s.Events)
	return out
}
