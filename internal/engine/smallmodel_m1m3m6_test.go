package engine

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// repairProvider sends malformed args until it sees the validator
// error + schema excerpt come back, then sends the fixed call.
type repairProvider struct {
	mu        sync.Mutex
	tool      string
	badArgs   string
	goodArgs  string
	fixed     bool
	sawRepair bool
}

func (p *repairProvider) Name() string { return "repair-fixture" }

func (p *repairProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	sawRepair := false
	for _, m := range messages {
		if strings.Contains(m.Content, "failed schema validation") && strings.Contains(m.Content, "version") {
			sawRepair = true
		}
	}
	if sawRepair {
		p.sawRepair = true
	}
	sendGood := sawRepair && !p.fixed
	if sendGood {
		p.fixed = true
	}
	deltas := make(chan provider.Delta, 8)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		if sendGood {
			deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: "c1", ToolName: p.tool, ToolArgs: p.goodArgs}
		} else if !sawRepair {
			deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: "c1", ToolName: p.tool, ToolArgs: p.badArgs}
		} else {
			for _, w := range strings.Split("all fixed done", " ") {
				deltas <- provider.Delta{Kind: provider.DeltaText, Text: w + " "}
			}
		}
		deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 1, CompletionTokens: 1}}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

// M1: malformed arguments recover on the second try — the validator
// error and schema excerpt go back to the model.
func TestToolCallRepairRecovers(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "fix.txt")
	if err := os.WriteFile(target, []byte("repaired-content\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	p := &repairProvider{
		tool:     "read_file",
		badArgs:  `{"path":123}`,
		goodArgs: `{"path":"` + target + `"}`,
	}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read it"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)
	// The validator error + schema excerpt reached the model
	// stand-in (it changed its call), and the fixed call ran.
	p.mu.Lock()
	repairSeen := p.sawRepair
	p.mu.Unlock()
	if !repairSeen {
		t.Fatalf("[M1] validator error + schema never fed back to the model")
	}
	doneSeen := false
	for _, e := range eng.session.Snapshot() {
		if e.Type == protocol.EventToolCompleted && strings.Contains(e.Text, "repaired-content") {
			doneSeen = true
		}
	}
	if !doneSeen {
		t.Fatalf("[M1] fixed call never succeeded")
	}
	t.Logf("[M1] malformed args -> validator error + schema excerpt -> recovered on retry")
}

// stubbornProvider never fixes its args.
type stubbornProvider struct{ tool, args string }

func (p *stubbornProvider) Name() string { return "stubborn-fixture" }

func (p *stubbornProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltas := make(chan provider.Delta, 8)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: "c1", ToolName: p.tool, ToolArgs: p.args}
		deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 1, CompletionTokens: 1}}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

// M1: two retries, then a logged failure — and M6: the second
// identical call earns a reminder, the third stops the loop.
func TestRepairGiveUpAndRepetitionGuard(t *testing.T) {
	p := &stubbornProvider{tool: "read_file", args: `{"path":123}`}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read it"}
	drainUntil(t, eventChan, protocol.EventTurnNotVerified, 60*time.Second)
	events := eng.session.Snapshot()
	var warns []string
	for _, e := range events {
		if e.Type == protocol.EventWarning {
			warns = append(warns, e.Text)
		}
	}
	joined := strings.Join(warns, "\n")
	for _, want := range []string{
		"repair failed after 2 attempts",
		"same tool call twice in a row",
		"different strategy",
		"Stopped recurring failing tool call",
	} {
		if !strings.Contains(joined, want) {
			t.Fatalf("[M1/M6] missing %q in warnings:\n%s", want, joined)
		}
	}
	t.Logf("[M1] two retries then logged failure; [M6] reminder on 2nd identical call, stop on 3rd")
}

// M3: phase-gated tools — edit tools are absent during PLAN.
func TestPhaseGatedToolsAbsentInPlan(t *testing.T) {
	p := &scriptProvider{steps: [][]provider.ToolCall{{}}}
	eng, _, _ := newScriptEngine(t, p)
	eng.runner.Perm.EnterPlanMode()
	names := map[string]bool{}
	for _, s := range eng.runner.toolSpecs() {
		names[s.Name] = true
	}
	for _, hidden := range []string{"edit_file", "write_file", "apply_patch", "notebook_edit", "shell"} {
		if names[hidden] {
			t.Fatalf("[M3] %s exposed during PLAN", hidden)
		}
	}
	for _, shown := range []string{"read_file", "grep", "glob"} {
		if !names[shown] {
			t.Fatalf("[M3] %s missing during PLAN", shown)
		}
	}
	// Dispatch denies them too, even if smuggled past the list.
	if _, err := eng.runner.dispatchTool(context.Background(), "edit_file", `{"path":"x","old_string":"a","new_string":"b"}`); err == nil {
		t.Fatalf("[M3] smuggled edit_file ran during PLAN")
	}
	t.Logf("[M3] PLAN exposes %d read-only tools; edit tools absent + dispatch-denied", len(names))
}
