package session

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// TestExportSessionWritesFiles exercises the R3 fork export surface
// end-to-end against a real in-memory store (no provider/engine
// needed): forking a session must produce a Markdown + JSON trace
// beside the log.
func TestExportSessionWritesFiles(t *testing.T) {
	dir := t.TempDir()
	st, err := Open(filepath.Join(dir, "sessions.db"))
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	defer st.Close()

	sid := protocol.SessionId("fork-test")
	if err := st.CreateSession(sid, "Forked Session"); err != nil {
		t.Fatalf("create session: %v", err)
	}
	if err := st.AppendEvent(sid, protocol.EngineEvent{Type: protocol.EventTurnStarted, Text: "hello"}); err != nil {
		t.Fatalf("append event: %v", err)
	}

	mdPath, jsonlPath, err := st.ExportSession(sid, "Forked Session")
	if err != nil {
		t.Fatalf("export: %v", err)
	}
	if mdPath == "" || jsonlPath == "" {
		t.Fatalf("expected non-empty paths, got md=%q jsonl=%q", mdPath, jsonlPath)
	}
	if !strings.HasSuffix(mdPath, filepath.Join("exports", "fork-test.md")) {
		t.Fatalf("unexpected md path: %q", mdPath)
	}
	if !strings.HasSuffix(jsonlPath, filepath.Join("exports", "fork-test.jsonl")) {
		t.Fatalf("unexpected jsonl path: %q", jsonlPath)
	}

	md, err := os.ReadFile(mdPath)
	if err != nil {
		t.Fatalf("read md: %v", err)
	}
	if !strings.Contains(string(md), "# Forked Session") {
		t.Fatalf("md missing title: %q", md)
	}
	if _, err := os.ReadFile(jsonlPath); err != nil {
		t.Fatalf("read jsonl: %v", err)
	}
}
