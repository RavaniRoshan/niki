package engine

import (
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/provider"
)

func msg(role, content string) provider.Message {
	return provider.Message{Role: role, Content: content}
}

// TestTieredCompactionEngagesInOrder (X1): compaction
// escalates soft → medium → hard as usage crosses each
// threshold fraction.
func TestTieredCompactionEngagesInOrder(t *testing.T) {
	c := NewContextAssembler()
	c.TokenLimit = 2000 // 2000 tokens = 8000 chars

	// Below the soft threshold: no compaction.
	c.Add(msg("user", strings.Repeat("a", 1000)))
	if c.Compact() {
		t.Fatal("compaction engaged below the soft threshold")
	}
	if c.LastTier != "" {
		t.Fatalf("no tier should be recorded, got %q", c.LastTier)
	}

	// Cross the soft threshold (~50%): soft tier trims
	// oversized tool output but keeps every message.
	// system+user ≈ 1060 chars (~13%). Add a 2500-char
	// tool output (trimmable: >2000) plus a 2000-char
	// user message → ~5560 chars ≈ 69% (soft band).
	c.Add(msg("user", strings.Repeat("c", 2000)))
	c.Add(msg("tool", strings.Repeat("b", 2500)))
	if !c.Compact() {
		t.Fatal("soft tier should engage past 50% usage")
	}
	if c.LastTier != TierSoft {
		t.Fatalf("expected soft tier, got %q", c.LastTier)
	}
	if len(c.Messages) != 4 {
		t.Fatalf("soft tier must keep every message, got %d", len(c.Messages))
	}
	if !strings.HasSuffix(c.Messages[3].Content, "[trimmed]") {
		t.Fatalf("tool output should be trimmed: %q", c.Messages[3].Content[:40])
	}

	// Cross the medium threshold (~75%): after soft we
	// sit at ~63%; add ~1200 chars to land at ~78%.
	c.Add(msg("user", strings.Repeat("d", 1200)))
	if !c.Compact() {
		t.Fatal("medium tier should engage past 75% usage")
	}
	if c.LastTier != TierMedium {
		t.Fatalf("expected medium tier, got %q", c.LastTier)
	}
	if c.Messages[0].Role != "system" {
		t.Fatal("system message lost in medium tier")
	}

	// Cross the hard threshold (~90%): hard tier keeps
	// system + recent user + trailing evidence only.
	for i := 0; i < 12; i++ {
		c.Add(msg("user", strings.Repeat("e", 500)))
		c.Add(msg("tool", strings.Repeat("f", 500)))
	}
	c.Compact()
	if c.LastTier != TierHard {
		t.Fatalf("expected hard tier at high usage, got %q", c.LastTier)
	}
	if c.Messages[0].Role != "system" {
		t.Fatal("system message lost in hard tier")
	}
}

// TestCompactionPreservesEvidence (X3): no tier ever
// drops the system message or the most recent user
// message — the evidence the user needs survives.
func TestCompactionPreservesEvidence(t *testing.T) {
	c := NewContextAssembler()
	c.TokenLimit = 400 // 1600 chars budget

	for i := 0; i < 30; i++ {
		c.Add(msg("user", "question "+string(rune('a'+i%26))+" "+strings.Repeat("x", 200)))
		c.Add(msg("tool", "output "+string(rune('a'+i%26))+" "+strings.Repeat("y", 200)))
	}
	// Force the hardest tier repeatedly.
	for i := 0; i < 5; i++ {
		c.Compact()
	}
	if c.BreakerTripped {
		t.Skip("breaker tripped; compaction exhausted")
	}

	// System message survives.
	if len(c.Messages) == 0 || c.Messages[0].Role != "system" {
		t.Fatal("system message must survive every tier")
	}
	// Most recent user message survives.
	lastUser := -1
	for i := range c.Messages {
		if c.Messages[i].Role == "user" {
			lastUser = i
		}
	}
	if lastUser < 0 {
		t.Fatal("no user message survived compaction")
	}
	if !strings.Contains(c.Messages[lastUser].Content, "question") {
		t.Fatalf("most recent user message damaged: %q", c.Messages[lastUser].Content)
	}
	// The last user message is the most recent one we added.
	if !strings.Contains(c.Messages[lastUser].Content, "question z") &&
		!strings.Contains(c.Messages[lastUser].Content, "question y") {
		t.Fatalf("expected a recent user message, got %q", c.Messages[lastUser].Content)
	}
	// Trailing tool evidence is retained by the hard tier.
	hasTool := false
	for _, m := range c.Messages {
		if m.Role == "tool" {
			hasTool = true
		}
	}
	if !hasTool {
		t.Fatal("hard tier dropped all tool evidence")
	}
}

// TestCompactionNeverGrowsContext (X1): compaction must
// never increase the message count.
func TestCompactionNeverGrowsContext(t *testing.T) {
	c := NewContextAssembler()
	c.TokenLimit = 200
	for i := 0; i < 50; i++ {
		c.Add(msg("user", strings.Repeat("z", 500)))
		before := len(c.Messages)
		c.Compact()
		if len(c.Messages) > before {
			t.Fatalf("compaction grew the context: %d -> %d", before, len(c.Messages))
		}
	}
}
