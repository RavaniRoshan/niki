package engine

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/checkpoint"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// scriptProvider runs a fixed list of tool calls (one per step) and
// then claims done.
type scriptProvider struct {
	mu    sync.Mutex
	steps [][]provider.ToolCall
	idx   int
}

func (p *scriptProvider) Name() string { return "script-fixture" }

func (p *scriptProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	p.mu.Lock()
	i := p.idx
	var calls []provider.ToolCall
	if i < len(p.steps) {
		calls = p.steps[i]
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
			for _, w := range strings.Split("all script steps done", " ") {
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

func newScriptEngine(t *testing.T, p provider.ModelProvider) (*Engine, chan protocol.EngineCommand, chan protocol.EngineEvent) {
	t.Helper()
	reg := tools.DefaultRegistry()
	eng, cmdChan, eventChan := NewEngine(200, p, reg, permissions.NewGuard(permissions.ModeWorkspaceWrite))
	eng.SetVerifier(passVerifier(), 0)
	go func() { _ = eng.Run() }()
	t.Cleanup(func() { eng.Stop() })
	return eng, cmdChan, eventChan
}

func drainUntil(t *testing.T, eventChan <-chan protocol.EngineEvent, want protocol.EventType, timeout time.Duration) []protocol.EngineEvent {
	t.Helper()
	var events []protocol.EngineEvent
	deadline := time.After(timeout)
	for {
		select {
		case evt := <-eventChan:
			events = append(events, evt)
			if evt.Type == want {
				return events
			}
		case <-deadline:
			t.Fatalf("[fixture] timeout waiting for %s", want)
		}
	}
}

func findWarning(events []protocol.EngineEvent, substr string) (protocol.EngineEvent, bool) {
	for _, e := range events {
		if e.Type == protocol.EventWarning && strings.Contains(e.Text, substr) {
			return e, true
		}
	}
	return protocol.EngineEvent{}, false
}

// E4: the last agent change can be undone; a changed-since file is
// refused (user edits kept); undo with an empty stack says so.
func TestUndoLastAgentChange(t *testing.T) {
	dir := t.TempDir()
	f := filepath.Join(dir, "target.txt")
	if err := os.WriteFile(f, []byte("before-content\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	// Step 1: edit_file before -> after. Step 2: claim.
	prov := &scriptProvider{steps: [][]provider.ToolCall{
		{{ID: "c1", Tool: "edit_file", Args: `{"path":"` + f + `","old_string":"before-content","new_string":"after-content"}`}},
	}}
	_, cmdChan, eventChan := newScriptEngine(t, prov)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "change the file"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)

	if got, _ := os.ReadFile(f); string(got) != "after-content\n" {
		t.Fatalf("[E4] precondition: file = %q", got)
	}

	// Undo the agent's change.
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdUndo}
	warns := drainUntil(t, eventChan, protocol.EventWarning, 5*time.Second)
	if _, ok := findWarning(warns, "undid"); !ok {
		// keep draining: more warnings possible
		for i := 0; i < 5 && !ok; i++ {
			warns = append(warns, drainUntil(t, eventChan, protocol.EventWarning, 5*time.Second)...)
			_, ok = findWarning(warns, "undid")
		}
		if !ok {
			t.Fatalf("[E4] no undo warning in %+v", warns)
		}
	}
	data, _ := os.ReadFile(f)
	if string(data) != "before-content\n" {
		t.Fatalf("[E4] undo did not restore content: %q", data)
	}
	fi, err := os.Stat(f)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 {
		t.Fatalf("[E4] undo lost file mode: got %v want 0600", fi.Mode().Perm())
	}

	// Nothing left to undo.
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdUndo}
	warns = nil
	for i := 0; i < 4; i++ {
		warns = append(warns, drainUntil(t, eventChan, protocol.EventWarning, 5*time.Second)...)
		if _, ok := findWarning(warns, "nothing to undo"); ok {
			t.Logf("[E4] empty-stack message: %s", mustFind(warns, "nothing to undo").Text)
			return
		}
	}
	t.Fatalf("[E4] empty-stack warning missing in %+v", warns)
}

func mustFind(events []protocol.EngineEvent, substr string) protocol.EngineEvent {
	e, _ := findWarning(events, substr)
	return e
}

// E4 refusal: the file changed after the agent edit -> undo refuses
// and the user's content stays; once restored to the agent's state,
// undo works again (the effect is kept, not consumed).
func TestUndoRefusesChangedFileAndKeepsUserEdits(t *testing.T) {
	dir := t.TempDir()
	f := filepath.Join(dir, "shared.txt")
	if err := os.WriteFile(f, []byte("v1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	prov := &scriptProvider{steps: [][]provider.ToolCall{
		{{ID: "c1", Tool: "edit_file", Args: `{"path":"` + f + `","old_string":"v1","new_string":"v2"}`}},
	}}
	_, cmdChan, eventChan := newScriptEngine(t, prov)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "edit"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)

	// The user edits the file after the agent's change.
	if err := os.WriteFile(f, []byte("user-was-here\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdUndo}
	var warns []protocol.EngineEvent
	found := false
	for i := 0; i < 4 && !found; i++ {
		warns = append(warns, drainUntil(t, eventChan, protocol.EventWarning, 5*time.Second)...)
		_, found = findWarning(warns, "undo refused")
	}
	if !found {
		t.Fatalf("[E4] refusal warning missing in %+v", warns)
	}
	t.Logf("[E4 refusal] %s", mustFind(warns, "undo refused").Text)
	data, _ := os.ReadFile(f)
	if string(data) != "user-was-here\n" {
		t.Fatalf("[E4] user edit was clobbered: %q", data)
	}

	// Put the file back to the agent's post-edit state; the kept
	// effect can now be undone.
	if err := os.WriteFile(f, []byte("v2\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdUndo}
	found = false
	warns = nil
	for i := 0; i < 4 && !found; i++ {
		warns = append(warns, drainUntil(t, eventChan, protocol.EventWarning, 5*time.Second)...)
		_, found = findWarning(warns, "undid")
	}
	if !found {
		t.Fatalf("[E4] retry undo missing warning in %+v", warns)
	}
	t.Logf("[E4 retry] %s", mustFind(warns, "undid").Text)
	data, _ = os.ReadFile(f)
	if string(data) != "v1\n" {
		t.Fatalf("[E4] retry undo content = %q, want v1", data)
	}
}

// E3: a checkpoint exists before the turn's edits; rewind restores
// bytes exactly, restores the file mode, removes files the turn
// created, and the checkpoint carries the conversation + a git
// shadow ref (not the user's branches).
func TestCheckpointBeforeTurnAndRewindRestoresBytesAndModes(t *testing.T) {
	dir := t.TempDir()
	f := filepath.Join(dir, "conf.txt")
	g := filepath.Join(dir, "created.txt")
	if err := os.WriteFile(f, []byte("original-bytes\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	prov := &scriptProvider{steps: [][]provider.ToolCall{
		{
			{ID: "c1", Tool: "edit_file", Args: `{"path":"` + f + `","old_string":"original-bytes","new_string":"edited-bytes"}`},
			{ID: "c2", Tool: "write_file", Args: `{"path":"` + g + `","content":"brand-new\n"}`},
		},
	}}
	eng, cmdChan, eventChan := newScriptEngine(t, prov)
	mgr := newTempCheckpointManager(t, dir)
	eng.SetCheckpoints(mgr)

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "edit and create"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)

	if got, _ := os.ReadFile(f); string(got) != "edited-bytes\n" {
		t.Fatalf("[E3] precondition edit: %q", got)
	}
	if _, err := os.Stat(g); err != nil {
		t.Fatalf("[E3] precondition create: %v", err)
	}
	// The turn (or a shell step) also changed the mode:
	if err := os.Chmod(f, 0o755); err != nil {
		t.Fatal(err)
	}

	cps := mgr.ListCheckpoints()
	if len(cps) == 0 {
		t.Fatalf("[E3] no checkpoint created for the turn")
	}
	cp := cps[len(cps)-1]
	if cp.GitTreeSHA == "" {
		t.Fatalf("[E3] no git tree snapshot (shadow ref) recorded")
	}
	// Shadow ref exists in git (not on any user branch), then clean up.
	ref := "refs/niki/checkpoints/" + cp.TurnID
	if out, err := exec.Command("git", "show-ref", ref).CombinedOutput(); err != nil || len(out) == 0 {
		t.Fatalf("[E3] shadow ref %s missing: err=%v out=%s", ref, err, out)
	}
	t.Cleanup(func() { _ = exec.Command("git", "update-ref", "-d", ref).Run() })

	// Conversation snapshot exists (events up to the turn start).
	conv, err := mgr.RewindConversation(string(cp.TurnID))
	if err != nil {
		t.Fatalf("[E3] conversation: %v", err)
	}
	if len(conv) == 0 {
		t.Fatalf("[E3] checkpoint conversation is empty")
	}

	// Rewind via the engine command (between turns).
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdRewind, TurnID: protocol.TurnId(cp.TurnID)}
	var warns []protocol.EngineEvent
	found := false
	for i := 0; i < 4 && !found; i++ {
		warns = append(warns, drainUntil(t, eventChan, protocol.EventWarning, 5*time.Second)...)
		_, found = findWarning(warns, "rewound to checkpoint")
	}
	if !found {
		t.Fatalf("[E3] rewind warning missing in %+v", warns)
	}
	t.Logf("[E3 rewind] %s", mustFind(warns, "rewound to checkpoint").Text)

	// Bytes exactly restored, mode exactly restored:
	data, err := os.ReadFile(f)
	if err != nil {
		t.Fatalf("[E3] file missing after rewind: %v", err)
	}
	if string(data) != "original-bytes\n" {
		t.Fatalf("[E3] bytes = %q, want original-bytes", data)
	}
	fi, err := os.Stat(f)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 {
		t.Fatalf("[E3] mode = %v, want 0600", fi.Mode().Perm())
	}
	// The file created by the turn is gone:
	if _, err := os.Stat(g); err == nil {
		t.Fatalf("[E3] created file survived rewind")
	}
	t.Logf("[E3] bytes+mode restored (%v), created file removed, conversation events=%d, shadow ref=%s",
		fi.Mode().Perm(), len(conv), ref)
}

func newTempCheckpointManager(t *testing.T, base string) *checkpoint.Manager {
	t.Helper()
	return checkpoint.NewManager(filepath.Join(base, "checkpoints"))
}
