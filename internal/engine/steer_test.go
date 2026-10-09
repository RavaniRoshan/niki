package engine

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func TestMidTurnPromptSteering(t *testing.T) {
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	prov.ToolScripts = map[string][]provider.ToolCall{
		"initial prompt": {
			{Tool: "read_file", Args: `{"path":"engine.go"}`},
		},
	}
	prov.Scripts["initial prompt"] = "Finished after steering."

	steerChan := make(chan string, 4)
	ca := NewContextAssembler()
	guard := permissions.NewGuard(permissions.ModeFullAccess)
	reg := tools.DefaultRegistry()

	runner := &TurnRunner{
		Provider:     prov,
		Registry:     reg,
		Context:      ca,
		Perm:         guard,
		SteerChannel: steerChan,
	}

	// Queue steering prompt before run begins
	steerChan <- "Please prioritize checking docs instead"

	var events []protocol.EngineEvent
	emit := func(evt protocol.EngineEvent) {
		events = append(events, evt)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	err := runner.Run(ctx, "initial prompt", emit)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify that the steering prompt was added to the context assembler
	foundSteer := false
	for _, m := range ca.Snapshot() {
		if strings.Contains(m.Content, "Please prioritize checking docs instead") {
			foundSteer = true
			break
		}
	}

	if !foundSteer {
		t.Fatalf("expected steering prompt to be present in context snapshot, got messages: %+v", ca.Snapshot())
	}
}
