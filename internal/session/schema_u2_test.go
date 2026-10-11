package session

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// U2: sessions and JSON event output are schema-versioned; readers
// tolerate unknown fields (forward-compatible decode).
func TestSessionSchemaVersioned(t *testing.T) {
	if protocol.EventSchemaVersion != 1 {
		t.Fatalf("[U2] EventSchemaVersion = %d, want 1", protocol.EventSchemaVersion)
	}
	s, err := Open(filepath.Join(t.TempDir(), "s.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	if v := s.SchemaVersion(); v != "1" {
		t.Fatalf("[U2] store schema_version = %q, want 1", v)
	}
	// Forward compat: a future event with unknown fields still
	// decodes (breaking changes bump the version instead).
	raw := `{"type":"turn_completed","timestamp":"2026-10-11T00:00:00Z","future_field":{"nested":true},"another":42}`
	var e protocol.EngineEvent
	if err := json.Unmarshal([]byte(raw), &e); err != nil {
		t.Fatalf("[U2] forward decode failed: %v", err)
	}
	if e.Type != protocol.EventTurnCompleted {
		t.Fatalf("[U2] decoded type = %q", e.Type)
	}
	t.Logf("[U2] schema version 1 stamped + reported; unknown fields tolerated")
}
