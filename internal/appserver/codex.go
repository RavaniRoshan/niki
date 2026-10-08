package appserver

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"sync"
	"sync/atomic"

	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

type CodexServer struct {
	prov      provider.ModelProvider
	reg       *tools.Registry
	guard     *permissions.Guard
	eng       *engine.Engine
	cmdChan   chan protocol.EngineCommand
	eventChan chan protocol.EngineEvent

	mu         sync.Mutex
	turnSeq    atomic.Int64
	activeTurn string
	outWriter  io.Writer
	done       chan struct{}
}

func NewCodexServer(prov provider.ModelProvider, reg *tools.Registry, guard *permissions.Guard) *CodexServer {
	if reg == nil {
		reg = tools.DefaultRegistry()
	}
	if guard == nil {
		guard = permissions.NewGuard(permissions.ModeWorkspaceWrite)
	}
	if prov == nil {
		prov = provider.NewMockProvider()
	}

	eng, cmdChan, eventChan := engine.NewEngine(100, prov, reg, guard)
	srv := &CodexServer{
		prov:      prov,
		reg:       reg,
		guard:     guard,
		eng:       eng,
		cmdChan:   cmdChan,
		eventChan: eventChan,
		done:      make(chan struct{}),
	}
	go func() { _ = eng.Run() }()
	go srv.listenEvents()
	return srv
}

func (s *CodexServer) Stop() {
	s.mu.Lock()
	defer s.mu.Unlock()
	select {
	case <-s.done:
	default:
		close(s.done)
		if s.eng != nil {
			s.eng.Stop()
		}
	}
}

func (s *CodexServer) listenEvents() {
	for {
		select {
		case <-s.done:
			return
		case evt, ok := <-s.eventChan:
			if !ok {
				return
			}
			s.mu.Lock()
			w := s.outWriter
			turnID := s.activeTurn
			s.mu.Unlock()

			if w == nil {
				continue
			}

			var notifType string
			payload := map[string]any{"turnId": turnID}

			switch evt.Type {
			case protocol.EventAssistantTextDelta:
				notifType = "assistantDelta"
				payload["text"] = evt.Text
			case protocol.EventToolStarted:
				notifType = "toolCall"
				payload["name"] = evt.ToolName
				payload["callId"] = string(evt.CallID)
			case protocol.EventToolCompleted:
				notifType = "toolResult"
				payload["name"] = evt.ToolName
				payload["callId"] = string(evt.CallID)
			case protocol.EventTurnCompleted:
				notifType = "completed"
			case protocol.EventTurnFailed:
				notifType = "failed"
				payload["error"] = evt.Error
			case protocol.EventTurnCancelled:
				notifType = "cancelled"
			default:
				continue
			}

			payload["type"] = notifType
			notif := RPCNotification{
				JSONRPC: "2.0",
				Method:  "turn/notification",
				Params:  payload,
			}
			b, _ := json.Marshal(notif)
			s.mu.Lock()
			if s.outWriter != nil {
				_, _ = s.outWriter.Write(append(b, '\n'))
			}
			s.mu.Unlock()
		}
	}
}

func (s *CodexServer) Serve(in io.Reader, out io.Writer) error {
	s.mu.Lock()
	s.outWriter = out
	s.mu.Unlock()

	sc := bufio.NewScanner(in)
	buf := make([]byte, 1024*1024)
	sc.Buffer(buf, 4*1024*1024)

	for sc.Scan() {
		line := sc.Bytes()
		if len(line) == 0 {
			continue
		}

		var req RPCRequest
		if err := json.Unmarshal(line, &req); err != nil {
			s.sendResponse(out, nil, nil, &RPCError{Code: -32700, Message: "Parse error"})
			continue
		}

		s.handleRequest(req, out)
	}
	return sc.Err()
}

func (s *CodexServer) handleRequest(req RPCRequest, out io.Writer) {
	switch req.Method {
	case "initialize":
		res := map[string]any{
			"protocolVersion": "1.0",
			"serverInfo": map[string]string{
				"name":    "nikicode-codex-server",
				"version": "0.11.0",
			},
			"capabilities": map[string]bool{
				"turns": true,
				"diff":  true,
				"plan":  true,
			},
		}
		s.sendResponse(out, req.ID, res, nil)

	case "thread/create":
		s.sendResponse(out, req.ID, map[string]string{"threadId": "th_1"}, nil)

	case "turn/start":
		var params struct {
			Prompt   string `json:"prompt"`
			ThreadID string `json:"threadId"`
		}
		_ = json.Unmarshal(req.Params, &params)

		turnID := fmt.Sprintf("turn_%d", s.turnSeq.Add(1))
		s.mu.Lock()
		s.activeTurn = turnID
		s.mu.Unlock()

		s.cmdChan <- protocol.EngineCommand{
			Type:   protocol.CmdSubmitPrompt,
			Prompt: params.Prompt,
		}

		s.sendResponse(out, req.ID, map[string]string{
			"turnId": turnID,
			"status": "started",
		}, nil)

	case "turn/interrupt":
		s.cmdChan <- protocol.EngineCommand{
			Type: protocol.CmdCancelTurn,
		}
		s.sendResponse(out, req.ID, map[string]string{"status": "interrupted"}, nil)

	case "shutdown":
		s.Stop()
		s.sendResponse(out, req.ID, map[string]string{"status": "ok"}, nil)

	default:
		s.sendResponse(out, req.ID, nil, &RPCError{
			Code:    -32601,
			Message: fmt.Sprintf("Method not found: %s", req.Method),
		})
	}
}

func (s *CodexServer) sendResponse(out io.Writer, id any, result any, rpcErr *RPCError) {
	resp := RPCResponse{
		JSONRPC: "2.0",
		ID:      id,
		Result:  result,
		Error:   rpcErr,
	}
	b, _ := json.Marshal(resp)
	s.mu.Lock()
	defer s.mu.Unlock()
	_, _ = out.Write(append(b, '\n'))
}
