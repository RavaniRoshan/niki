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

func TestAutonomousMultiStepToolRecursionLoop(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	prov.ToolScripts = map[string][]provider.ToolCall{
		"read main file": {
			{Tool: "read_file", Args: `{"path":"engine.go"}`},
		},
	}
	prov.Scripts["read main file"] = "Autonomous loop completed: file was read successfully."

	guard := permissions.NewGuard(permissions.ModeFullAccess)
	runner := &TurnRunner{
		Provider: prov,
		Registry: reg,
		Context:  NewContextAssembler(),
		Perm:     guard,
	}

	var events []protocol.EngineEvent
	emit := func(evt protocol.EngineEvent) {
		events = append(events, evt)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	err := runner.Run(ctx, "please read main file", emit)
	if err != nil {
		t.Fatalf("turn failed: %v", err)
	}

	// Verify event sequence across the multi-step recursion
	var sawTurnStart, sawToolStart, sawToolComplete, sawTurnComplete bool
	var assistantTextCount int

	for _, ev := range events {
		switch ev.Type {
		case protocol.EventTurnStarted:
			sawTurnStart = true
		case protocol.EventToolStarted:
			sawToolStart = true
			if ev.ToolName != "read_file" {
				t.Errorf("expected read_file tool start, got %q", ev.ToolName)
			}
		case protocol.EventToolCompleted:
			sawToolComplete = true
		case protocol.EventAssistantTextDelta:
			assistantTextCount++
		case protocol.EventTurnCompleted:
			sawTurnComplete = true
		}
	}

	if !sawTurnStart {
		t.Error("missing EventTurnStarted")
	}
	if !sawToolStart {
		t.Error("missing EventToolStarted")
	}
	if !sawToolComplete {
		t.Error("missing EventToolCompleted")
	}
	if !sawTurnComplete {
		t.Error("missing EventTurnCompleted")
	}
	if assistantTextCount == 0 {
		t.Error("expected assistant text deltas from multi-step response")
	}

	// Verify messages in context reflect both tool execution and final assistant message
	msgs := runner.Context.Snapshot()
	hasUser := false
	hasTool := false
	hasAssistant := false
	for _, m := range msgs {
		if m.Role == "user" {
			hasUser = true
		}
		if m.Role == "tool" {
			hasTool = true
		}
		if m.Role == "assistant" {
			hasAssistant = true
		}
	}

	if !hasUser || !hasTool || !hasAssistant {
		t.Fatalf("context must contain user, tool, and assistant messages: hasUser=%v hasTool=%v hasAssistant=%v", hasUser, hasTool, hasAssistant)
	}
}
