package engine

import (
	"context"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// U5: mode switches are real events. The engine emits
// mode_changed for the permission-mode and plan-mode commands,
// carrying the mode the engine actually holds.

func modeEventsFor(t *testing.T, cmds ...protocol.EngineCommand) []protocol.EngineEvent {
	t.Helper()
	guard := permissions.NewGuard(permissions.ModeWorkspaceWrite)
	eng, cmdChan, eventChan := NewEngine(16, provider.NewMockProvider(), tools.DefaultRegistry(), guard)
	go func() { _ = eng.Run() }()
	defer eng.Stop()
	for _, c := range cmds {
		select {
		case cmdChan <- c:
		case <-time.After(time.Second):
			t.Fatal("engine command channel full")
		}
	}
	var events []protocol.EngineEvent
	deadline := time.After(2 * time.Second)
	for len(events) < len(cmds) {
		select {
		case evt := <-eventChan:
			events = append(events, evt)
		case <-deadline:
			t.Fatalf("timed out waiting for mode events, got %d", len(events))
		}
	}
	return events
}

func TestModeChangedEvents(t *testing.T) {
	// plan mode on
	for _, ev := range modeEventsFor(t,
		protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "plan"},
	) {
		if ev.Type == protocol.EventModeChanged {
			if !ev.PlanActive || ev.Mode != string(permissions.ModeReadOnly) {
				t.Fatalf("plan mode event = %+v", ev)
			}
		}
	}
	// full access (yolo)
	for _, ev := range modeEventsFor(t,
		protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "full_access"},
	) {
		if ev.Type == protocol.EventModeChanged {
			if ev.PlanActive || ev.Mode != "full_access" {
				t.Fatalf("yolo mode event = %+v", ev)
			}
		}
	}
	// back to workspace write (act)
	for _, ev := range modeEventsFor(t,
		protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "workspace_write"},
	) {
		if ev.Type == protocol.EventModeChanged {
			if ev.PlanActive || ev.Mode != "workspace_write" {
				t.Fatalf("act mode event = %+v", ev)
			}
		}
	}
}

// U5: the approvals popup's source event is real — the engine's
// prompt handler emits the question event with the safe option
// index and blocks until the frontend answers.
func TestQuestionPromptedEmitter(t *testing.T) {
	guard := permissions.NewGuard(permissions.ModeFullAccess)
	eng, _, eventChan := NewEngine(16, provider.NewMockProvider(), tools.DefaultRegistry(), guard)
	go func() { _ = eng.Run() }()
	defer eng.Stop()

	type result struct {
		answers []string
		err     error
	}
	done := make(chan result, 1)
	go func() {
		// The real prompt handler, as ask_user_question calls it.
		answers, err := eng.promptHandler(context.Background(), []tools.UserQuestion{{
			Header:   "needs you",
			Question: "add golang.org/x/time?",
			Options:  []string{"write the limiter without it", "allow once"},
			Safe:     0,
		}})
		done <- result{answers, err}
	}()

	var qEvt protocol.EngineEvent
	deadline := time.After(2 * time.Second)
	for {
		select {
		case evt := <-eventChan:
			if evt.Type == protocol.EventQuestionPrompted {
				qEvt = evt
			}
		case <-deadline:
			t.Fatal("timed out waiting for question_prompted")
		}
		if qEvt.Type == protocol.EventQuestionPrompted {
			break
		}
	}
	if len(qEvt.Questions) != 1 {
		t.Fatalf("question event carries %d questions", len(qEvt.Questions))
	}
	q := qEvt.Questions[0]
	if q.Safe != 0 || len(q.Options) != 2 || q.Question != "add golang.org/x/time?" {
		t.Fatalf("question event = %+v", q)
	}
	// The answer reaches the waiting handler through the command
	// path the frontend uses.
	eng.deliverAnswer([]string{"allow once"})
	select {
	case r := <-done:
		if r.err != nil {
			t.Fatalf("handler error: %v", r.err)
		}
		if len(r.answers) != 1 || r.answers[0] != "allow once" {
			t.Fatalf("answers = %v", r.answers)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("the handler never received the answer")
	}
}
