// Package trace exports a run's collected engine events as a schema-versioned
// JSON trace document (O2). A trace is the raw, greppable artifact for
// debugging: every event the engine emitted for a session, in order, with its
// schema version pinned so readers can validate shape without guessing.
package trace

import (
	"encoding/json"
	"fmt"
	"io"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// SchemaVersion versions the trace document format. It mirrors
// protocol.EventSchemaVersion so a trace is as forward-tolerant as the
// session log (U2): readers must tolerate unknown fields, and a structural
// break that invalidates readers bumps this number instead.
const SchemaVersion = protocol.EventSchemaVersion

// Document is the on-disk shape of a trace export. One document per
// io.Writer call; it is a stable JSON object, not a live struct.
type Document struct {
	SchemaVersion int                    `json:"schema_version"`
	GeneratedAt   time.Time              `json:"generated_at"`
	SessionID     protocol.SessionId     `json:"session_id"`
	EventCount    int                    `json:"event_count"`
	Events        []protocol.EngineEvent `json:"events"`
}

// Recorder accumulates events observed from the engine and can export them.
// Wire it with engine.Observe(r.Observe); the engine emits each event once, so
// the recorder holds an ordered, complete log.
type Recorder struct {
	sid    protocol.SessionId
	events []protocol.EngineEvent
}

// NewRecorder returns a recorder for a session. Register it with
// engine.Observe(r.Observe) before the engine starts emitting.
func NewRecorder(sessionID protocol.SessionId) *Recorder {
	return &Recorder{sid: sessionID}
}

// Observe is the engine observer callback: append events in arrival order.
func (r *Recorder) Observe(e protocol.EngineEvent) {
	r.events = append(r.events, e)
}

// Events returns the collected events (for inspection/testing). The slice is a
// view over internal state — callers must not mutate it.
func (r *Recorder) Events() []protocol.EngineEvent {
	return r.events
}

// Export writes the trace document as a single indented JSON object to w.
func (r *Recorder) Export(w io.Writer) error {
	doc := Document{
		SchemaVersion: SchemaVersion,
		GeneratedAt:   time.Now().UTC(),
		SessionID:     r.sid,
		EventCount:    len(r.events),
		Events:        r.events,
	}
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(doc)
}

// Validate checks the minimal invariants a trace reader relies on:
// the schema version is non-zero, the event count matches the slice, and every
// event carries a type and a timestamp. It returns the first problem.
func (d Document) Validate() error {
	if d.SchemaVersion <= 0 {
		return ValidationError{Field: "schema_version", Msg: "must be positive"}
	}
	if d.EventCount != len(d.Events) {
		return ValidationError{Field: "event_count", Msg: "mismatch"}
	}
	for i, e := range d.Events {
		if e.Type == "" {
			return ValidationError{Field: "events", Msg: fmt.Sprintf("event %d missing type", i)}
		}
		if e.Timestamp.IsZero() {
			return ValidationError{Field: "events", Msg: fmt.Sprintf("event %d missing timestamp", i)}
		}
	}
	return nil
}

// ValidationError describes a document that a trace reader cannot trust.
type ValidationError struct {
	Field string
	Msg   string
}

func (e ValidationError) Error() string {
	return "trace: invalid " + e.Field + ": " + e.Msg
}
