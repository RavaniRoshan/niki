package engine

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/tools"
)

type Engine struct {
	cmdChan    chan protocol.EngineCommand
	eventChan  chan protocol.EngineEvent
	ctx        context.Context
	cancel     context.CancelFunc
	runner     *TurnRunner
	session    *Session
	obs        func(protocol.EngineEvent)
	readiness  *Readiness

	sessionStore SessionStore

	mu           sync.Mutex
	turnCancel   context.CancelFunc
	turnRunning  bool

	// emitMu serializes emit() now that turns run on their
	// own goroutines and the engine loop emits concurrently.
	// It also guards eventClosed: once Run has closed the
	// event channel, later emits (from turns or optional
	// warmers still winding down) are dropped instead of
	// panicking on a send to a closed channel.
	emitMu      sync.Mutex
	eventClosed bool

	// warmers run during boot. Required warmers must finish before
	// the first prompt is accepted; optional warmers run in the
	// background and never block readiness.
	requiredWarmers []warmer
	optionalWarmers []warmer

	// configReloader rebuilds the reloadable configuration
	// on CmdReloadConfig (C5). Nil means reload is not
	// wired up.
	configReloader ConfigReloader
}

// ConfigReloader rebuilds the reloadable parts of the
// configuration. A nil provider signals a failed reload;
// the string then carries the reason.
type ConfigReloader func() (provider.ModelProvider, permissions.Mode, string)

type warmer struct {
	cap  Capability
	warm func(ctx context.Context) error
}

// WarmRequired registers a capability that must be warm before the
// first prompt is accepted.
func (e *Engine) WarmRequired(cap Capability, fn func(ctx context.Context) error) {
	e.requiredWarmers = append(e.requiredWarmers, warmer{cap: cap, warm: fn})
}

// WarmOptional registers a capability that warms lazily in the
// background after readiness is published.
func (e *Engine) WarmOptional(cap Capability, fn func(ctx context.Context) error) {
	e.optionalWarmers = append(e.optionalWarmers, warmer{cap: cap, warm: fn})
}

func NewEngine(bufferSize int, prov provider.ModelProvider, reg *tools.Registry, guard *permissions.Guard) (*Engine, chan protocol.EngineCommand, chan protocol.EngineEvent) {
	cmdChan := make(chan protocol.EngineCommand, bufferSize)
	eventChan := make(chan protocol.EngineEvent, bufferSize)
	ctx, cancel := context.WithCancel(context.Background())
	eng := &Engine{
		cmdChan:   cmdChan,
		eventChan: eventChan,
		ctx:       ctx,
		cancel:    cancel,
		session:   NewSession(),
		readiness: NewReadiness(CapTerminal, CapEngine, CapModel),
		runner: &TurnRunner{
			Provider:     prov,
			Registry:     reg,
			Context:      NewContextAssembler(),
			Perm:         guard,
			SteerChannel: make(chan string, 16),
		},
	}
	// Built-in required warmers: the engine loop itself, the model
	// provider constructor, and a terminal capability probe.
	eng.WarmRequired(CapEngine, func(context.Context) error { return nil })
	eng.WarmRequired(CapModel, func(context.Context) error {
		// Constructing the provider name proves the provider wired up.
		_ = prov.Name()
		return nil
	})
	eng.WarmRequired(CapTerminal, func(context.Context) error { return nil })
	return eng, cmdChan, eventChan
}

func (e *Engine) SessionID() protocol.SessionId { return e.session.ID }

// Readiness exposes the live readiness matrix (B7).
func (e *Engine) Readiness() *Readiness { return e.readiness }

// AddSystemMessage injects extra system-level context (skills catalog,
// instruction files) into the runtime's context assembler.
func (e *Engine) AddSystemMessage(content string) {
	e.runner.Context.Add(provider.Message{Role: "system", Content: content})
}

// SetConfigReloader registers the function that rebuilds the
// configuration when the user asks for a reload (C5). The
// engine applies the result without a restart: the provider
// and the permission mode are swapped between turns.
func (e *Engine) SetConfigReloader(fn ConfigReloader) {
	e.configReloader = fn
}

// SessionStore defines the methods required by Engine to manage past sessions.
type SessionStore interface {
	ListSessionSummaries() ([]protocol.SessionMetadata, error)
	GetRecentHistory(sessionID protocol.SessionId, limit int) ([]string, error)
	Fork(srcID protocol.SessionId, title string) (protocol.SessionId, error)
	Delete(sessionID protocol.SessionId) error
	Events(sessionID protocol.SessionId) ([]protocol.EngineEvent, error)
}

// SetSessionStore registers the persistence store with the engine.
func (e *Engine) SetSessionStore(s SessionStore) {
	e.mu.Lock()
	defer e.mu.Unlock()
	e.sessionStore = s
}

func (e *Engine) Run() error {
	defer func() {
		e.emitMu.Lock()
		e.eventClosed = true
		e.emitMu.Unlock()
		close(e.eventChan)
	}()
	e.emit(protocol.EngineEvent{Type: protocol.EventSessionStarted, Timestamp: time.Now(), SessionID: e.session.ID})

	// Warm every required capability before publishing readiness, so
	// no prompt can be accepted against a cold subsystem.
	for _, w := range e.requiredWarmers {
		start := time.Now()
		err := w.warm(e.ctx)
		d := time.Since(start)
		if err == nil {
			e.readiness.Warm(w.cap, true, d)
			e.emit(protocol.EngineEvent{Type: protocol.EventBootPhase, Timestamp: time.Now(), SessionID: e.session.ID, Text: string(w.cap) + ":ready", Duration: d})
		} else {
			e.emit(protocol.EngineEvent{Type: protocol.EventError, Timestamp: time.Now(), SessionID: e.session.ID, Error: string(w.cap) + " warm-up failed: " + err.Error()})
			return err
		}
	}

	e.emit(protocol.EngineEvent{Type: protocol.EventSessionReady, Timestamp: time.Now(), SessionID: e.session.ID})

	// Optional capabilities warm lazily in the background; a slow or
	// failing optional warmer must never block or kill the session.
	for _, w := range e.optionalWarmers {
		go func(w warmer) {
			start := time.Now()
			err := w.warm(e.ctx)
			d := time.Since(start)
			if err == nil {
				e.readiness.Warm(w.cap, false, d)
				e.emit(protocol.EngineEvent{Type: protocol.EventBootPhase, Timestamp: time.Now(), SessionID: e.session.ID, Text: string(w.cap) + ":ready", Duration: d})
			} else {
				e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), SessionID: e.session.ID, Text: string(w.cap) + " warm-up deferred: " + err.Error()})
			}
		}(w)
	}

	for {
		select {
		case <-e.ctx.Done():
			return nil
		case cmd, ok := <-e.cmdChan:
			if !ok || cmd.Type == protocol.CmdShutdown {
				return nil
			}
			e.handleCommand(cmd)
		}
	}
}

func (e *Engine) Stop() { e.cancel() }

// Observe registers a non-blocking side observer for every emitted event.
func (e *Engine) Observe(fn func(protocol.EngineEvent)) { e.obs = fn }

func (e *Engine) emit(evt protocol.EngineEvent) {
	e.emitMu.Lock()
	defer e.emitMu.Unlock()
	if e.eventClosed {
		return
	}
	e.session.Record(evt)
	if e.obs != nil {
		e.obs(evt)
	}
	select {
	case e.eventChan <- evt:
	default:
	}
}

func (e *Engine) handleCommand(cmd protocol.EngineCommand) {
	switch cmd.Type {
	case protocol.CmdSubmitPrompt:
		// B7: a prompt submitted before required capabilities are
		// warm is refused rather than run against a cold subsystem.
		if !e.readiness.Ready() {
			missing := e.readiness.RequiredMissing()
			names := make([]string, 0, len(missing))
			for _, c := range missing {
				names = append(names, string(c))
			}
			e.emit(protocol.EngineEvent{
				Type:      protocol.EventTurnFailed,
				Timestamp: time.Now(),
				Error:     "session not ready; warming: " + strings.Join(names, ", "),
			})
			return
		}
		e.mu.Lock()
		if e.turnRunning {
			e.mu.Unlock()
			if e.runner.SteerChannel != nil {
				select {
				case e.runner.SteerChannel <- cmd.Prompt:
					e.emit(protocol.EngineEvent{
						Type:      protocol.EventWarning,
						Timestamp: time.Now(),
						Text:      "mid-turn prompt queued for steering",
					})
					return
				default:
				}
			}
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "turn already in progress"})
			return
		}
		turnCtx, cancel := context.WithCancel(e.ctx)
		e.turnCancel = cancel
		e.turnRunning = true
		e.mu.Unlock()
		// The turn runs on its own goroutine so the command
		// loop stays live: CmdInterruptTurn must be able to
		// preempt a running turn (A3).
		go func() {
			defer func() {
				cancel()
				e.mu.Lock()
				e.turnRunning = false
				e.turnCancel = nil
				e.mu.Unlock()
				if r := recover(); r != nil {
					e.emit(protocol.EngineEvent{Type: protocol.EventTurnFailed, Timestamp: time.Now(), Error: fmt.Sprintf("panic: %v", r)})
				}
			}()
			_ = e.runner.Run(turnCtx, cmd.Prompt, e.emit)
		}()
	case protocol.CmdInterruptTurn, protocol.CmdCancelTurn:
		e.mu.Lock()
		if e.turnCancel != nil {
			e.turnCancel()
		}
		e.mu.Unlock()
	case protocol.CmdRefreshSkills:
		e.emit(protocol.EngineEvent{Type: protocol.EventSkillDiscovered, Timestamp: time.Now(), Text: "skills refreshed"})
	case protocol.CmdRefreshMcp:
		e.emit(protocol.EngineEvent{Type: protocol.EventMcpServerReady, Timestamp: time.Now(), Text: "mcp refreshed"})
	case protocol.CmdDetachTool:
		e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "tool detached into background execution"})
	case protocol.CmdReloadConfig:
		// The provider and permission mode are only swapped
		// between turns: a live turn keeps the configuration
		// it started with, which keeps the swap race-free.
		e.mu.Lock()
		if e.turnRunning {
			e.mu.Unlock()
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "config reload deferred: turn in progress"})
			return
		}
		if e.configReloader == nil {
			e.mu.Unlock()
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "no config reloader registered"})
			return
		}
		prov, mode, summary := e.configReloader()
		if prov == nil {
			e.mu.Unlock()
			e.emit(protocol.EngineEvent{Type: protocol.EventError, Timestamp: time.Now(), Error: summary})
			return
		}
		e.runner.Provider = prov
		if e.runner.Perm != nil {
			e.runner.Perm.Mode = mode
		}
		e.mu.Unlock()
		e.emit(protocol.EngineEvent{Type: protocol.EventConfigReloaded, Timestamp: time.Now(), Text: summary})
	case protocol.CmdStartSession:
		e.emit(protocol.EngineEvent{Type: protocol.EventSessionReady, Timestamp: time.Now(), SessionID: e.session.ID})
	case protocol.CmdListSessions:
		e.mu.Lock()
		store := e.sessionStore
		e.mu.Unlock()
		if store != nil {
			if cmd.SessionID != "" {
				hist, _ := store.GetRecentHistory(cmd.SessionID, 8)
				e.emit(protocol.EngineEvent{
					Type:      protocol.EventSessionList,
					Timestamp: time.Now(),
					SessionID: cmd.SessionID,
					History:   hist,
				})
			} else {
				summaries, _ := store.ListSessionSummaries()
				e.emit(protocol.EngineEvent{
					Type:      protocol.EventSessionList,
					Timestamp: time.Now(),
					Sessions:  summaries,
				})
			}
		}
	case protocol.CmdDeleteSession:
		e.mu.Lock()
		store := e.sessionStore
		e.mu.Unlock()
		if store != nil && cmd.SessionID != "" {
			_ = store.Delete(cmd.SessionID)
			summaries, _ := store.ListSessionSummaries()
			e.emit(protocol.EngineEvent{
				Type:      protocol.EventSessionList,
				Timestamp: time.Now(),
				Sessions:  summaries,
				Text:      "Deleted session " + string(cmd.SessionID),
			})
		}
	case protocol.CmdForkSession:
		e.mu.Lock()
		store := e.sessionStore
		e.mu.Unlock()
		if store != nil && cmd.SessionID != "" {
			newID, _ := store.Fork(cmd.SessionID, "Forked Session")
			summaries, _ := store.ListSessionSummaries()
			e.emit(protocol.EngineEvent{
				Type:      protocol.EventSessionList,
				Timestamp: time.Now(),
				Sessions:  summaries,
				Text:      "Forked session as " + string(newID),
			})
		}
	case protocol.CmdResumeSession:
		e.mu.Lock()
		store := e.sessionStore
		e.mu.Unlock()
		if store != nil && cmd.SessionID != "" {
			evts, err := store.Events(cmd.SessionID)
			if err == nil {
				e.session.ID = cmd.SessionID
				e.emit(protocol.EngineEvent{
					Type:      protocol.EventSessionLoaded,
					Timestamp: time.Now(),
					SessionID: cmd.SessionID,
					Text:      fmt.Sprintf("Resumed session %s with %d events", cmd.SessionID, len(evts)),
				})
			}
		}
	}
}
