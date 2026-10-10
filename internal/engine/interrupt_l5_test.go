package engine

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// l5Provider drives a two-step fixture: step 1 writes file1, step 2
// writes file2 through the gate, then it would claim done.
type l5Provider struct {
	mu   sync.Mutex
	call int
	dir  string
}

func (p *l5Provider) Name() string { return "l5-fixture" }

func (p *l5Provider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	p.call++
	n := p.call
	p.mu.Unlock()

	var calls []provider.ToolCall
	var text string
	switch n {
	case 1:
		calls = []provider.ToolCall{{ID: "w1", Tool: "write_file",
			Args: fmt.Sprintf(`{"path":%q,"content":"content-one-KEPT"}`, filepath.Join(p.dir, "file1.txt"))}}
	case 2:
		calls = []provider.ToolCall{{ID: "w2", Tool: "write_file",
			Args: fmt.Sprintf(`{"path":%q,"content":"content-two-WHOLE"}`, filepath.Join(p.dir, "file2.txt"))}}
	default:
		text = "all writes done"
	}

	deltas := make(chan provider.Delta, 16)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		for _, c := range calls {
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case deltas <- provider.Delta{Kind: provider.DeltaToolCall, CallID: c.ID, ToolName: c.Tool, ToolArgs: c.Args}:
			}
		}
		if text != "" {
			for _, w := range strings.Split(text, " ") {
				select {
				case <-ctx.Done():
					errs <- ctx.Err()
					return
				case deltas <- provider.Delta{Kind: provider.DeltaText, Text: w + " "}:
				}
			}
		}
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 8, CompletionTokens: 4}}:
		}
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case deltas <- provider.Delta{Kind: provider.DeltaDone}:
		}
	}()
	return deltas, errs
}

// gatedWriteTool wraps the product write_file tool for file2.txt: it
// signals entry and blocks until released WITHOUT checking ctx, so a
// hard interrupt lands while the write is "in progress" and the
// product write still has to complete atomically.
type gatedWriteTool struct {
	inner   tools.Tool
	entered chan struct{}
	release chan struct{}
	once    sync.Once
}

func (g *gatedWriteTool) Name() string            { return g.inner.Name() }
func (g *gatedWriteTool) Description() string     { return g.inner.Description() }
func (g *gatedWriteTool) Schema() string          { return g.inner.Schema() }
func (g *gatedWriteTool) IsConcurrencySafe() bool { return g.inner.IsConcurrencySafe() }
func (g *gatedWriteTool) IsReadOnly() bool        { return g.inner.IsReadOnly() }
func (g *gatedWriteTool) Run(ctx context.Context, args json.RawMessage) (tools.ToolResult, error) {
	var a struct {
		Path string `json:"path"`
	}
	_ = json.Unmarshal(args, &a)
	if strings.HasSuffix(a.Path, "file2.txt") {
		g.once.Do(func() { close(g.entered) })
		select {
		case <-g.release:
		}
	}
	return g.inner.Run(ctx, args)
}

// newL5Engine builds an engine over a temp workspace whose write_file
// is the product tool (file2 gated).
func newL5Engine(t *testing.T) (*Engine, chan protocol.EngineCommand, chan protocol.EngineEvent, *gatedWriteTool, string) {
	t.Helper()
	dir := t.TempDir()
	reg := tools.DefaultRegistry()
	gate := &gatedWriteTool{
		inner:   tools.NewWriteFileTool(),
		entered: make(chan struct{}),
		release: make(chan struct{}),
	}
	reg.Register(gate) // overrides write_file

	eng, cmdChan, eventChan := NewEngine(100, &l5Provider{dir: dir}, reg, permissions.NewGuard(permissions.ModeWorkspaceWrite))
	eng.SetVerifier(passVerifier(), 0)
	go func() { _ = eng.Run() }()
	t.Cleanup(eng.Stop)
	return eng, cmdChan, eventChan, gate, dir
}

func collectEvents(ch chan protocol.EngineEvent, until func(protocol.EngineEvent) bool, timeout time.Duration) []protocol.EngineEvent {
	var out []protocol.EngineEvent
	deadline := time.After(timeout)
	for {
		select {
		case evt := <-ch:
			out = append(out, evt)
			if until(evt) {
				return out
			}
		case <-deadline:
			return out
		}
	}
}

func readWhole(t *testing.T, dir, name string) (string, bool) {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		return "", false
	}
	return string(b), true
}

// L5 acceptance: interrupt mid-write, file on disk is whole.
// Esc once -> pause at the safe point; the in-flight write completes.
func TestInterruptMidWriteKeepsFileWhole(t *testing.T) {
	_, cmdChan, eventChan, gate, dir := newL5Engine(t)

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "write files"}

	<-gate.entered                                                 // write of file2 is in progress
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdPauseTurn} // Esc #1: pause
	// Sync on the engine's confirmation that the pause is pending;
	// only then release the gate so the runner deterministically
	// sees the flag. The write still completes atomically (a
	// graceful pause never cancels ctx).
	evts := collectEvents(eventChan, func(e protocol.EngineEvent) bool {
		return e.Type == protocol.EventWarning && strings.Contains(e.Text, "pausing at safe point")
	}, 5*time.Second)
	if !hasEventType(evts, protocol.EventWarning) {
		t.Fatal("[L5] engine never confirmed the pause request")
	}
	close(gate.release)
	evts = append(evts, collectEvents(eventChan, func(e protocol.EngineEvent) bool {
		return e.Type == protocol.EventTurnCancelled
	}, 5*time.Second)...)
	if !hasEventType(evts, protocol.EventTurnCancelled) {
		t.Fatal("[L5] no turn cancelled event after graceful pause")
	}

	if hasEventType(evts, protocol.EventTurnCompleted) {
		t.Fatalf("[L5] paused run must not complete")
	}
	var cancel protocol.EngineEvent
	for _, e := range evts {
		if e.Type == protocol.EventTurnCancelled {
			cancel = e
		}
	}
	if !strings.Contains(cancel.Text, "paused at a safe point") {
		t.Fatalf("[L5] cancel text = %q, want pause at safe point", cancel.Text)
	}

	content, ok := readWhole(t, dir, "file2.txt")
	if !ok || content != "content-two-WHOLE" {
		t.Fatalf("[L5] file2 mid-write interrupted = (%q, %v), want whole new content", content, ok)
	}
	if c1, ok1 := readWhole(t, dir, "file1.txt"); !ok1 || c1 != "content-one-KEPT" {
		t.Fatalf("[L5] file1 = (%q, %v), want kept", c1, ok1)
	}
	t.Logf("[L5 mid-write] file2=%q whole; cancel=%q", content, cancel.Text)
}

// Esc twice -> hard stop; earlier edits kept, in-flight write whole.
func TestEscTwiceStopsAndKeepsAllEdits(t *testing.T) {
	_, cmdChan, eventChan, gate, dir := newL5Engine(t)

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "write files"}

	<-gate.entered                                                 // write of file2 in progress
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdPauseTurn} // Esc #1
	evts := collectEvents(eventChan, func(e protocol.EngineEvent) bool {
		return e.Type == protocol.EventWarning && strings.Contains(e.Text, "pausing at safe point")
	}, 5*time.Second)
	if !hasEventType(evts, protocol.EventWarning) {
		t.Fatal("[L5] engine never confirmed the pause request")
	}
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdPauseTurn} // Esc #2: stop now
	evts = append(evts, collectEvents(eventChan, func(e protocol.EngineEvent) bool {
		return e.Type == protocol.EventWarning && strings.Contains(e.Text, "stopping now")
	}, 5*time.Second)...)
	if !hasEventType(evts, protocol.EventWarning) || !strings.Contains(evts[len(evts)-1].Text, "stopping now") {
		t.Fatal("[L5] engine never confirmed the hard stop")
	}
	close(gate.release)
	evts = append(evts, collectEvents(eventChan, func(e protocol.EngineEvent) bool {
		return e.Type == protocol.EventTurnCancelled
	}, 5*time.Second)...)

	if hasEventType(evts, protocol.EventTurnCompleted) {
		t.Fatalf("[L5] stopped run must not complete")
	}
	if hasEventType(evts, protocol.EventTurnNotVerified) {
		t.Fatalf("[L5] stopped run must not end not-verified")
	}

	// Edits from before the stop are kept.
	if c1, ok := readWhole(t, dir, "file1.txt"); !ok || c1 != "content-one-KEPT" {
		t.Fatalf("[L5] file1 = (%q, %v), want kept edit", c1, ok)
	}
	// The write that was in progress is whole: never torn.
	content, ok := readWhole(t, dir, "file2.txt")
	if !ok {
		t.Fatalf("[L5] file2 missing")
	}
	if content != "content-two-WHOLE" && content != "" {
		// Old content was empty (new file) or full new content.
		t.Fatalf("[L5] file2 = %q, want whole (old or new), never partial", content)
	}
	if content == "" {
		t.Fatalf("[L5] file2 empty — write in progress must complete atomically, got torn/empty file")
	}
	t.Logf("[L5 esc-twice] file1 kept; file2=%q whole", content)
}

func hasEventType(evts []protocol.EngineEvent, want protocol.EventType) bool {
	for _, e := range evts {
		if e.Type == want {
			return true
		}
	}
	return false
}
