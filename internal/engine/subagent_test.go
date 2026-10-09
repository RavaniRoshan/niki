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

// TestSubagentReturnsSummaryOnly (goal 3.1): a subagent
// spawns with an isolated context and returns only a
// summary to the parent — the child's assistant deltas
// never reach the parent's event channel.
func TestSubagentReturnsSummaryOnly(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	parent := &TurnRunner{
		Provider: prov,
		Registry: reg,
		Context:  NewContextAssembler(),
		Perm:     permissions.NewGuard(permissions.ModeFullAccess),
	}
	sub := NewSubagent(parent, 2)

	// The parent-side emit must see only subagent
	// lifecycle events, never the child's text deltas.
	var parentEvents []protocol.EngineEvent
	summary, err := sub.Run(context.Background(), "summarize the repo", func(evt protocol.EngineEvent) {
		parentEvents = append(parentEvents, evt)
	})
	if err != nil {
		t.Fatalf("subagent run failed: %v", err)
	}

	for _, evt := range parentEvents {
		switch evt.Type {
		case protocol.EventAssistantTextDelta:
			t.Fatal("child assistant delta leaked to parent")
		case protocol.EventTurnStarted, protocol.EventTurnCompleted, protocol.EventAssistantMessageDone:
			t.Fatalf("child turn event %v leaked to parent", evt.Type)
		case protocol.EventSubagentStarted, protocol.EventSubagentCompleted:
			// expected lifecycle events
		default:
			t.Fatalf("unexpected parent event %v", evt.Type)
		}
	}

	// The summary carries the child's final text, bounded.
	if summary.SubagentID != sub.ID {
		t.Fatalf("summary id %q != subagent id %q", summary.SubagentID, sub.ID)
	}
	if !strings.Contains(summary.Text, "Acknowledged") {
		t.Fatalf("summary should carry the child's final text, got %q", summary.Text)
	}
	if summary.Tokens <= 0 {
		t.Fatalf("summary should report token usage, got %d", summary.Tokens)
	}

	// The child's context stayed isolated: the parent
	// context has no knowledge of the subagent prompt.
	for _, m := range parent.Context.Messages {
		if strings.Contains(m.Content, "summarize the repo") {
			t.Fatal("subagent prompt leaked into parent context")
		}
	}
}

// TestSubagentDepthLimit: nesting beyond MaxDepth is
// refused with a typed error rather than recursing.
func TestSubagentDepthLimit(t *testing.T) {
	parent := &TurnRunner{
		Provider: provider.NewMockProvider(),
		Registry: tools.DefaultRegistry(),
		Context:  NewContextAssembler(),
	}
	sub := NewSubagent(parent, 1)
	sub.Depth = 1
	_, err := sub.Run(context.Background(), "nested", nil)
	if err != ErrDepthLimit {
		t.Fatalf("expected ErrDepthLimit, got %v", err)
	}
}
