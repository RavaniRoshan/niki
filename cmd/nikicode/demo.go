package main

import (
	"fmt"
	"time"

	"github.com/spf13/cobra"

	"github.com/RavaniRoshan/niki/internal/firstrun"
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
		"show the status": {
			{Tool: "git_status", Args: `{}`},
		},
	}
	// Slow the mock stream so the thinking/running/streaming states are
	// visible in recordings. Mock-only pacing; never a product delay.
	m.ChunkDelay = 120 * time.Millisecond
	m.Scripts["read the main file"] = "Read complete — the file is in the tool cell above."
	m.Scripts["search for worker"] = "Matches are in the tool cell above."
	m.Scripts["show the status"] = "Status is in the tool cell above."
	return m
}

// newDemoCommand is the no-key demo entry point offered by the first-run
// flow (branch 3): it replays the shipped recorded session with an explicit
// recorded label. No model, no network, no spend.
func newDemoCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "demo",
		Short: "Replay the recorded demo session (no key, no network)",
		RunE: func(cmd *cobra.Command, args []string) error {
			demo, err := firstrun.LoadRecordedDemo()
			if err != nil {
				return err
			}
			out := cmd.OutOrStdout()
			fmt.Fprintln(out, "[recorded] session replay — this is a pre-captured recording, not a live model session.")
			fmt.Fprintln(out, "Label:", demo.Label)
			fmt.Fprintln(out)
			for _, e := range demo.Events {
				fmt.Fprintf(out, "  %s> %s\n", e.Role, e.Text)
			}
			fmt.Fprintln(out)
			fmt.Fprintln(out, "[end of recording] Set ANTHROPIC_API_KEY / OPENAI_API_KEY, or start Ollama, to run live.")
			return nil
		},
	}
}
