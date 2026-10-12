package engine

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/RavaniRoshan/niki/internal/budget"
	"github.com/RavaniRoshan/niki/internal/checkpoint"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/session"
	"github.com/RavaniRoshan/niki/internal/tools"
	"github.com/RavaniRoshan/niki/internal/undo"
	"github.com/RavaniRoshan/niki/internal/verifier"
)

type Engine struct {
	cmdChan   chan protocol.EngineCommand
	eventChan chan protocol.EngineEvent
	ctx       context.Context
	cancel    context.CancelFunc
	runner    *TurnRunner
	session   *Session
	obs       func(protocol.EngineEvent)
	readiness *Readiness

	sessionStore SessionStore

	mu         sync.Mutex
	turnCancel context.CancelFunc

	// L2 budgets: limits/model/check applied per turn via beginBudget.
	budgetLimits budget.Limits
	budgetModel  string
	budgetCheck  func(ctx context.Context) (string, error)
	turnRunning  bool

	// L5: first Esc asks for a pause at the next safe point; a
	// second Esc (or CmdInterruptTurn) stops the run now.
	pauseRequested atomic.Bool

	// C3 gates: one pending question at a time; the answer
	// channel hands the frontend's answers to the waiting handler.
	questionMu   sync.Mutex
	questionWait chan []string

	// E3: per-turn checkpoints; set once via SetCheckpoints.
	checkpoints *checkpoint.Manager
	// E4: session-scoped undo stack of the agent's file changes;
	// CmdUndo reverts the newest effect.
	undo *undo.Stack

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
	eng.runner.PauseRequested = func() bool { return eng.pauseRequested.Load() }
	// C3 gates: the ask_user_question tool asks through this handler,
	// which emits the question event and blocks until the frontend
	// answers (CmdAnswerQuestion). The event was defined but never
	// emitted; this is the minimal wiring that makes it real.
	if q, ok := reg.Get("ask_user_question"); ok {
		if aq, ok := q.(*tools.AskUserQuestionTool); ok {
			aq.SetPromptHandler(eng.promptHandler)
		}
	}
	eng.undo = undo.NewStack()
	eng.runner.Undo = eng.undo
	eng.runner.HistoryEvents = eng.session.Snapshot
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

// SetBudget installs the per-run budget limits (L2) and the model name
// used for cost gauging. Limits with every dimension zero disable budgets.
func (e *Engine) SetBudget(l budget.Limits, modelName string) {
	e.budgetLimits = l
	e.budgetModel = modelName
}

// SetVerifier installs the completion-gate verifier (L3): a run ends
// as done only through this verifier. maxLoops caps verification
// attempts per claim; 0 means unbounded (D-04), backstopped by budgets.
func (e *Engine) SetVerifier(v verifier.Verifier, maxLoops int) {
	e.runner.verifier = v
	e.runner.verMaxLoops = maxLoops
	e.runner.verifyAttempt = 0
}

// SetCheckpoints installs the per-turn checkpoint manager (E3): each
// turn snapshots the git tree before doing anything, captures files
// at first touch, and CmdRewind restores from that state.
func (e *Engine) SetCheckpoints(mgr *checkpoint.Manager) {
	e.checkpoints = mgr
	e.runner.Checkpoints = mgr
}

// SetBudgetCheck installs the check invoked when a budget enters WRAP
// or stops the run; its return value becomes the recorded verdict (V
// wires the real verifier here).
func (e *Engine) SetBudgetCheck(fn func(ctx context.Context) (string, error)) {
	e.budgetCheck = fn
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
	case protocol.CmdAnswerQuestion:
		// C3: deliver the gate answer to the waiting prompt handler.
		e.deliverAnswer(cmd.Answers)
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
		e.pauseRequested.Store(false)
		e.runner.beginBudget(e.budgetLimits, e.budgetModel, e.budgetCheck)
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
	case protocol.CmdPauseTurn:
		// L5: Esc #1 requests a pause at the next safe point (the
		// in-flight tool finishes); Esc #2 while a pause is already
		// pending stops the run now, keeping all edits.
		e.mu.Lock()
		if e.turnCancel == nil {
			e.mu.Unlock()
			return
		}
		if e.pauseRequested.Load() {
			cancel := e.turnCancel
			e.mu.Unlock()
			cancel()
			e.emit(protocol.EngineEvent{
				Type:      protocol.EventWarning,
				Timestamp: time.Now(),
				Text:      "stopping now (Esc)",
			})
			return
		}
		e.pauseRequested.Store(true)
		e.mu.Unlock()
		e.emit(protocol.EngineEvent{
			Type:      protocol.EventWarning,
			Timestamp: time.Now(),
			Text:      "pausing at safe point — press Esc again to stop now",
		})
	case protocol.CmdInterruptTurn, protocol.CmdCancelTurn:
		// Immediate stop (Ctrl+C / ACP cancel): cancels ctx outright;
		// a pending Esc pause is superseded by the stop.
		e.mu.Lock()
		if e.turnCancel != nil {
			e.turnCancel()
		}
		e.mu.Unlock()
	case protocol.CmdCompact:
		// U7: real compaction or a real refusal — the frontend's
		// confirmation is no longer a fabrication. The compactor is
		// the context assembler the turns run on.
		e.mu.Lock()
		running := e.turnRunning
		e.mu.Unlock()
		if running {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "compact refused: a turn is running"})
			return
		}
		if e.runner.Context.Compact() {
			e.emit(protocol.EngineEvent{Type: protocol.EventContextCompacted, Timestamp: time.Now()})
			return
		}
		e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "compact: nothing to fold (context is already within budget)"})
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
	case protocol.CmdSetPermissionMode:
		e.mu.Lock()
		if e.runner.Perm != nil {
			switch cmd.Mode {
			case "readonly":
				e.runner.Perm.Mode = permissions.ModeReadOnly
				e.runner.Perm.PlanMode = false
			case "full_access", "yolo", "never_ask":
				e.runner.Perm.Mode = permissions.ModeFullAccess
				e.runner.Perm.PlanMode = false
			case "manual", "always_ask":
				e.runner.Perm.Mode = permissions.ModeManual
				e.runner.Perm.PlanMode = false
			default:
				e.runner.Perm.Mode = permissions.ModeWorkspaceWrite
				e.runner.Perm.PlanMode = false
			}
		}
		mode := cmd.Mode
		planActive := e.runner.Perm != nil && e.runner.Perm.PlanMode
		e.mu.Unlock()
		// U5: the mode switch is its own event — the frontend's
		// mode state and chip render from it.
		e.emitModeChanged(mode, planActive)
	case protocol.CmdSetPlanMode:
		e.mu.Lock()
		if e.runner.Perm != nil {
			if cmd.Mode == "plan" || cmd.Mode == "true" {
				e.runner.Perm.EnterPlanMode()
			} else {
				_ = e.runner.Perm.ExitPlanMode(true)
			}
		}
		planActive := e.runner.Perm != nil && e.runner.Perm.PlanMode
		mode := string(permissions.ModeReadOnly)
		if !planActive && e.runner.Perm != nil {
			mode = string(e.runner.Perm.Mode)
		}
		e.mu.Unlock()
		e.emitModeChanged(mode, planActive)
	case protocol.CmdUndo:
		e.mu.Lock()
		running := e.turnRunning
		e.mu.Unlock()
		if running {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "undo refused: a turn is running"})
			return
		}
		eff, err := e.undo.UndoLast()
		if err != nil {
			msg := "undo refused: " + err.Error()
			if err.Error() == "nothing to undo" {
				msg = "nothing to undo"
			}
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: msg})
			return
		}
		e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "undid " + eff.Tool + " change to " + eff.Path})
	case protocol.CmdRewind:
		e.mu.Lock()
		running := e.turnRunning
		e.mu.Unlock()
		if running {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "rewind refused: a turn is running"})
			return
		}
		if e.checkpoints == nil {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "rewind refused: checkpoints are not enabled"})
			return
		}
		target := string(cmd.TurnID)
		if target == "" {
			target = e.checkpoints.LastTurnID()
			if target == "" {
				e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "rewind refused: no checkpoint yet"})
				return
			}
		}
		res, convEvents, err := e.checkpoints.RewindAll(target, false)
		if err != nil {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "rewind failed: " + err.Error()})
			return
		}
		e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: fmt.Sprintf(
			"rewound to checkpoint %s: restored %d file(s), skipped %d, conversation events %d",
			target, len(res.Restored), len(res.Skipped), len(convEvents))})
	case protocol.CmdUnrevert:
		e.mu.Lock()
		running := e.turnRunning
		e.mu.Unlock()
		if running {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "unrevert refused: a turn is running"})
			return
		}
		if e.checkpoints == nil {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "unrevert refused: checkpoints are not enabled"})
			return
		}
		res, err := e.checkpoints.Unrevert()
		if err != nil {
			e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: "unrevert failed: " + err.Error()})
			return
		}
		e.emit(protocol.EngineEvent{Type: protocol.EventWarning, Timestamp: time.Now(), Text: fmt.Sprintf(
			"unreverted: restored %d file(s)", len(res.Restored))})
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
			// R3 wiring: every fork ("niki fork") leaves a portable
			// export trace — Markdown + JSON lines — beside the
			// session log, so the forked session is usable standalone.
			exportNote := ""
			if ss, ok := store.(*session.Store); ok {
				if _, _, err := ss.ExportSession(newID, "Forked Session"); err == nil {
					exportNote = " · exported"
				}
			}
			summaries, _ := store.ListSessionSummaries()
			e.emit(protocol.EngineEvent{
				Type:      protocol.EventSessionList,
				Timestamp: time.Now(),
				Sessions:  summaries,
				Text:      "Forked session as " + string(newID) + exportNote,
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

// emitModeChanged publishes a real mode switch (U5): the permission
// mode and the plan-mode flag the engine actually holds now.
func (e *Engine) emitModeChanged(mode string, planActive bool) {
	e.emit(protocol.EngineEvent{
		Type:       protocol.EventModeChanged,
		Timestamp:  time.Now(),
		Mode:       mode,
		PlanActive: planActive,
	})
}

// promptHandler is the ask_user_question tool's hook (C3): it emits
// the question event and blocks for the frontend's answer. A
// cancelled turn or a closed channel answers "skipped" so the tool
// never hangs the run.
func (e *Engine) promptHandler(ctx context.Context, questions []tools.UserQuestion) ([]string, error) {
	protocolQuestions := make([]protocol.UserQuestion, 0, len(questions))
	for _, q := range questions {
		safe := q.Safe
		if safe < 0 || safe >= len(q.Options) {
			safe = 0
		}
		protocolQuestions = append(protocolQuestions, protocol.UserQuestion{
			Header:      q.Header,
			Question:    q.Question,
			Options:     q.Options,
			AllowCustom: q.AllowCustom,
			Safe:        safe,
		})
	}
	wait := make(chan []string, 1)
	e.questionMu.Lock()
	if e.questionWait != nil {
		// A second concurrent gate: the first one wins; this call
		// skips rather than deadlocking the run.
		e.questionMu.Unlock()
		return []string{"skipped"}, nil
	}
	e.questionWait = wait
	e.questionMu.Unlock()
	defer func() {
		e.questionMu.Lock()
		if e.questionWait == wait {
			e.questionWait = nil
		}
		e.questionMu.Unlock()
	}()

	e.emit(protocol.EngineEvent{
		Type:      protocol.EventQuestionPrompted,
		Timestamp: time.Now(),
		Questions: protocolQuestions,
	})
	select {
	case answers := <-wait:
		return answers, nil
	case <-ctx.Done():
		return []string{"skipped"}, nil
	}
}

// deliverAnswer routes a CmdAnswerQuestion to the waiting gate (C3).
func (e *Engine) deliverAnswer(answers []string) {
	e.questionMu.Lock()
	wait := e.questionWait
	e.questionMu.Unlock()
	if wait == nil {
		return
	}
	select {
	case wait <- answers:
	default:
	}
}
