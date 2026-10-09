package engine

import (
	"context"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// TestSkillsInjectIntoContext (C4): a skill catalog
// injected via AddSystemMessage lands in the
// runtime context the model sees.
func TestSkillsInjectIntoContext(t *testing.T) {
	eng, _, _ := NewEngine(100, provider.NewMockProvider(), tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeReadOnly))
	defer eng.Stop()

	eng.AddSystemMessage("Available skills: deploy, rollback")

	snapshot := eng.runner.Context.Snapshot()
	var found bool
	for _, m := range snapshot {
		if m.Role == "system" && strings.Contains(m.Content, "deploy") && strings.Contains(m.Content, "rollback") {
			found = true
		}
	}
	if !found {
		t.Fatalf("injected skills message not in context: %+v", snapshot)
	}
}

// TestConfigReloadsWithoutRestart (C5): a config
// reload swaps the provider and the permission mode
// live, emitting the resolved configuration.
func TestConfigReloadsWithoutRestart(t *testing.T) {
	reg := tools.DefaultRegistry()
	original := provider.NewMockProvider()
	guard := permissions.NewGuard(permissions.ModeFullAccess)
	eng, cmdChan, eventChan := NewEngine(100, original, reg, guard)
	go eng.Run()
	defer eng.Stop()

	reloaded := provider.NewMockProvider()
	eng.SetConfigReloader(func() (provider.ModelProvider, permissions.Mode, string) {
		return reloaded, permissions.ModeReadOnly, "provider=mock mode=readonly"
	})

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}

	var evt protocol.EngineEvent
	deadline := time.After(5 * time.Second)
	for evt.Type != protocol.EventConfigReloaded {
		select {
		case e, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			if e.Type == protocol.EventConfigReloaded {
				evt = e
			}
		case <-deadline:
			t.Fatal("timed out waiting for config_reloaded")
		}
	}
	if !strings.Contains(evt.Text, "provider=mock") || !strings.Contains(evt.Text, "mode=readonly") {
		t.Errorf("reload summary = %q", evt.Text)
	}
	// The provider was swapped without a restart.
	if eng.runner.Provider != provider.ModelProvider(reloaded) {
		t.Error("provider was not swapped")
	}
	// The permission mode was applied live.
	if guard.Mode != permissions.ModeReadOnly {
		t.Errorf("guard mode = %v, want readonly", guard.Mode)
	}
}

// TestConfigReloadFailedSurfacesError (C5): a failed
// reload reports the error and keeps the current
// configuration.
func TestConfigReloadFailedSurfacesError(t *testing.T) {
	eng, cmdChan, eventChan := NewEngine(100, provider.NewMockProvider(), tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	go eng.Run()
	defer eng.Stop()

	before := eng.runner.Provider
	eng.SetConfigReloader(func() (provider.ModelProvider, permissions.Mode, string) {
		return nil, "", "config reload failed: parse error"
	})

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}

	var evt protocol.EngineEvent
	deadline := time.After(5 * time.Second)
	for evt.Type != protocol.EventError {
		select {
		case e, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			if e.Type == protocol.EventError {
				evt = e
			}
		case <-deadline:
			t.Fatal("timed out waiting for error event")
		}
	}
	if !strings.Contains(evt.Error, "parse error") {
		t.Errorf("error event = %q", evt.Error)
	}
	if eng.runner.Provider != before {
		t.Error("failed reload must keep the current provider")
	}
}

// TestConfigReloadDeferredDuringTurn (C5): a reload
// requested mid-turn is deferred, not applied.
func TestConfigReloadDeferredDuringTurn(t *testing.T) {
	slow := newBlockingProvider()
	eng, cmdChan, eventChan := NewEngine(100, slow, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	go eng.Run()
	defer eng.Stop()

	reloaded := provider.NewMockProvider()
	eng.SetConfigReloader(func() (provider.ModelProvider, permissions.Mode, string) {
		return reloaded, permissions.ModeReadOnly, "provider=mock mode=readonly"
	})

	// Start a turn that blocks until the test releases it.
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "block"}
	// Wait until the turn is actually streaming.
	select {
	case <-slow.started:
	case <-time.After(5 * time.Second):
		t.Fatal("turn never started")
	}

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}

	var evt protocol.EngineEvent
	deadline := time.After(5 * time.Second)
	for evt.Type != protocol.EventWarning {
		select {
		case e, ok := <-eventChan:
			if !ok {
				t.Fatal("event channel closed early")
			}
			if e.Type == protocol.EventWarning {
				evt = e
			}
		case <-deadline:
			t.Fatal("timed out waiting for deferral warning")
		}
	}
	if !strings.Contains(evt.Text, "deferred") {
		t.Errorf("warning = %q, want deferred", evt.Text)
	}
	// The provider must not have been swapped mid-turn.
	if eng.runner.Provider != provider.ModelProvider(slow) {
		t.Error("provider swapped during a live turn")
	}

	close(slow.release)
}

// blockingProvider stalls a turn until released.
type blockingProvider struct {
	started chan struct{}
	release chan struct{}
	once    sync.Once
}

func newBlockingProvider() *blockingProvider {
	return &blockingProvider{
		started: make(chan struct{}),
		release: make(chan struct{}),
	}
}

func (b *blockingProvider) Name() string { return "blocking" }

func (b *blockingProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	b.once.Do(func() { close(b.started) })
	deltas := make(chan provider.Delta, 1)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		select {
		case <-b.release:
		case <-ctx.Done():
		}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}
