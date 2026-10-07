package engine

import (
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func TestEngineEchoTurn(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeFullAccess))
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "hello"}

	var sawStart, sawDelta, sawDone bool
	deadline := time.After(5 * time.Second)
	for !sawDone {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			switch evt.Type {
			case protocol.EventTurnStarted:
				sawStart = true
			case protocol.EventAssistantTextDelta:
				sawDelta = true
			case protocol.EventTurnCompleted:
				sawDone = true
			}
		case <-deadline:
			t.Fatal("timed out waiting for turn completion")
		}
	}
	if !sawStart || !sawDelta {
		t.Fatalf("start=%v delta=%v done=%v", sawStart, sawDelta, sawDone)
	}
}
