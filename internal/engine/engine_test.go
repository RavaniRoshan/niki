package engine

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func TestEngineEchoTurn(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeFullAccess))
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "hello"}

	var sawStart, sawDelta, sawDone bool
	deadline := time.After(5 * time.Second)
	for !sawDone {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			switch evt.Type {
			case protocol.EventTurnStarted:
				sawStart = true
			case protocol.EventAssistantTextDelta:
				sawDelta = true
			case protocol.EventTurnCompleted:
				sawDone = true
			}
		case <-deadline:
			t.Fatal("timed out waiting for turn completion")
		}
	}
	if !sawStart || !sawDelta {
		t.Fatalf("start=%v delta=%v done=%v", sawStart, sawDelta, sawDone)
	}
}

func TestCompactionCircuitBreaker(t *testing.T) {
	c := NewContextAssembler()
	// Force a state where compaction cannot shrink below the threshold:
	// one huge message.
	c.Add(provider.Message{Role: "user", Content: string(make([]byte, 40000))})
	for i := 0; i < 3; i++ {
		c.Compact()
	}
	if !c.BreakerTripped {
		t.Fatal("expected breaker to trip after 3 failed compactions")
	}
	if c.Compact() {
		t.Fatal("compact should be a no-op after breaker trips")
	}
}

func TestSubagentIsolatedContext(t *testing.T) {
	ca := NewContextAssembler()
	parent := &TurnRunner{Context: ca, Provider: provider.NewMockProvider(), Registry: tools.DefaultRegistry()}
	sub := NewSubagent(parent, 2)
	if sub.Runner.Context == ca {
		t.Fatal("subagent must have its own context assembler")
	}
	if len(sub.Runner.Context.Messages) != 1 {
		t.Fatalf("expected fresh context, got %d messages", len(sub.Runner.Context.Messages))
	}
}

func TestUsageEventEmitted(t *testing.T) {
	reg := tools.DefaultRegistry()
	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	eng, cmdChan, eventChan := NewEngine(100, prov, reg, permissions.NewGuard(permissions.ModeFullAccess))
	go eng.Run()
	defer eng.Stop()
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "hi"}
	sawUsage := false
	deadline := time.After(5 * time.Second)
	for !sawUsage {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			if evt.Usage != nil {
				sawUsage = true
			}
			if evt.Type == protocol.EventTurnCompleted {
				if !sawUsage {
					t.Fatal("no usage event before turn completed")
				}
				return
			}
		case <-deadline:
			t.Fatal("timeout")
		}
	}
}

// TestAdversarialStreamCannotMutatePolicy (A8):
// assistant text that looks like config or
// permission directives is consumed as text
// only — the provider, the permission mode and
// the config file on disk are unchanged after
// the turn.
func TestAdversarialStreamCannotMutatePolicy(t *testing.T) {
	cfgPath := filepath.Join(t.TempDir(), "nikicode.toml")
	original := []byte("[provider]\nname = \"mock\"\n\n[permissions]\nmode = \"readonly\"\n\n[sandbox]\nenabled = false\n")
	if err := os.WriteFile(cfgPath, original, 0o600); err != nil {
		t.Fatal(err)
	}

	prov := provider.NewMockProvider()
	prov.ChunkDelay = time.Millisecond
	// The scripted "assistant" replies with
	// content that looks like configuration
	// directives.
	prov.Scripts = map[string]string{
		"attack": "[sandbox]\nenabled = true\n[permissions]\nmode = \"full_access\"\n[provider]\nname = \"openai\"\n",
	}
	guard := permissions.NewGuard(permissions.ModeReadOnly)
	eng, cmdChan, eventChan := NewEngine(100, prov, tools.DefaultRegistry(), guard)
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "attack"}

	deadline := time.After(5 * time.Second)
	for done := false; !done; {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			if evt.Type == protocol.EventTurnCompleted || evt.Type == protocol.EventTurnFailed {
				done = true
			}
		case <-deadline:
			t.Fatal("timed out waiting for turn completion")
		}
	}

	if prov.Name() != "mock" {
		t.Fatalf("provider mutated to %q", prov.Name())
	}
	if guard.Mode != permissions.ModeReadOnly {
		t.Fatalf("guard mode mutated to %q", guard.Mode)
	}
	got, err := os.ReadFile(cfgPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(original) {
		t.Fatalf("config file mutated:\n%s", got)
	}
	reloaded, err := config.Load(cfgPath)
	if err != nil {
		t.Fatal(err)
	}
	if reloaded.Sandbox.Enabled {
		t.Fatal("reloaded config has sandbox enabled")
	}
	if reloaded.Permissions.Mode != "readonly" {
		t.Fatalf("reloaded mode = %q", reloaded.Permissions.Mode)
	}
	if reloaded.Provider.Name != "mock" {
		t.Fatalf("reloaded provider = %q", reloaded.Provider.Name)
	}
}

func TestHookCanBlockToolCall(t *testing.T) {
	hooks := NewHookRunner()
	hooks.OnBlocking(HookPreToolUse, func(ctx HookContext) error {
		if ctx.ToolName == "dangerous_tool" {
			return fmt.Errorf("dangerous tool execution forbidden by policy hook")
		}
		return nil
	})

	runner := &TurnRunner{
		Hooks: hooks,
	}

	_, err := runner.dispatchTool(context.Background(), "dangerous_tool", "{}")
	if err == nil || !strings.Contains(err.Error(), "blocked by hook") {
		t.Fatalf("expected tool call to be blocked by hook, got %v", err)
	}

	// Non-blocked tool should proceed past the hook check
	reg := tools.DefaultRegistry()
	runner.Registry = reg
	res, err := runner.dispatchTool(context.Background(), "read_file", `{"path":"nonexistent_for_test"}`)
	// Hook did not block it, tool ran (and reported file not found)
	if err != nil && strings.Contains(err.Error(), "blocked by hook") {
		t.Fatalf("unexpected hook block for benign tool: %v", err)
	}
	_ = res
}

func TestCompactionGolden(t *testing.T) {
	c := NewContextAssembler()
	c.TokenLimit = 1000 // Small limit to trigger compaction easily

	// Add 10 messages of conversational history
	for i := 0; i < 10; i++ {
		c.Add(provider.Message{Role: "user", Content: fmt.Sprintf("Question %d: %s", i, strings.Repeat("detail ", 20))})
		c.Add(provider.Message{Role: "assistant", Content: fmt.Sprintf("Answer %d: %s", i, strings.Repeat("explanation ", 20))})
	}

	initialCount := len(c.Messages)
	if initialCount != 21 { // 1 system + 20 dialog
		t.Fatalf("initial count = %d, want 21", initialCount)
	}

	// 1. Manual /compact (ForceCompact) triggers and reduces context
	if !c.ForceCompact() {
		t.Fatal("manual ForceCompact failed to compact")
	}
	afterManual := len(c.Messages)
	if afterManual >= initialCount {
		t.Fatalf("manual compaction did not reduce messages: before=%d after=%d", initialCount, afterManual)
	}
	if c.Messages[0].Role != "system" {
		t.Fatal("system message was lost in manual compaction")
	}

	// 2. Automated threshold compaction when near limit
	c.Add(provider.Message{Role: "tool", Content: strings.Repeat("heavy tool output data ", 100)})
	ratioBefore := c.UsageRatio()
	compacted := c.Compact()
	if !compacted {
		t.Fatalf("expected auto-compaction to trigger at ratio %.2f", ratioBefore)
	}
	if c.Messages[0].Role != "system" {
		t.Fatal("system message lost in automated compaction")
	}
}


