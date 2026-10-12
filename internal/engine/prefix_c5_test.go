package engine

import (
	"encoding/json"
	"testing"

	"github.com/RavaniRoshan/niki/internal/provider"
)

// C5: prompt-cache-stable prefix — system, tool schemas, and
// instructions come before volatile content. A golden test checks
// the prefix is byte-identical across turns.
func TestStablePrefixByteIdentical(t *testing.T) {
	specs := []provider.ToolSpec{
		{Name: "shell", Description: "Run a shell command", Parameters: json.RawMessage(`{"type":"object"}`)},
		{Name: "read_file", Description: "Read a file", Parameters: json.RawMessage(`{"type":"object"}`)},
	}
	instructions := []string{"global rules", "repo rules"}
	// Two turns: same stable inputs, different volatile tails (which
	// never enter the prefix).
	p1 := StablePrefix("You are NikiCode.", specs, instructions)
	p2 := StablePrefix("You are NikiCode.", specs, instructions)
	if p1 != p2 {
		t.Fatalf("[C5] prefix differs across turns")
	}
	// Order contract: system, then tools, then instructions.
	sys, tools, instr := 0, 0, 0
	for i, ln := range splitLines(p1) {
		switch ln {
		case "system:":
			sys = i
		case "---tools---":
			tools = i
		case "---instructions---":
			instr = i
		}
	}
	if sys >= tools || tools >= instr {
		t.Fatalf("[C5] prefix order wrong:\n%s", p1)
	}
	// Specs sorted by name regardless of input order.
	if idxRF, idxSh := indexOf(p1, "read_file"), indexOf(p1, "shell"); idxRF > idxSh {
		t.Fatalf("[C5] tool specs not sorted")
	}
	// Any stable-input change moves the prefix (no silent reuse).
	if StablePrefix("You are Other.", specs, instructions) == p1 {
		t.Fatalf("[C5] system change invisible in prefix")
	}
	t.Logf("[C5] prefix byte-identical across turns (%d bytes), order system/tools/instructions", len(p1))
}

func splitLines(s string) []string {
	var out []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	return out
}

func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}
