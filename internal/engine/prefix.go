package engine

import (
	"encoding/json"
	"sort"
	"strings"

	"github.com/RavaniRoshan/niki/internal/provider"
)

// StablePrefix serializes the cache-stable request prefix (C5):
// the system prompt, then the tool schemas, then the instruction
// files — always in that order, always byte-identical for the same
// inputs, before any volatile conversation content. Providers put
// this prefix first on the wire, so prompt-cache reuse is maximal.
// (Instruction contents join here when the C1 hierarchy is wired
// into the turn; the order contract already holds.)
func StablePrefix(system string, specs []provider.ToolSpec, instructions []string) string {
	ordered := append([]provider.ToolSpec{}, specs...)
	sort.Slice(ordered, func(i, j int) bool { return ordered[i].Name < ordered[j].Name })
	var b strings.Builder
	b.WriteString("system:\n")
	b.WriteString(system)
	b.WriteString("\n---tools---\n")
	for _, s := range ordered {
		params, _ := json.Marshal(s.Parameters)
		b.WriteString(s.Name + "\n" + s.Description + "\n" + string(params) + "\n")
	}
	b.WriteString("---instructions---\n")
	for _, in := range instructions {
		b.WriteString(in + "\n")
	}
	return b.String()
}
