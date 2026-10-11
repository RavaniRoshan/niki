package engine

import (
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// X3 (trajectory): the in-memory session never retains credential
// text either.
func TestSessionRecordRedactsSecrets(t *testing.T) {
	s := NewSession()
	s.Record(protocol.EngineEvent{Type: protocol.EventToolCompleted, Timestamp: time.Now(), Text: "used sk-plantedsecretKEY1234567890 ok"})
	if len(s.Events) != 1 {
		t.Fatalf("events = %d", len(s.Events))
	}
	if strings.Contains(s.Events[0].Text, "sk-plantedsecret") || !strings.Contains(s.Events[0].Text, "[REDACTED]") {
		t.Fatalf("[X3] trajectory event not scrubbed: %q", s.Events[0].Text)
	}
	t.Logf("[X3] engine trajectory scrubbed")
}
