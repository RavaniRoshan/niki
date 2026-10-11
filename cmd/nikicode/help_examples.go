package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/spf13/cobra"
)

// HelpTopics is the topic registry for `nikicode help [topic]` (H1).
var HelpTopics = map[string]string{
	"modes":           "Modes: ask (workspace_write, the default), always (manual: prompt on every action), plan (readonly exploration, edits locked), yolo (full_access: Never Ask). Shift+Tab cycles ask -> always -> plan. Never Ask is never the default and never one keypress away: only the explicit /yolo command confirms it (X2).",
	"tools":           "Tools (36): read_file, write_file, edit_file, apply_patch, glob, grep, shell, web_search, web_fetch, notebook_edit, update_plan, todo_write, exec_command, write_stdin, bash_output, kill_shell, ask_user_question, tool_search, subagents (spawn/send/wait/close/resume), git_* (12), symbol_search. Schemas are strict and versioned (T2); results are typed success/error/timeout (T8); output over 50k is capped head-and-tail with the full text on disk (C2).",
	"verifier":        "Verifier (V1-V4): the request becomes numbered requirements; checks come only from the stack allowlist (go test/vet, npm test, cargo test, pytest — README text is never run); trust tiers T0-T3 with Verified requiring T2+ (inconclusive never passes); test/CI/lockfile integrity is hashed before and after (mid-run deletions block T2). A run ends done only through the verifier (L3).",
	"safety":          "Safety: commands run sandboxed (bubblewrap/seatbelt) with $HOME hidden and no network (X1); approvals default to ask with safest-focused prompts and Esc-deny (X2); secrets are redacted in trajectories, logs, UI and exports (X3); tool output is data, never instructions (X4); untrusted projects lose MCP/approval-escalation/sandbox-weakening (X6); git push is never run by autonomous runs (G3).",
	"sessions":        "Sessions are append-only JSONL (atomic appends) plus sqlite; `nikicode resume` restores after kill -9 (R1). Fork at a checkpoint (R2); export Markdown/JSON (R3). Undo the last agent change with /undo (E4); rewind a turn with /rewind (E3).",
	"budgets":         "Budgets (L2, per run): wall_time 10m, cost $2, steps 100 — configure under [budgets], zero disables. At 85% the run WRAPs (finish the step, check, verdict); at 100% it stops. Usage totals print on turn_completed (O1).",
	"mcp":             "MCP over stdio with per-server allow lists, 10s start timeouts and status events (T3). Tool descriptions are scanned for instruction-like text: poisoned tools are hidden and uncallable. Untrusted projects cannot start MCP servers (X6).",
	"skills":          "Skills are SKILL.md folders (user + project locations), discovered and loaded on demand (T4). Self-extension is default OFF (T5): detect with reason, draft in staging, test offline, declare permissions, propose, gate by scope, enable with provenance, audit, retire.",
	"troubleshooting": "No model? `niki doctor` prints the exact fix per check (I3). Start local with `ollama serve` + `ollama pull <model>` (I5). Stuck loop? Esc pauses at a safe point, Esc twice stops (L5). Never verified? The verifier reason names the failing check (V3).",
}

// newHelpCommand builds `nikicode help [topic]`: bare help renders
// the standard command help; a topic prints its text; an unknown
// topic lists the available ones.
func newHelpCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "help [topic]",
		Short: "Help topics (modes, tools, verifier, safety, sessions, budgets, mcp, skills, troubleshooting)",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return cmd.Parent().Help()
			}
			if text, ok := HelpTopics[args[0]]; ok {
				fmt.Println(text)
				return nil
			}
			names := make([]string, 0, len(HelpTopics))
			for n := range HelpTopics {
				names = append(names, n)
			}
			sort.Strings(names)
			fmt.Printf("unknown topic %q; topics: %s\n", args[0], strings.Join(names, ", "))
			return nil
		},
	}
}

// Example is one runnable example for `nikicode examples` (H1).
// Argv runs headless; Expect must appear in the combined output.
// Fixture, when set, prepares a directory and returns the cwd to
// run the example in.
type Example struct {
	Title   string
	Argv    []string
	Expect  string
	NeedsGo bool
	Fixture func(dir string) string
}

// tinyGoFixture writes a one-test Go module for the verified-run
// example and returns its directory.
func tinyGoFixture(dir string) string {
	files := map[string]string{
		"go.mod":     "module exfx\n\ngo 1.21\n",
		"ex.go":      "package exfx\n\nfunc Add(a, b int) int { return a + b }\n",
		"ex_test.go": "package exfx\n\nimport \"testing\"\n\nfunc TestAdd(t *testing.T) {\n\tif Add(1, 2) != 3 {\n\t\tt.Fatal(\"broken\")\n\t}\n}\n",
	}
	for name, content := range files {
		_ = os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644)
	}
	return dir
}

// RunnableExamples are checked by the docs-snippets test: each one
// runs against the fixture provider and must exit 0 with Expect in
// its output. Keep them fast and hermetic (HOME and cwd are faked).
func RunnableExamples() []Example {
	return []Example{
		{Title: "version", Argv: []string{"--version"}, Expect: "nikicode version"},
		{Title: "help topic", Argv: []string{"help", "verifier"}, Expect: "T2"},
		{Title: "doctor", Argv: []string{"doctor"}, Expect: "doctor:"},
		{Title: "verified exec run", Argv: []string{"exec", "--jsonl", "confirm Add works"}, Expect: "turn_completed", NeedsGo: true, Fixture: tinyGoFixture},
	}
}

func newExamplesCommand() *cobra.Command {
	return &cobra.Command{
		Use:   "examples",
		Short: "Runnable examples (checked by the docs-snippets test)",
		RunE: func(cmd *cobra.Command, args []string) error {
			for i, e := range RunnableExamples() {
				fmt.Printf("%d. %s:\n   nikicode %s\n", i+1, e.Title, strings.Join(e.Argv, " "))
			}
			return nil
		},
	}
}
