package engine

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// TurnRunner executes a single turn: stream model, dispatch tools, loop.
type TurnRunner struct {
	Provider     provider.ModelProvider
	Registry     *tools.Registry
	Context      *ContextAssembler
	Perm         *permissions.Guard
	Hooks        *HookRunner
	SteerChannel chan string
}

func (t *TurnRunner) Run(ctx context.Context, prompt string, emit func(protocol.EngineEvent)) error {
	turnID := protocol.NewTurnId()
	emit(protocol.EngineEvent{Type: protocol.EventTurnStarted, Timestamp: time.Now(), TurnID: turnID})

	t.Context.Add(provider.Message{Role: "user", Content: prompt})
	if t.Context.Compact() {
		emit(protocol.EngineEvent{Type: protocol.EventContextCompacted, Timestamp: time.Now(), TurnID: turnID})
	}

	const maxSteps = 30
	var (
		lastToolName string
		lastToolArgs string
		failCount    int
	)

	for step := 0; step < maxSteps; step++ {
		// Mid-turn steering: drain any injected steering prompt from user without aborting turn
		if t.SteerChannel != nil {
			select {
			case steer := <-t.SteerChannel:
				if strings.TrimSpace(steer) != "" {
					t.Context.Add(provider.Message{Role: "user", Content: steer})
					emit(protocol.EngineEvent{
						Type:      protocol.EventTurnStarted,
						Timestamp: time.Now(),
						TurnID:    turnID,
						Text:      "steered: " + steer,
					})
				}
			default:
			}
		}

		projected := ProjectContext(t.Context.Snapshot())

		var (
			assistant strings.Builder
			toolCalls []provider.ToolCall
			stepUsage *provider.Usage
		)

		deltas, errs := t.Provider.Stream(ctx, projected)
		for d := range deltas {
			switch d.Kind {
			case provider.DeltaText:
				assistant.WriteString(d.Text)
				emit(protocol.EngineEvent{Type: protocol.EventAssistantTextDelta, Timestamp: time.Now(), TurnID: turnID, Text: d.Text})
			case provider.DeltaToolCall:
				toolCalls = append(toolCalls, provider.ToolCall{
					Tool: d.ToolName,
					Args: d.ToolArgs,
				})
			case provider.DeltaUsage:
				if d.Usage != nil {
					stepUsage = d.Usage
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

		if assistant.Len() > 0 || len(toolCalls) > 0 {
			t.Context.Add(provider.Message{Role: "assistant", Content: assistant.String()})
		}

		// If no tool calls were made, model completed its response
		if len(toolCalls) == 0 {
			if stepUsage == nil {
				emit(protocol.EngineEvent{Type: protocol.EventAssistantMessageDone, Timestamp: time.Now(), TurnID: turnID})
			}
			emit(protocol.EngineEvent{Type: protocol.EventTurnCompleted, Timestamp: time.Now(), TurnID: turnID})
			return nil
		}

		// Dispatch tools
		for _, tc := range toolCalls {
			callID := string(protocol.NewToolCallId())
			emit(protocol.EngineEvent{
				Type:      protocol.EventToolStarted,
				Timestamp: time.Now(),
				TurnID:    turnID,
				CallID:    protocol.ToolCallId(callID),
				ToolName:  tc.Tool,
			})

			res, err := t.dispatchTool(ctx, tc.Tool, tc.Args)
			output := res.Output
			if err != nil {
				emit(protocol.EngineEvent{
					Type:      protocol.EventToolFailed,
					Timestamp: time.Now(),
					TurnID:    turnID,
					CallID:    protocol.ToolCallId(callID),
					Error:     err.Error(),
				})
				output = fmt.Sprintf("Tool error: %v", err)
				if tc.Tool == lastToolName && tc.Args == lastToolArgs {
					failCount++
				} else {
					lastToolName = tc.Tool
					lastToolArgs = tc.Args
					failCount = 1
				}
			} else {
				emit(protocol.EngineEvent{
					Type:      protocol.EventToolCompleted,
					Timestamp: time.Now(),
					TurnID:    turnID,
					CallID:    protocol.ToolCallId(callID),
					ToolName:  tc.Tool,
					Text:      res.Output,
				})
				failCount = 0
			}

			t.Context.Add(provider.Message{Role: "tool", Content: output, Name: tc.Tool})
		}

		// Doom loop guard: 3 consecutive identical failures
		if failCount >= 3 {
			emit(protocol.EngineEvent{
				Type:      protocol.EventWarning,
				Timestamp: time.Now(),
				TurnID:    turnID,
				Text:      fmt.Sprintf("Stopped recurring failing tool call %s after 3 attempts (doom loop guard)", lastToolName),
			})
			break
		}
	}

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
