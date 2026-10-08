// Command fakeserver is a minimal stdio JSON-RPC MCP
// server used by tests. It answers initialize,
// tools/list, and tools/call.
package main

import (
	"bufio"
	"encoding/json"
	"os"
	"strings"
)

type request struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

func main() {
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(make([]byte, 1024*1024), 1024*1024)
	enc := json.NewEncoder(os.Stdout)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}
		var req request
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			continue
		}
		switch req.Method {
		case "initialize":
			_ = enc.Encode(map[string]any{
				"jsonrpc": "2.0",
				"id":      json.RawMessage(req.ID),
				"result": map[string]any{
					"protocolVersion": "2024-11-05",
					"capabilities":    map[string]any{},
					"serverInfo":      map[string]any{"name": "fake", "version": "0.0.1"},
				},
			})
		case "tools/list":
			_ = enc.Encode(map[string]any{
				"jsonrpc": "2.0",
				"id":      json.RawMessage(req.ID),
				"result": map[string]any{
					"tools": []map[string]string{
						{"name": "echo", "description": "echo input"},
						{"name": "sum", "description": "sum two ints"},
					},
				},
			})
		case "tools/call":
			var params struct {
				Name      string         `json:"name"`
				Arguments map[string]any `json:"arguments"`
			}
			_ = json.Unmarshal(req.Params, &params)
			result := map[string]any{"content": []map[string]any{{"type": "text", "text": "ok"}}}
			if params.Name == "echo" {
				if msg, ok := params.Arguments["message"].(string); ok {
					result["content"] = []map[string]any{{"type": "text", "text": msg}}
				}
			}
			_ = enc.Encode(map[string]any{
				"jsonrpc": "2.0",
				"id":      json.RawMessage(req.ID),
				"result":  result,
			})
		default:
			_ = enc.Encode(map[string]any{
				"jsonrpc": "2.0",
				"id":      json.RawMessage(req.ID),
				"error":   map[string]any{"code": -32601, "message": "method not found: " + req.Method},
			})
		}
	}
}
