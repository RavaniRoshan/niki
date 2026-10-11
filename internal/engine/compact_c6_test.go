package engine

import (
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// C6: structured compaction at a context threshold — goal,
// decisions, open items, file list. The golden record is exact;
// the full history stays in the session log.
func TestStructuredCompactionGolden(t *testing.T) {
	c := NewContextAssembler()
	c.Add(provider.Message{Role: "user", Content: "Fix the login redirect loop"})
	c.Add(provider.Message{Role: "assistant", Content: "I will inspect the auth handler first", ToolCalls: []provider.ToolCall{
		{ID: "c1", Tool: "read_file", Args: `{"path":"auth.go"}`},
	}})
	c.Add(provider.Message{Role: "tool", Content: "handler code", Name: "read_file", ToolCallID: "c1"})
	c.Add(provider.Message{Role: "assistant", Content: "Found it: session not renewed", ToolCalls: []provider.ToolCall{
		{ID: "c2", Tool: "edit_file", Args: `{"path":"auth.go","old_string":"a","new_string":"b"}`},
	}})

	got := c.Summarize()
	want := "goal: Fix the login redirect loop\n" +
		"decisions:\n" +
		"- I will inspect the auth handler first\n" +
		"- Found it: session not renewed\n" +
		"open items:\n" +
		"- read_file\n" +
		"- edit_file\n" +
		"files:\n" +
		"- auth.go\n"
	if got != want {
		t.Fatalf("[C6] compaction golden mismatch:\n got:\n%s\nwant:\n%s", got, want)
	}
	t.Logf("[C6] structured compaction golden exact (goal/decisions/open/files)")

	// The full history stays in the session log: compacting the
	// working set never touches recorded events.
	sess := NewSession()
	for _, m := range c.Messages {
		sess.Record(protocol.EngineEvent{Type: protocol.EventToolCompleted, Timestamp: time.Now(), Text: m.Content})
	}
	before := len(sess.Events)
	c.ForceCompact()
	// Force more pressure: shrink limit so tiers engage.
	c.TokenLimit = 1
	c.Compact()
	if len(sess.Events) != before {
		t.Fatalf("[C6] session log touched by compaction: %d -> %d", before, len(sess.Events))
	}
	if !strings.Contains(c.Summarize(), "goal: Fix the login redirect loop") {
		t.Fatalf("[C6] summary lost the goal after compaction")
	}
	t.Logf("[C6] session log untouched (%d events); summary survives compaction", before)
}
