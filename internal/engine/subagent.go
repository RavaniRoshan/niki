package engine

import (
	"context"
	"errors"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Subagent runs a bounded child agent turn with its own context window.
type Subagent struct {
	ID       protocol.SubagentId
	Runner   *TurnRunner
	MaxDepth int
	Depth    int
}

func NewSubagent(parent *TurnRunner, maxDepth int) *Subagent {
	child := *parent
	child.Context = NewContextAssembler()
	return &Subagent{
		ID:       protocol.NewSubagentId(),
		Runner:   &child,
		MaxDepth: maxDepth,
	}
}

var ErrDepthLimit = errors.New("subagent depth limit reached")

func (s *Subagent) Run(ctx context.Context, prompt string, emit func(protocol.EngineEvent)) error {
	if s.Depth >= s.MaxDepth {
		return ErrDepthLimit
	}
	s.Depth++
	return s.Runner.Run(ctx, prompt, emit)
}
