package engine

import (
	"github.com/RavaniRoshan/niki/internal/provider"
)

const maxContextMessages = 40

// ContextAssembler holds conversation history and performs naive compaction.
type ContextAssembler struct {
	Messages []provider.Message
	// CompactFailures counts consecutive failed compaction attempts; the
	// circuit breaker trips at 3 (X2).
	CompactFailures int
	BreakerTripped  bool
}

func NewContextAssembler() *ContextAssembler {
	return &ContextAssembler{Messages: []provider.Message{{Role: "system", Content: "You are Niki, a fast local AI coding agent."}}}
}

func (c *ContextAssembler) Add(m provider.Message) {
	c.Messages = append(c.Messages, m)
	if len(c.Messages) > maxContextMessages {
		// Keep system + last half
		keep := maxContextMessages / 2
		c.Messages = append([]provider.Message{c.Messages[0]}, c.Messages[len(c.Messages)-keep:]...)
	}
}

func (c *ContextAssembler) Snapshot() []provider.Message {
	return append([]provider.Message{}, c.Messages...)
}

// TokenEstimate is a rough 4-chars-per-token estimate.
func (c *ContextAssembler) TokenEstimate() int {
	total := 0
	for _, m := range c.Messages {
		total += len(m.Content)
	}
	return total / 4
}

// Compact truncates history further when the estimate is large.
func (c *ContextAssembler) Compact() (compacted bool) {
	if c.BreakerTripped {
		return false
	}
	if c.TokenEstimate() > 8000 {
		keep := 8
		if len(c.Messages) > keep+1 {
			c.Messages = append([]provider.Message{c.Messages[0]}, c.Messages[len(c.Messages)-keep:]...)
			c.CompactFailures = 0
			return true
		}
		c.CompactFailures++
		if c.CompactFailures >= 3 {
			c.BreakerTripped = true
		}
	}
	return false
}
