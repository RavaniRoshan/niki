package engine

import (
	"context"
	"encoding/json"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// TurnRunner executes a single turn: stream model, dispatch tools, loop.
type TurnRunner struct {
	Provider provider.ModelProvider
	Registry *tools.Registry
	Context  *ContextAssembler
	Perm     *permissions.Guard
	Hooks    *HookRunner
}

func (t *TurnRunner) Run(ctx context.Context, prompt string, emit func(protocol.EngineEvent)) error {
	turnID := protocol.NewTurnId()
	emit(protocol.EngineEvent{Type: protocol.EventTurnStarted, Timestamp: time.Now(), TurnID: turnID})

	t.Context.Add(provider.Message{Role: "user", Content: prompt})
	if t.Context.Compact() {
		emit(protocol.EngineEvent{Type: protocol.EventContextCompacted, Timestamp: time.Now(), TurnID: turnID})
	}

	var assistant strings.Builder
	deltas, errs := t.Provider.Stream(ctx, t.Context.Snapshot())
	for d := range deltas {
		switch d.Kind {
		case provider.DeltaText:
			assistant.WriteString(d.Text)
			emit(protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Timestamp: time.Now(), TurnID: turnID, Text: d.Text})
		case provider.DeltaToolCall:
			callID := string(protocol.NewToolCallId())
			emit(protocol.EngineEvent{Type: protocol.EventToolStarted, Timestamp: time.Now(), TurnID: turnID, CallID: protocol.ToolCallId(callID), ToolName: d.ToolName})
			res, err := t.dispatchTool(ctx, d.ToolName, d.ToolArgs)
			if err != nil {
				emit(protocol.EngineEvent{Type: protocol.EventToolFailed, Timestamp: time.Now(), TurnID: turnID, CallID: protocol.ToolCallId(callID), Error: err.Error()})
			} else {
				emit(protocol.EngineEvent{Type: protocol.EventToolCompleted, Timestamp: time.Now(), TurnID: turnID, CallID: protocol.ToolCallId(callID), ToolName: d.ToolName, Text: res.Output})
			}
			t.Context.Add(provider.Message{Role: "tool", Content: res.Output, Name: d.ToolName})
		case provider.DeltaUsage:
			if d.Usage != nil {
				emit(protocol.EngineEvent{Type: protocol.EventAssistantMessageDone, Timestamp: time.Now(), TurnID: turnID, Usage: d.Usage})
			}
		case provider.DeltaDone:
		}
	}
	if err := <-errs; err != nil {
		if ctx.Err() != nil {
			emit(protocol.EngineEvent{Type: protocol.EventTurnCancelled, Timestamp: time.Now(), TurnID: turnID})
			return WrapCancelled()
		}
		emit(protocol.EngineEvent{Type: protocol.EventTurnFailed, Timestamp: time.Now(), TurnID: turnID, Error: err.Error()})
		return WrapProvider(err)
	}

	t.Context.Add(provider.Message{Role: "assistant", Content: assistant.String()})
	emit(protocol.EngineEvent{Type: protocol.EventAssistantMessageDone, Timestamp: time.Now(), TurnID: turnID})
	emit(protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: time.Now(), TurnID: turnID})
	return nil
}

func (t *TurnRunner) dispatchTool(ctx context.Context, name, argsJSON string) (tools.ToolResult, error) {
	if t.Perm != nil && !t.Perm.Allow(name) {
		return tools.ToolResult{}, &Error{Kind: ErrPermission, Message: "tool not allowed in current mode: " + name}
	}
	if t.Hooks != nil {
		if err := t.Hooks.FirePreTool(HookContext{Point: HookPreToolUse, ToolName: name, Payload: argsJSON}); err != nil {
			return tools.ToolResult{}, &Error{Kind: ErrPermission, Message: "blocked by hook: " + err.Error()}
		}
	}
	return t.Registry.Run(ctx, name, jsonRaw(argsJSON))
}

func jsonRaw(s string) json.RawMessage {
	if s == "" {
		return json.RawMessage("{}")
	}
	return json.RawMessage(s)
}
