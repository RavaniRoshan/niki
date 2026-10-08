package mcp

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"sync"

	"github.com/RavaniRoshan/niki/internal/tools"
)

type Server struct {
	mu       sync.Mutex
	registry *tools.Registry
	done     chan struct{}
}

func NewServer(reg *tools.Registry) *Server {
	if reg == nil {
		reg = tools.DefaultRegistry()
	}
	return &Server{
		registry: reg,
		done:     make(chan struct{}),
	}
}

// Serve reads JSON-RPC requests line by line from in and writes JSON-RPC responses to out.
func (s *Server) Serve(in io.Reader, out io.Writer) error {
	sc := bufio.NewScanner(in)
	// Support up to 4MB messages
	buf := make([]byte, 1024*1024)
	sc.Buffer(buf, 4*1024*1024)

	for sc.Scan() {
		line := sc.Bytes()
		if len(line) == 0 {
			continue
		}

		var req Request
		if err := json.Unmarshal(line, &req); err != nil {
			_ = s.sendError(out, 0, -32700, "Parse error")
			continue
		}

		resp := s.handleRequest(req)
		if resp != nil {
			respBytes, err := json.Marshal(resp)
			if err == nil {
				s.mu.Lock()
				_, _ = out.Write(append(respBytes, '\n'))
				s.mu.Unlock()
			}
		}
	}
	return sc.Err()
}

func (s *Server) sendError(out io.Writer, id int64, code int, msg string) error {
	resp := Response{
		JSONRPC: "2.0",
		ID:      id,
		Error: &struct {
			Code    int    `json:"code"`
			Message string `json:"message"`
		}{
			Code:    code,
			Message: msg,
		},
	}
	b, _ := json.Marshal(resp)
	s.mu.Lock()
	defer s.mu.Unlock()
	_, err := out.Write(append(b, '\n'))
	return err
}

func (s *Server) handleRequest(req Request) *Response {
	// Notifications (no ID) require no response
	if req.ID == 0 && req.Method == "notifications/initialized" {
		return nil
	}

	switch req.Method {
	case "initialize":
		result, _ := json.Marshal(map[string]any{
			"protocolVersion": "2024-11-05",
			"capabilities": map[string]any{
				"tools":     map[string]any{},
				"resources": map[string]any{},
				"prompts":   map[string]any{},
			},
			"serverInfo": map[string]any{
				"name":    "nikicode",
				"version": "0.1.0",
			},
		})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	case "tools/list":
		allTools := s.registry.List()
		var toolDefs []map[string]any
		for _, t := range allTools {
			var schemaObj map[string]any
			if t.Schema() != "" {
				_ = json.Unmarshal([]byte(t.Schema()), &schemaObj)
			}
			toolDefs = append(toolDefs, map[string]any{
				"name":        t.Name(),
				"description": t.Description(),
				"inputSchema": schemaObj,
			})
		}
		result, _ := json.Marshal(map[string]any{"tools": toolDefs})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	case "tools/call":
		var params struct {
			Name      string          `json:"name"`
			Arguments json.RawMessage `json:"arguments"`
		}
		rawParams, _ := json.Marshal(req.Params)
		_ = json.Unmarshal(rawParams, &params)

		tResult, err := s.registry.Run(context.Background(), params.Name, params.Arguments)
		isError := false
		outText := tResult.Output
		if err != nil {
			isError = true
			outText = err.Error()
		} else if tResult.IsError {
			isError = true
		}

		result, _ := json.Marshal(map[string]any{
			"content": []map[string]any{
				{
					"type": "text",
					"text": outText,
				},
			},
			"isError": isError,
		})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	case "resources/list":
		result, _ := json.Marshal(map[string]any{
			"resources": []map[string]any{
				{
					"uri":         "nikicode://config",
					"name":        "NikiCode configuration",
					"description": "Current effective configuration of the NikiCode harness",
					"mimeType":    "text/plain",
				},
			},
		})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	case "resources/read":
		var params struct {
			URI string `json:"uri"`
		}
		rawParams, _ := json.Marshal(req.Params)
		_ = json.Unmarshal(rawParams, &params)
		result, _ := json.Marshal(map[string]any{
			"contents": []map[string]any{
				{
					"uri":  params.URI,
					"text": "nikicode configuration resource text",
				},
			},
		})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	case "prompts/list":
		result, _ := json.Marshal(map[string]any{
			"prompts": []map[string]any{
				{
					"name":        "review_code",
					"description": "Perform code review and style check",
				},
			},
		})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	case "prompts/get":
		var params struct {
			Name      string            `json:"name"`
			Arguments map[string]string `json:"arguments"`
		}
		rawParams, _ := json.Marshal(req.Params)
		_ = json.Unmarshal(rawParams, &params)
		result, _ := json.Marshal(map[string]any{
			"description": "Code review prompt",
			"messages": []map[string]any{
				{
					"role": "user",
					"content": map[string]any{
						"type": "text",
						"text": "Please review the code changes thoroughly.",
					},
				},
			},
		})
		return &Response{JSONRPC: "2.0", ID: req.ID, Result: result}

	default:
		return &Response{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error: &struct {
				Code    int    `json:"code"`
				Message string `json:"message"`
			}{
				Code:    -32601,
				Message: fmt.Sprintf("Method not found: %s", req.Method),
			},
		}
	}
}
