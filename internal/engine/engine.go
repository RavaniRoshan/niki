package engine

import (
	"context"
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

	mu         sync.Mutex
	turnCancel context.CancelFunc
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
		runner: &TurnRunner{
			Provider: prov,
			Registry: reg,
			Context:  NewContextAssembler(),
			Perm:     guard,
		},
	}
	return eng, cmdChan, eventChan
}

func (e *Engine) SessionID() protocol.SessionId { return e.session.ID }

func (e *Engine) Run() error {
	defer close(e.eventChan)
	e.emit(protocol.EngineEvent{Type: protocol.EventSessionStarted, Timestamp: time.Now(), SessionID: e.session.ID})
	e.emit(protocol.EngineEvent{Type: protocol.EventSessionReady, Timestamp: time.Now(), SessionID: e.session.ID})

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

func (e *Engine) emit(evt protocol.EngineEvent) {
	e.session.Record(evt)
	select {
	case e.eventChan <- evt:
	default:
	}
}

func (e *Engine) handleCommand(cmd protocol.EngineCommand) {
	switch cmd.Type {
	case protocol.CmdSubmitPrompt:
		e.mu.Lock()
		turnCtx, cancel := context.WithCancel(e.ctx)
		e.turnCancel = cancel
		e.mu.Unlock()
		_ = e.runner.Run(turnCtx, cmd.Prompt, e.emit)
		cancel()
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
	case protocol.CmdStartSession:
		e.emit(protocol.EngineEvent{Type: protocol.EventSessionReady, Timestamp: time.Now(), SessionID: e.session.ID})
	}
}
