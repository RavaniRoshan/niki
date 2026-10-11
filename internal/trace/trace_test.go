package trace

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestRecorderExportSchemaAndOrder(t *testing.T) {
	r := NewRecorder("trace-o2-1")
	now := time.Date(2026, 10, 11, 12, 0, 0, 0, time.UTC)
	// The recorder preserves arrival order, not sorted order: a trace is an
	// ordered log of what happened.
	r.Observe(protocol.EngineEvent{Type: protocol.EventTurnStarted, Timestamp: now, TurnID: "t1", Text: "refactor trace.go"})
	r.Observe(protocol.EngineEvent{Type: protocol.EventToolStarted, Timestamp: now, TurnID: "t1", CallID: "c1", ToolName: "write_file"})
	r.Observe(protocol.EngineEvent{Type: protocol.EventToolCompleted, Timestamp: now, TurnID: "t1", CallID: "c1", Text: "trace.go written"})
	r.Observe(protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: now, TurnID: "t1", Text: "done"})

	var buf bytes.Buffer
	if err := r.Export(&buf); err != nil {
		t.Fatal(err)
	}
	out := buf.String()

	// Schema version stamped at the documented value.
	if !strings.Contains(out, `"schema_version": 1`) {
		t.Fatalf("[O2] schema_version not stamped: %s", out)
	}
	if !strings.Contains(out, `"session_id": "trace-o2-1"`) {
		t.Fatalf("[O2] session_id not in document: %s", out)
	}
	if !strings.Contains(out, `"event_count": 4`) {
		t.Fatalf("[O2] event_count mismatch: %s", out)
	}

	// The document round-trips and validates.
	var doc Document
	if err := json.Unmarshal(buf.Bytes(), &doc); err != nil {
		t.Fatalf("[O2] document does not round-trip: %v", err)
	}
	if err := doc.Validate(); err != nil {
		t.Fatalf("[O2] Validate rejected a good document: %v", err)
	}
	if doc.EventCount != len(doc.Events) {
		t.Fatalf("[O2] EventCount (%d) != len(Events) (%d)", doc.EventCount, len(doc.Events))
	}
	// Arrival order preserved.
	got := []protocol.EventType{doc.Events[0].Type, doc.Events[3].Type}
	if got[0] != protocol.EventTurnStarted || got[1] != protocol.EventTurnCompleted {
		t.Fatalf("[O2] event order corrupted: got %v", got)
	}

	// Schema test: a malformed document is rejected.
	bad := Document{SchemaVersion: 0, Events: []protocol.EngineEvent{{Type: protocol.EventSessionStarted, Timestamp: now}}}
	if err := bad.Validate(); err == nil {
		t.Fatal("[O2] malformed document (zero schema) should fail Validate")
	}
	bad2 := Document{SchemaVersion: 1, EventCount: 99, Events: nil}
	if err := bad2.Validate(); err == nil {
		t.Fatal("[O2] event_count mismatch should fail Validate")
	}
	bad3 := Document{SchemaVersion: 1, Events: []protocol.EngineEvent{{}}}
	if err := bad3.Validate(); err == nil {
		t.Fatal("[O2] event with no type/timestamp should fail Validate")
	}

	t.Logf("[O2] trace document schema_version=1, event_count=4, round-trips, validates; malformed docs rejected")
}
