package session

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// JSON export (R3). ExportToJSON writes the session's events as a
// versioned JSON document: a header with the session id, the event
// schema version, and the redacted event list. Readers must tolerate
// unknown fields (forward-compat per U2) — the schema test below
// proves a doc with extra fields still decodes.
type JSONDocument struct {
	Version int                    `json:"version"`
	ID      string                 `json:"id"`
	Events  []protocol.EngineEvent `json:"events"`
}

// ExportToJSON generates a versioned JSON document of the session.
func ExportToJSON(sessionID protocol.SessionId, store *Store) (JSONDocument, error) {
	evts, err := store.Events(sessionID)
	if err != nil {
		return JSONDocument{}, err
	}
	for i := range evts {
		evts[i] = evts[i].Redacted()
	}
	return JSONDocument{
		Version: protocol.EventSchemaVersion,
		ID:      string(sessionID),
		Events:  evts,
	}, nil
}

// MarshalJSONDocument serializes a JSONDocument with stable key order
// (encoding/json marshals struct fields in declaration order).
func MarshalJSONDocument(doc JSONDocument) (string, error) {
	data, err := json.MarshalIndent(doc, "", "  ")
	if err != nil {
		return "", err
	}
	return string(data), nil
}

// ExportToJSONText generates the JSON export as text (R3 golden).
func ExportToJSONText(sessionID protocol.SessionId, store *Store) (string, error) {
	doc, err := ExportToJSON(sessionID, store)
	if err != nil {
		return "", err
	}
	return MarshalJSONDocument(doc)
}

// ExportSession bundles a session (title, events, JSON export) for the
// export golden test (R3). Timestamps are frozen by the caller.
type ExportBundle struct {
	ID     string
	Title  string
	Events []protocol.EngineEvent
}

// BuildExportBundle reads a session from the store.
func BuildExportBundle(store *Store, sid protocol.SessionId, title string) (ExportBundle, error) {
	evts, err := store.Events(sid)
	if err != nil {
		return ExportBundle{}, err
	}
	for i := range evts {
		evts[i] = evts[i].Redacted()
	}
	return ExportBundle{ID: string(sid), Title: title, Events: evts}, nil
}

// JSONLines renders the bundle as one JSON object per line.
func (b ExportBundle) JSONLines() string {
	var sb strings.Builder
	enc := func(v any) string {
		data, err := json.Marshal(v)
		if err != nil {
			return ""
		}
		return string(data)
	}
	sb.WriteString(enc(map[string]any{"type": "session", "id": b.ID, "title": b.Title}))
	sb.WriteString("\n")
	for _, e := range b.Events {
		sb.WriteString(enc(e))
		sb.WriteString("\n")
	}
	return sb.String()
}

// MarkdownTOC renders a compact markdown export of the bundle.
func (b ExportBundle) MarkdownTOC() string {
	var sb strings.Builder
	sb.WriteString("# " + b.Title + "\n\n")
	for _, e := range b.Events {
		if e.Type == protocol.EventTurnStarted {
			sb.WriteString("## " + e.Text + "\n\n")
		}
		if e.Type == protocol.EventTurnCompleted {
			sb.WriteString(e.Text + "\n\n")
		}
		if e.Type == protocol.EventTurnNotVerified {
			sb.WriteString("Unverified: " + e.Text + "\n\n")
		}
	}
	sb.WriteString("_exported: 2026-10-11_\n")
	return sb.String()
}

// ExportSession is the R3 export surface invoked by the fork path
// ("niki fork"). It builds a Markdown + JSON-lines bundle of the
// session and writes both files into the store's log directory under
// an exports/ subfolder, returning their paths. It is the persistence
// side of ExportBundle so a forked session is portable on its own.
func (s *Store) ExportSession(sid protocol.SessionId, title string) (mdPath, jsonlPath string, err error) {
	b, err := BuildExportBundle(s, sid, title)
	if err != nil {
		return "", "", err
	}
	dir := filepath.Join(filepath.Dir(s.logPath), "exports")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", "", err
	}
	mdPath = filepath.Join(dir, string(sid)+".md")
	jsonlPath = filepath.Join(dir, string(sid)+".jsonl")
	if err := os.WriteFile(mdPath, []byte(b.MarkdownTOC()), 0o644); err != nil {
		return "", "", err
	}
	if err := os.WriteFile(jsonlPath, []byte(b.JSONLines()), 0o644); err != nil {
		return "", "", err
	}
	return mdPath, jsonlPath, nil
}

// forkGolden is a stable rendering helper for tests: one line per
// event type plus text, joined by newlines. The acceptance golden
// pins it.
func forkGolden(evts []protocol.EngineEvent) string {
	var lines []string
	for _, e := range evts {
		if e.Text != "" {
			lines = append(lines, fmt.Sprintf("%s: %s", e.Type, e.Text))
		} else {
			lines = append(lines, string(e.Type))
		}
	}
	return strings.Join(lines, "\n")
}
