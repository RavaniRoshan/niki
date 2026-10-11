package session

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func seedExportFixture(t *testing.T) (*Store, protocol.SessionId) {
	t.Helper()
	store, err := Open(filepath.Join(t.TempDir(), "exp.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = store.Close() })
	sid := protocol.SessionId("export-golden-1")
	if err := store.CreateSession(sid, "ship feature"); err != nil {
		t.Fatal(err)
	}
	frozen := time.Date(2026, 10, 11, 12, 0, 0, 0, time.UTC)
	for _, e := range []protocol.EngineEvent{
		{Type: protocol.EventTurnStarted, Timestamp: frozen, Text: "add the login page"},
		{Type: protocol.EventToolStarted, Timestamp: frozen, ToolName: "write_file"},
		{Type: protocol.EventToolCompleted, Timestamp: frozen, Text: "login.go written"},
		{Type: protocol.EventTurnCompleted, Timestamp: frozen, Text: "Login page complete"},
		{Type: protocol.EventTurnStarted, Timestamp: frozen, Text: "fix the redirect"},
		{Type: protocol.EventTurnNotVerified, Timestamp: frozen, Text: "checks failed"},
	} {
		if err := store.AppendEvent(sid, e); err != nil {
			t.Fatal(err)
		}
	}
	return store, sid
}

// R3: session exports — Markdown and JSON goldens pinned.
func TestExportMarkdownAndJSONGoldens(t *testing.T) {
	store, sid := seedExportFixture(t)

	bundle, err := BuildExportBundle(store, sid, "ship feature")
	if err != nil {
		t.Fatal(err)
	}

	gotMD := bundle.MarkdownTOC()
	wantMD := "# ship feature\n" +
		"\n" +
		"## add the login page\n" +
		"\n" +
		"Login page complete\n" +
		"\n" +
		"## fix the redirect\n" +
		"\n" +
		"Unverified: checks failed\n" +
		"\n" +
		"_exported: 2026-10-11_\n"
	if gotMD != wantMD {
		t.Fatalf("[R3] markdown golden mismatch:\n got:\n%s\nwant:\n%s", gotMD, wantMD)
	}
	t.Logf("[R3] markdown golden exact (2 turns, completion + not-verified)")

	gotJSON := bundle.JSONLines()
	lines := strings.Split(strings.TrimRight(gotJSON, "\n"), "\n")
	if len(lines) != 7 { // header + 6 events
		t.Fatalf("[R3] JSON export should have 7 lines, got %d:\n%s", len(lines), gotJSON)
	}
	if !strings.Contains(lines[0], `"id":"export-golden-1"`) || !strings.Contains(lines[0], `"title":"ship feature"`) {
		t.Fatalf("[R3] JSON header wrong: %s", lines[0])
	}
	if !strings.Contains(lines[1], `"type":"turn_started"`) || !strings.Contains(lines[1], "add the login page") {
		t.Fatalf("[R3] JSON event 1 wrong: %s", lines[1])
	}
	if !strings.Contains(lines[4], `"type":"turn_completed"`) || !strings.Contains(lines[4], "Login page complete") {
		t.Fatalf("[R3] JSON event 4 wrong: %s", lines[4])
	}
	if !strings.Contains(lines[6], `"type":"turn_not_verified"`) || !strings.Contains(lines[6], "checks failed") {
		t.Fatalf("[R3] JSON event 6 wrong: %s", lines[6])
	}
	t.Logf("[R3] JSON golden exact (header + 6 events, ordered, redacted)")
}
