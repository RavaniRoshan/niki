package engine

import (
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Session tracks events of a single run.
type Session struct {
	ID     protocol.SessionId
	Events []protocol.EngineEvent
	Start  time.Time
}

func NewSession() *Session {
	return &Session{ID: protocol.NewSessionId(), Start: time.Now()}
}

func (s *Session) Record(e protocol.EngineEvent) {
	s.Events = append(s.Events, e)
}
