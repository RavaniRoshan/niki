package engine

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/session"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// L1 acceptance: a fixture stream cancelled at a token boundary leaves a
// consistent session — persisted to the real store with exactly one
// turn_cancelled as the last event, whole-token deltas only, no
// turn_completed, no torn assistant message in context, and the session
// usable for a follow-up turn.

const l1FullAnswer = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen"

func TestCancelAtTokenBoundaryLeavesConsistentSession(t *testing.T) {
	prov := provider.NewMockProvider()
	prov.ChunkDelay = 10 * time.Millisecond // one token every 10ms
	prov.Scripts = map[string]string{
		"slow words":   l1FullAnswer,
		"after cancel": "follow up completed",
	}

	st, err := session.Open(filepath.Join(t.TempDir(), "s.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = st.Close() }()

	eng, cmdChan, eventChan := NewEngine(100, prov, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	eng.SetVerifier(passVerifier(), 0)
	// Same persistence wiring the CLI uses (main.go): observe -> store.
	eng.Observe(func(evt protocol.EngineEvent) {
		_ = st.AppendEvent(eng.SessionID(), evt)
	})
	go eng.Run()
	defer eng.Stop()
	sid := eng.SessionID()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "say slow words please"}

	var deltas int
	interrupted := false
	cancelled := false
	deadline := time.After(10 * time.Second)
	for !cancelled {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed")
			}
			switch evt.Type {
			case protocol.EventAssistantTextDelta:
				deltas++
				if deltas == 3 && !interrupted {
					interrupted = true
					// Cancel between tokens: mid-stream, mid-answer.
					cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
				}
			case protocol.EventTurnCancelled:
				cancelled = true
			case protocol.EventTurnCompleted:
				t.Fatal("turn completed despite mid-token cancel")
			}
		case <-deadline:
			t.Fatalf("timeout; deltas=%d interrupted=%v", deltas, interrupted)
		}
	}
	if !interrupted || deltas < 1 {
		t.Fatalf("need a mid-token cancel with partial output (interrupted=%v deltas=%d)", interrupted, deltas)
	}

	// Let the observer persist the final event (it runs synchronously
	// inside emit, so it is already durable; small settle anyway).
	time.Sleep(50 * time.Millisecond)

	events, err := st.Events(sid)
	if err != nil {
		t.Fatal(err)
	}

	var (
		cancelCount, completedCount int
		deltaTexts                  []string
	)
	for i, evt := range events {
		switch evt.Type {
		case protocol.EventTurnCancelled:
			cancelCount++
			if i != len(events)-1 {
				var after []string
				for _, e2 := range events[i+1:] {
					after = append(after, string(e2.Type))
				}
				t.Errorf("turn_cancelled must be the last event, found after it: %v", after)
			}
		case protocol.EventTurnCompleted:
			completedCount++
		case protocol.EventAssistantTextDelta:
			if evt.Text == "" {
				t.Errorf("torn delta event with empty text at index %d", i)
			}
			deltaTexts = append(deltaTexts, evt.Text)
		}
	}
	if cancelCount != 1 {
		t.Errorf("expected exactly 1 turn_cancelled, got %d", cancelCount)
	}
	if completedCount != 0 {
		t.Errorf("cancelled turn must not record turn_completed, got %d", completedCount)
	}
	joined := strings.Join(deltaTexts, "")
	if joined == "" {
		t.Error("partial output lost from persisted session")
	}
	if !strings.HasPrefix(l1FullAnswer, strings.TrimRight(joined, " ")) &&
		!strings.HasPrefix(l1FullAnswer+" ", joined) {
		t.Errorf("persisted deltas must be whole-token prefix of the answer; got %q", joined)
	}

	// Context consistency: no torn assistant message survives the cancel.
	for _, m := range eng.runner.Context.Snapshot() {
		if m.Role == "assistant" && m.Content != "" &&
			m.Content != l1FullAnswer && m.Content != "follow up completed" {
			t.Errorf("torn assistant message in context after cancel: %q", m.Content)
		}
	}

	// The session stays usable: a follow-up turn runs and completes.
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "after cancel say done"}
	finished := false
	deadline2 := time.After(10 * time.Second)
	for !finished {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed")
			}
			if evt.Type == protocol.EventTurnCompleted {
				finished = true
			}
			if evt.Type == protocol.EventTurnFailed || evt.Type == protocol.EventTurnCancelled {
				t.Fatalf("follow-up turn did not run cleanly: %s %s", evt.Type, evt.Error)
			}
		case <-deadline2:
			t.Fatal("follow-up turn timed out after cancel")
		}
	}
	time.Sleep(50 * time.Millisecond)
	events2, err := st.Events(sid)
	if err != nil {
		t.Fatal(err)
	}
	last := events2[len(events2)-1]
	if last.Type != protocol.EventTurnCompleted {
		t.Errorf("post-cancel session must end in turn_completed, got %s", last.Type)
	}
	t.Logf("[L1] cancel at token %d/%d: 1 turn_cancelled last in store, %d whole-token deltas persisted, follow-up turn completed (store now %d events)",
		deltas, strings.Count(l1FullAnswer, " ")+1, len(deltaTexts), len(events2))
}
