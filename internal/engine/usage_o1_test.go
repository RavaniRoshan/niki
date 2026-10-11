package engine

import (
	"os"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// O1: live cost, tokens, time, and steps — visible during the run
// (per-step usage deltas + budget gauges) and in the final report.
// Totals match the fixture's recorded usage exactly.
func TestUsageTotalsMatchFixture(t *testing.T) {
	dir := t.TempDir()
	f := dir + "/u.txt"
	mustWrite(t, f, "v1\n")
	p := &scriptProvider{steps: [][]provider.ToolCall{
		{{ID: "c1", Tool: "read_file", Args: `{"path":"` + f + `"}`}},
		{{ID: "c2", Tool: "read_file", Args: `{"path":"` + f + `"}`}},
	}}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read twice"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)

	events := eng.session.Snapshot()
	// Per-step deltas: 2 tool steps + 1 claim step, 7+7 each.
	gotpt, gotc := 0, 0
	for _, e := range events {
		if e.Usage != nil && e.Type == protocol.EventAssistantMessageDone {
			gotpt, gotc = gotpt+e.Usage.PromptTokens, gotc+e.Usage.CompletionTokens
		}
	}
	if gotpt != 21 || gotc != 21 {
		t.Fatalf("[O1] per-step usage = %d/%d, want 21/21", gotpt, gotc)
	}
	var done *protocol.EngineEvent
	for i := range events {
		if events[i].Type == protocol.EventTurnCompleted {
			done = &events[i]
		}
	}
	if done == nil || done.Usage == nil {
		t.Fatalf("[O1] completed event carries no usage totals")
	}
	if done.Usage.PromptTokens != 21 || done.Usage.CompletionTokens != 21 || done.Usage.TotalTokens != 42 {
		t.Fatalf("[O1] totals = %+v, want 21/21/42", done.Usage)
	}
	if !strings.Contains(done.Text, "3 steps") {
		t.Fatalf("[O1] final report missing steps: %q", done.Text)
	}
	t.Logf("[O1] per-step 21/21 summed exactly; final report: %s", done.Text)
}

func mustWrite(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}
