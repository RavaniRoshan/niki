package engine

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// recordingStreamer captures the tool specs the engine exposes per turn.
type recordingStreamer struct {
	specs [][]provider.ToolSpec
	reply string
}

func (r *recordingStreamer) Name() string { return "recording" }

func (r *recordingStreamer) StreamWithTools(ctx context.Context, messages []provider.Message, specs []provider.ToolSpec) (<-chan provider.Delta, <-chan error) {
	r.specs = append(r.specs, specs)
	deltas := make(chan provider.Delta, 4)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		deltas <- provider.Delta{Kind: provider.DeltaText, Text: r.reply}
		deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 1, CompletionTokens: 1}}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

// Stream is unused; ToolStreamer wins.
func (r *recordingStreamer) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltas := make(chan provider.Delta)
	errs := make(chan error, 1)
	<-ctx.Done()
	errs <- ctx.Err()
	close(deltas)
	close(errs)
	return deltas, errs
}

// TestEngineExposesRegistryToolsToProvider proves request-side tool exposure:
// the turn runner passes the registry's tool schemas to any provider that
// implements ToolStreamer (the missing request-side half of the tool loop).
func TestEngineExposesRegistryToolsToProvider(t *testing.T) {
	rec := &recordingStreamer{reply: "done"}
	reg := tools.DefaultRegistry()
	eng, cmdChan, eventChan := NewEngine(16, rec, reg, permissions.NewGuard(permissions.ModeWorkspaceWrite))
	eng.SetVerifier(passVerifier(), 0)
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "say hello"}

	deadline := time.After(5 * time.Second)
	for done := false; !done; {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			if evt.Type == protocol.EventTurnCompleted {
				done = true
			}
		case <-deadline:
			t.Fatal("timed out waiting for turn completion")
		}
	}

	if len(rec.specs) == 0 {
		t.Fatal("provider never received tool specs: engine did not call StreamWithTools")
	}
	specs := rec.specs[0]
	if len(specs) < 8 {
		t.Fatalf("expected the full registry exposed, got %d specs", len(specs))
	}
	byName := map[string]provider.ToolSpec{}
	for _, s := range specs {
		byName[s.Name] = s
	}
	read, ok := byName["read_file"]
	if !ok {
		t.Fatal("read_file missing from exposed specs")
	}
	if read.Description == "" {
		t.Fatal("read_file description empty")
	}
	var obj map[string]any
	if err := json.Unmarshal(read.Parameters, &obj); err != nil || obj["type"] != "object" {
		t.Fatalf("read_file parameters not JSON schema: %s", read.Parameters)
	}
	if _, ok := byName["shell"]; !ok {
		t.Fatal("shell missing from exposed specs")
	}
	t.Logf("exposed %d tool specs to provider (read_file params: %s)", len(specs), read.Parameters)
}
