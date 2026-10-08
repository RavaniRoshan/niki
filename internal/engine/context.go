package engine

import (
	"github.com/RavaniRoshan/niki/internal/provider"
)

// Compaction tiers (X1). Compaction escalates tier by tier as
// the context grows: each tier keeps strictly more of the
// conversation than the one above it. No tier ever drops the
// system message or the most recent user message (X3: the
// evidence the user needs — the current task — survives).
const (
	// TierSoft trims old tool output bodies but keeps every
	// message skeleton.
	TierSoft = "soft"
	// TierMedium drops the oldest half of non-essential
	// messages, keeping system + recent history.
	TierMedium = "medium"
	// TierHard keeps only the system message, the last user
	// message, and the last few messages of evidence.
	TierHard = "hard"

	defaultContextLimit  = 8000
	softThresholdRatio   = 0.5
	mediumThresholdRatio = 0.75
	hardThresholdRatio   = 0.9
	// evidenceKeep is how many trailing messages TierHard
	// retains so recent tool evidence is never lost.
	evidenceKeep = 8
)

// maxContextMessages caps the raw message count so the
// assembler can never grow unbounded (X1).
const maxContextMessages = 40

// ContextAssembler holds conversation history and performs
// tiered compaction with a circuit breaker on repeated
// failures (X2).
type ContextAssembler struct {
	Messages []provider.Message
	// TokenLimit is the estimated-token budget; compaction
	// tiers engage as usage crosses fractions of it.
	TokenLimit int
	// CompactFailures counts consecutive failed compaction
	// attempts; the circuit breaker trips at 3 (X2).
	CompactFailures int
	BreakerTripped  bool
	// LastTier records the most aggressive tier applied.
	LastTier string
}

func NewContextAssembler() *ContextAssembler {
	return &ContextAssembler{
		Messages:   []provider.Message{{Role: "system", Content: "You are NikiCode, a fast local personal coding agent."}},
		TokenLimit: defaultContextLimit,
	}
}

func (c *ContextAssembler) Add(m provider.Message) {
	c.Messages = append(c.Messages, m)
	if len(c.Messages) > maxContextMessages {
		c.Compact()
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

// UsageRatio is estimated usage divided by the token limit.
func (c *ContextAssembler) UsageRatio() float64 {
	if c.TokenLimit <= 0 {
		return 0
	}
	return float64(c.TokenEstimate()) / float64(c.TokenLimit)
}

// Compact applies the least aggressive tier whose threshold is
// crossed, and returns whether any compaction was applied.
// The system message (index 0) and the most recent user
// message always survive (X3).
func (c *ContextAssembler) Compact() (compacted bool) {
	if c.BreakerTripped {
		return false
	}
	ratio := c.UsageRatio()
	switch {
	case ratio >= hardThresholdRatio:
		if c.compactHard() {
			c.LastTier = TierHard
			c.CompactFailures = 0
			return true
		}
	case ratio >= mediumThresholdRatio:
		if c.compactMedium() {
			c.LastTier = TierMedium
			c.CompactFailures = 0
			return true
		}
	case ratio >= softThresholdRatio:
		if c.compactSoft() {
			c.LastTier = TierSoft
			c.CompactFailures = 0
			return true
		}
	default:
		return false
	}
	// A tier engaged but could not shrink the context: count
	// the failure and trip the breaker after 3 in a row (X2).
	c.CompactFailures++
	if c.CompactFailures >= 3 {
		c.BreakerTripped = true
	}
	return false
}

// ForceCompact unconditionally applies compaction (used by /compact command).
func (c *ContextAssembler) ForceCompact() bool {
	if c.compactMedium() {
		c.LastTier = TierMedium
		return true
	}
	if c.compactSoft() {
		c.LastTier = TierSoft
		return true
	}
	return false
}

// compactSoft trims oversized tool outputs to a bounded
// skeleton, keeping every message. Returns whether anything
// was trimmed.
func (c *ContextAssembler) compactSoft() bool {
	const maxToolOutput = 2000
	trimmed := false
	for i := 1; i < len(c.Messages); i++ {
		m := &c.Messages[i]
		if m.Role == "tool" && len(m.Content) > maxToolOutput {
			m.Content = m.Content[:maxToolOutput] + "\n[trimmed]"
			trimmed = true
		}
	}
	return trimmed
}

// compactMedium drops the oldest half of the non-essential
// messages while preserving the system message and the most
// recent user message.
func (c *ContextAssembler) compactMedium() bool {
	if len(c.Messages) <= 4 {
		return false
	}
	lastUser := len(c.Messages) - 1
	for i := len(c.Messages) - 1; i > 0; i-- {
		if c.Messages[i].Role == "user" {
			lastUser = i
			break
		}
	}
	// Keep system + messages from the midpoint onward, but
	// never drop below the most recent user message.
	mid := len(c.Messages) / 2
	if mid > lastUser {
		mid = lastUser
	}
	if mid <= 1 {
		return false
	}
	kept := append([]provider.Message{c.Messages[0]}, c.Messages[mid:]...)
	c.Messages = kept
	return true
}

// compactHard keeps the system message, the most recent user
// message, and the last evidenceKeep messages of evidence,
// dropping everything else.
func (c *ContextAssembler) compactHard() bool {
	if len(c.Messages) <= evidenceKeep {
		return false
	}
	lastUser := len(c.Messages) - 1
	for i := len(c.Messages) - 1; i > 0; i-- {
		if c.Messages[i].Role == "user" {
			lastUser = i
			break
		}
	}
	start := len(c.Messages) - evidenceKeep
	if start < 1 {
		start = 1
	}
	// The most recent user message must survive even when
	// it sits outside the trailing evidence window.
	kept := []provider.Message{c.Messages[0]}
	if lastUser >= start {
		kept = append(kept, c.Messages[start:]...)
	} else {
		kept = append(kept, c.Messages[lastUser])
		kept = append(kept, c.Messages[start:]...)
	}
	if len(kept) >= len(c.Messages) {
		return false
	}
	c.Messages = kept
	return true
}
