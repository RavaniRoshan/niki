package session

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

const x3Secret = "sk-plantedsecretKEY1234567890"

// X3: secret redaction across the persisted trajectory (sqlite +
// JSONL sidecar) and both exports. A planted secret must appear
// nowhere; the [REDACTED] marker must.
func TestPlantedSecretScrubbedEverywhere(t *testing.T) {
	tmp := t.TempDir()
	store, err := Open(filepath.Join(tmp, "test_sessions.db"))
	if err != nil {
		t.Fatal(err)
	}
	sid := protocol.SessionId("x3-session")
	evt := protocol.EngineEvent{
		Type:      protocol.EventToolCompleted,
		Timestamp: time.Now(),
		SessionID: sid,
		Text:      "called with key=" + x3Secret + " done",
	}
	if err := store.AppendEvent(sid, evt); err != nil {
		t.Fatal(err)
	}
	if err := store.Close(); err != nil {
		t.Fatal(err)
	}

	// 1. Raw files on disk: sqlite + JSONL sidecar.
	leak := false
	_ = filepath.Walk(tmp, func(p string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return nil
		}
		data, rerr := os.ReadFile(p)
		if rerr != nil {
			return nil
		}
		if strings.Contains(string(data), x3Secret) {
			t.Errorf("[X3] secret leaked in %s", p)
			leak = true
		}
		return nil
	})
	if leak {
		t.Fatalf("[X3] planted secret found in persisted files")
	}
	t.Logf("[X3] store files (sqlite + jsonl): 0 hits")

	// 2. Reopened store reads back scrubbed events.
	store2, err := Open(filepath.Join(tmp, "test_sessions.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer store2.Close()
	evts, err := store2.Events(sid)
	if err != nil || len(evts) != 1 {
		t.Fatalf("[X3] events: %v %v", evts, err)
	}
	if strings.Contains(evts[0].Text, x3Secret) || !strings.Contains(evts[0].Text, "[REDACTED]") {
		t.Fatalf("[X3] stored event not scrubbed: %q", evts[0].Text)
	}
	t.Logf("[X3] stored event scrubbed: %q", evts[0].Text)

	// 3. Both exports are clean.
	md, err := ExportToMarkdown(sid, store2)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(md, x3Secret) {
		t.Fatalf("[X3] markdown export leaked")
	}
	html, err := ExportToHTML(sid, store2)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(html, x3Secret) {
		t.Fatalf("[X3] html export leaked")
	}
	t.Logf("[X3] markdown + html exports: 0 hits")
}
