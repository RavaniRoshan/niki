// Package eval runs the behavioral eval suite (Q2): fixed tasks on
// fixture repos, each with an expected verdict, driven through the
// public engine API with deterministic fixture providers. The pass
// rate is stored in docs/finish/EVALS.md. M9 runs the same suite
// against a small local model and appends its (honestly lower) rate.
package eval

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/budget"
	"github.com/RavaniRoshan/niki/internal/checkpoint"
	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
	"github.com/RavaniRoshan/niki/internal/verifier"
)

// verdict kinds a task can expect.
const (
	WantCompleted   = "completed"
	WantNotVerified = "not-verified"
	WantCancelled   = "cancelled"
	WantStopped     = "budget-stopped"
)

// Task is one eval case.
type Task struct {
	ID string
	// Prompt sent to the turn.
	Prompt string
	// Steps are the scripted tool calls, one list per step; a nil
	// step claims completion with text instead.
	Steps [][]provider.ToolCall
	// Verifier: "pass", "fail", or "none".
	Verifier string
	// PreCommands run before the prompt (e.g. enter plan mode).
	PreCommands []protocol.EngineCommand
	// FollowUps run after the terminal event (e.g. undo, rewind).
	FollowUps []protocol.EngineCommand
	// WantTerminal is the expected terminal event type ("" = any
	// completion; use Want* constants via terminalType below).
	WantTerminal protocol.EventType
	// WantTerminalCount, when >1, requires that many terminal
	// events of WantTerminal (multi-turn tasks).
	WantTerminalCount int
	// WantTexts must each appear in some event text/error.
	WantTexts []string
	// WantFile, when set, must exist with WantContent after the run.
	WantFile    string
	WantContent string
	// NeedsCheckpoints wires a checkpoint manager.
	NeedsCheckpoints bool
	// Budget, when non-nil, is installed via SetBudget.
	Budget *budget.Limits
	// Blocker, when set, registers a blocking tool named Blocker
	// released by closing the returned channel.
	Blocker string
}

// scriptProvider replays fixed steps then claims done.
type scriptProvider struct {
	mu    sync.Mutex
	steps [][]provider.ToolCall
	idx   int
}

func (p *scriptProvider) Name() string { return "eval-fixture" }

func (p *scriptProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	var calls []provider.ToolCall
	if p.idx < len(p.steps) {
		calls = p.steps[p.idx]
		p.idx++
	}
	p.mu.Unlock()
	deltas := make(chan provider.Delta, 32)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		if calls != nil {
			for _, c := range calls {
				deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: string(c.ID), ToolName: c.Tool, ToolArgs: c.Args}
			}
		} else {
			for _, w := range strings.Split("task script done", " ") {
				select {
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				case deltas <- provider.Delta{Kind: provider.DeltaText, Text: w + " "}:
				}
			}
		}
		deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 7, CompletionTokens: 7}}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

func passVerifier() verifier.Verifier {
	return verifier.Func(func(context.Context, verifier.Request) (verifier.Verdict, error) {
		return verifier.Verdict{Result: verifier.Verified, Reason: "eval pass"}, nil
	})
}

func failVerifier() verifier.Verifier {
	return verifier.Func(func(context.Context, verifier.Request) (verifier.Verdict, error) {
		return verifier.Verdict{Result: verifier.NotVerified, Reason: "eval always fails"}, nil
	})
}

// blockingTool waits for release before returning.
type blockingTool struct {
	tools.Base
	release chan struct{}
	started chan struct{}
	once    sync.Once
}

func (b *blockingTool) Name() string        { return "blocker" }
func (b *blockingTool) Description() string { return "blocks until released" }

func (b *blockingTool) Run(ctx context.Context, args json.RawMessage) (tools.ToolResult, error) {
	b.once.Do(func() { close(b.started) })
	select {
	case <-ctx.Done():
		return tools.ToolResult{Output: "interrupted", IsError: true}, nil
	case <-b.release:
		return tools.ToolResult{Output: "released"}, nil
	}
}

func (b *blockingTool) Schema() string { return `{"version":"1","required":[],"fields":{}}` }

// Result is one task outcome.
type Result struct {
	ID     string
	Pass   bool
	Detail string
}

// RunTask executes one task in dir (cwd) and judges it.
func RunTask(t Task, dir string) Result {
	oldWd, _ := os.Getwd()
	_ = os.Chdir(dir)
	defer func() { _ = os.Chdir(oldWd) }()

	reg := tools.DefaultRegistry()
	var release chan struct{}
	var started chan struct{}
	if t.Blocker != "" {
		release = make(chan struct{})
		started = make(chan struct{})
		bt := &blockingTool{release: release, started: started}
		bt.Base.SchemaStr = bt.Schema()
		reg.Register(bt)
		_ = release
		_ = started
	}
	_ = filepath.Join
	eng, cmdChan, eventChan := engine.NewEngine(200, &scriptProvider{steps: t.Steps}, reg, permissions.NewGuard(permissions.ModeWorkspaceWrite))
	switch t.Verifier {
	case "pass":
		eng.SetVerifier(passVerifier(), 0)
	case "fail":
		eng.SetVerifier(failVerifier(), 0)
	}
	if t.Budget != nil {
		eng.SetBudget(*t.Budget, "eval-model")
	}
	if t.NeedsCheckpoints {
		eng.SetCheckpoints(checkpoint.NewManager(filepath.Join(dir, "checkpoints")))
	}
	go func() { _ = eng.Run() }()
	defer eng.Stop()

	collect := func(want protocol.EventType, timeout time.Duration) []protocol.EngineEvent {
		var out []protocol.EngineEvent
		deadline := time.After(timeout)
		for {
			select {
			case e := <-eventChan:
				out = append(out, e)
				if want != "" && e.Type == want {
					return out
				}
			case <-deadline:
				return out
			}
		}
	}

	for _, c := range t.PreCommands {
		cmdChan <- c
		time.Sleep(100 * time.Millisecond)
	}
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: t.Prompt}

	// Blocker tasks: wait for the tool to start, then interrupt.
	if t.Blocker != "" {
		<-started
		cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
		close(release)
	}

	terminalSet := map[protocol.EventType]bool{
		protocol.EventTurnCompleted: true, protocol.EventTurnNotVerified: true,
		protocol.EventTurnCancelled: true, protocol.EventTurnFailed: true,
		protocol.EventBudgetStopped: true,
	}
	var events []protocol.EngineEvent
	deadline := time.After(30 * time.Second)
loop:
	for {
		select {
		case e := <-eventChan:
			events = append(events, e)
			if terminalSet[e.Type] {
				break loop
			}
		case <-deadline:
			return Result{ID: t.ID, Detail: "no terminal event in 30s"}
		}
	}

	for _, c := range t.FollowUps {
		cmdChan <- c
		events = append(events, collect("", 3*time.Second)...)
		time.Sleep(200 * time.Millisecond)
	}

	if t.WantTerminal != "" {
		n := 0
		for _, e := range events {
			if e.Type == t.WantTerminal {
				n++
			}
		}
		want := t.WantTerminalCount
		if want < 1 {
			want = 1
		}
		if n < want {
			return Result{ID: t.ID, Detail: fmt.Sprintf("terminal %q x%d, got %d", t.WantTerminal, want, n)}
		}
	}
	blob := ""
	for _, e := range events {
		blob += e.Text + "\n" + e.Error + "\n"
	}
	for _, want := range t.WantTexts {
		if !strings.Contains(blob, want) {
			return Result{ID: t.ID, Detail: fmt.Sprintf("want text %q missing", want)}
		}
	}
	if t.WantFile != "" {
		data, err := os.ReadFile(t.WantFile)
		if err != nil {
			return Result{ID: t.ID, Detail: fmt.Sprintf("want file: %v", err)}
		}
		if t.WantContent != "" && string(data) != t.WantContent {
			return Result{ID: t.ID, Detail: fmt.Sprintf("file content %q", data)}
		}
	}
	return Result{ID: t.ID, Pass: true}
}
