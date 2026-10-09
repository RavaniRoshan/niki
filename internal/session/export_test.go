package session

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestSessionExportMarkdownAndHTML(t *testing.T) {
	tmp := t.TempDir()
	store, err := Open(filepath.Join(tmp, "test_sessions.db"))
	if err != nil {
		t.Fatalf("Open failed: %v", err)
	}
	defer func() { _ = store.Close() }()

	sessID := protocol.SessionId("test-sess-export-1")
	if err := store.CreateSession(sessID, "Export Test Session"); err != nil {
		t.Fatalf("CreateSession failed: %v", err)
	}

	_ = store.AppendEvent(sessID, protocol.EngineEvent{
		Type:      protocol.EventTurnStarted,
		Timestamp: time.Now(),
		Text:      "Add new greeting function",
	})
	_ = store.AppendEvent(sessID, protocol.EngineEvent{
		Type:      protocol.EventToolStarted,
		Timestamp: time.Now(),
		ToolName:  "edit_file",
	})
	_ = store.AppendEvent(sessID, protocol.EngineEvent{
		Type:      protocol.EventToolCompleted,
		Timestamp: time.Now(),
		Text:      "Updated main.go successfully",
	})
	_ = store.AppendEvent(sessID, protocol.EngineEvent{
		Type:      protocol.EventAssistantTextDelta,
		Timestamp: time.Now(),
		Text:      "I have added the greeting function.",
	})

	md, err := ExportToMarkdown(sessID, store)
	if err != nil {
		t.Fatalf("ExportToMarkdown failed: %v", err)
	}
	if !strings.Contains(md, "Add new greeting function") || !strings.Contains(md, "edit_file") {
		t.Fatalf("markdown missing expected content: %s", md)
	}

	htmlOut, err := ExportToHTML(sessID, store)
	if err != nil {
		t.Fatalf("ExportToHTML failed: %v", err)
	}
	if !strings.Contains(htmlOut, "<!DOCTYPE html>") || !strings.Contains(htmlOut, "edit_file") {
		t.Fatalf("HTML missing expected tags: %s", htmlOut)
	}
}
