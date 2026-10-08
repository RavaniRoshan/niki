package engine

import (
	"context"
	"errors"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Subagent runs a bounded child agent turn with its own
// context window. The child's full event stream stays
// inside the subagent; the parent receives only a
// condensed summary (goal: subagents return only a
// summary to the parent).
type Subagent struct {
	ID       protocol.SubagentId
	Runner   *TurnRunner
	MaxDepth int
	Depth    int
}

func NewSubagent(parent *TurnRunner, maxDepth int) *Subagent {
	child := *parent
	child.Context = NewContextAssembler()
	child.Context.TokenLimit = parent.Context.TokenLimit
	return &Subagent{
		ID:       protocol.NewSubagentId(),
		Runner:   &child,
		MaxDepth: maxDepth,
	}
}

var ErrDepthLimit = errors.New("subagent depth limit reached")

// Summary is the condensed result a subagent returns
// to its parent: the child's final assistant text,
// bounded to summaryMaxChars.
type Summary struct {
	SubagentID protocol.SubagentId
	Text       string
	Tokens     int
}

const summaryMaxChars = 4000

// Run executes the child turn, capturing its event
// stream internally, and returns only a Summary to
// the caller. The emit function passed here receives
// only subagent lifecycle events, never the child's
// assistant deltas.
func (s *Subagent) Run(ctx context.Context, prompt string, emit func(protocol.EngineEvent)) (Summary, error) {
	if s.Depth >= s.MaxDepth {
		return Summary{}, ErrDepthLimit
	}
	s.Depth++
	if emit != nil {
		emit(protocol.EngineEvent{
			Type:      protocol.EventSubagentStarted,
			Timestamp: time.Now(),
			Text:      string(s.ID),
		})
	}

	// The child's deltas are captured here, never
	// forwarded to the parent's event channel.
	var transcript strings.Builder
	childEmit := func(evt protocol.EngineEvent) {
		switch evt.Type {
		case protocol.EventAssistantTextDelta:
			transcript.WriteString(evt.Text)
		}
	}

	err := s.Runner.Run(ctx, prompt, childEmit)
	text := transcript.String()
	tokens := len(text) / 4
	if len(text) > summaryMaxChars {
		text = text[:summaryMaxChars] + "\n[summary truncated]"
	}
	summary := Summary{SubagentID: s.ID, Text: text, Tokens: tokens}

	if emit != nil {
		evtType := protocol.EventSubagentCompleted
		if err != nil {
			evtType = protocol.EventSubagentFailed
		}
		emit(protocol.EngineEvent{
			Type:      evtType,
			Timestamp: time.Now(),
			Text:      string(s.ID),
			Error:     errString(err),
		})
	}
	return summary, err
}

func errString(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}
