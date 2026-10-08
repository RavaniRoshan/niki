package main

import (
	"time"

	"github.com/RavaniRoshan/niki/internal/paths"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// Demo provider scripts (G6 demo): with NIKICODE_DEMO_TOUR=1 the mock
// provider performs scripted tool calls first, so recordings showcase
// the real agent loop (thinking → running tool → streaming → done)
// with zero model spend. Never active unless explicitly enabled.
func demoMockProvider() provider.ModelProvider {
	m := provider.NewMockProvider()
	if paths.Env("DEMO_TOUR") == "" {
		return m
	}
	m.ToolScripts = map[string][]provider.ToolCall{
		"read the main file": {
			{Tool: "read_file", Args: `{"path":"main.go"}`},
		},
		"search for worker": {
			{Tool: "grep", Args: `{"pattern":"Worker"}`},
		},
	}
	// Slow the mock stream so the thinking/running/streaming states are
	// visible in recordings. Mock-only pacing; never a product delay.
	m.ChunkDelay = 120 * time.Millisecond
	m.Scripts["read the main file"] = "Read complete — the file is in the tool cell above."
	m.Scripts["search for worker"] = "Matches are in the tool cell above."
	return m
}
