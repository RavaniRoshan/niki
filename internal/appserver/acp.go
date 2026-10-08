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

// ACPServer implements Agent Client Protocol (ACP) JSON-RPC 2.0 adapter.
type ACPServer struct {
	prov      provider.ModelProvider
	reg       *tools.Registry
	guard     *permissions.Guard
	eng       *engine.Engine
	cmdChan   chan protocol.EngineCommand
	eventChan chan protocol.EngineEvent

	mu            sync.Mutex
	sessionSeq    atomic.Int64
	activeSession string
	outWriter     io.Writer
	done          chan struct{}
}

func NewACPServer(prov provider.ModelProvider, reg *tools.Registry, guard *permissions.Guard) *ACPServer {
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
	srv := &ACPServer{
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

func (s *ACPServer) Stop() {
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

func (s *ACPServer) listenEvents() {
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
			sessID := s.activeSession
			s.mu.Unlock()

			if w == nil {
				continue
			}

			payload := map[string]any{"sessionId": sessID}
			switch evt.Type {
			case protocol.EventAssistantTextDelta:
				payload["updateType"] = "textDelta"
				payload["content"] = evt.Text
			case protocol.EventToolStarted:
				payload["updateType"] = "toolCall"
				payload["name"] = evt.ToolName
				payload["callId"] = string(evt.CallID)
			case protocol.EventToolCompleted:
				payload["updateType"] = "toolResult"
				payload["name"] = evt.ToolName
				payload["callId"] = string(evt.CallID)
			case protocol.EventTurnCompleted:
				payload["updateType"] = "completed"
			case protocol.EventTurnFailed:
				payload["updateType"] = "failed"
				payload["error"] = evt.Error
			case protocol.EventTurnCancelled:
				payload["updateType"] = "cancelled"
			default:
				continue
			}

			notif := RPCNotification{
				JSONRPC: "2.0",
				Method:  "session/update",
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

func (s *ACPServer) Serve(in io.Reader, out io.Writer) error {
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

func (s *ACPServer) handleRequest(req RPCRequest, out io.Writer) {
	switch req.Method {
	case "initialize":
		res := map[string]any{
			"protocolVersion": "1.0",
			"capabilities": map[string]bool{
				"sessions":  true,
				"streaming": true,
				"tools":     true,
			},
			"serverInfo": map[string]string{
				"name":    "nikicode-acp-server",
				"version": "0.11.0",
			},
		}
		s.sendResponse(out, req.ID, res, nil)

	case "session/new":
		sessID := fmt.Sprintf("acp_sess_%d", s.sessionSeq.Add(1))
		s.mu.Lock()
		s.activeSession = sessID
		s.mu.Unlock()

		s.sendResponse(out, req.ID, map[string]string{
			"sessionId": sessID,
			"status":    "created",
		}, nil)

	case "session/prompt":
		var params struct {
			SessionID string `json:"sessionId"`
			Prompt    string `json:"prompt"`
		}
		_ = json.Unmarshal(req.Params, &params)

		s.mu.Lock()
		s.activeSession = params.SessionID
		s.mu.Unlock()

		s.cmdChan <- protocol.EngineCommand{
			Type:   protocol.CmdSubmitPrompt,
			Prompt: params.Prompt,
		}

		s.sendResponse(out, req.ID, map[string]string{
			"status": "accepted",
		}, nil)

	case "session/cancel":
		s.cmdChan <- protocol.EngineCommand{
			Type: protocol.CmdCancelTurn,
		}
		s.sendResponse(out, req.ID, map[string]string{
			"status": "cancelled",
		}, nil)

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

func (s *ACPServer) sendResponse(out io.Writer, id any, result any, rpcErr *RPCError) {
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
