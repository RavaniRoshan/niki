package mcp_test

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/mcp"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func TestMCPServer(t *testing.T) {
	reg := tools.DefaultRegistry()
	srv := mcp.NewServer(reg)

	inReader, inWriter := io.Pipe()
	outReader, outWriter := io.Pipe()

	serverErr := make(chan error, 1)
	go func() {
		serverErr <- srv.Serve(inReader, outWriter)
	}()

	outScanner := bufio.NewReader(outReader)

	sendReq := func(req mcp.Request) mcp.Response {
		b, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal req failed: %v", err)
		}
		if _, err := inWriter.Write(append(b, '\n')); err != nil {
			t.Fatalf("write req failed: %v", err)
		}

		line, err := outScanner.ReadBytes('\n')
		if err != nil {
			t.Fatalf("read resp failed: %v", err)
		}
		var resp mcp.Response
		if err := json.Unmarshal(line, &resp); err != nil {
			t.Fatalf("unmarshal resp failed: %v (raw: %s)", err, string(line))
		}
		return resp
	}

	// 1. initialize
	initResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      1,
		Method:  "initialize",
	})
	if initResp.ID != 1 {
		t.Errorf("expected ID 1, got %d", initResp.ID)
	}
	if !strings.Contains(string(initResp.Result), `"protocolVersion":"2024-11-05"`) {
		t.Errorf("initialize result missing protocolVersion: %s", string(initResp.Result))
	}

	// 2. tools/list
	listResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      2,
		Method:  "tools/list",
	})
	if listResp.ID != 2 {
		t.Errorf("expected ID 2, got %d", listResp.ID)
	}
	if !strings.Contains(string(listResp.Result), `"read_file"`) {
		t.Errorf("tools/list missing read_file: %s", string(listResp.Result))
	}

	// 3. tools/call
	callResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      3,
		Method:  "tools/call",
		Params: map[string]any{
			"name": "read_file",
			"arguments": map[string]any{
				"path": "server_test.go",
			},
		},
	})
	if callResp.ID != 3 {
		t.Errorf("expected ID 3, got %d", callResp.ID)
	}
	if !strings.Contains(string(callResp.Result), "package mcp_test") {
		t.Errorf("tools/call output missing test file content: %s", string(callResp.Result))
	}

	// 4. resources/list and resources/read
	resListResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      4,
		Method:  "resources/list",
	})
	if !strings.Contains(string(resListResp.Result), "niki://config") {
		t.Errorf("resources/list unexpected: %s", string(resListResp.Result))
	}

	resReadResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      5,
		Method:  "resources/read",
		Params:  map[string]any{"uri": "niki://config"},
	})
	if !strings.Contains(string(resReadResp.Result), "niki configuration resource text") {
		t.Errorf("resources/read unexpected: %s", string(resReadResp.Result))
	}

	// 5. prompts/list and prompts/get
	pmtListResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      6,
		Method:  "prompts/list",
	})
	if !strings.Contains(string(pmtListResp.Result), "review_code") {
		t.Errorf("prompts/list unexpected: %s", string(pmtListResp.Result))
	}

	pmtGetResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      7,
		Method:  "prompts/get",
		Params:  map[string]any{"name": "review_code"},
	})
	if !strings.Contains(string(pmtGetResp.Result), "Please review the code changes") {
		t.Errorf("prompts/get unexpected: %s", string(pmtGetResp.Result))
	}

	// 6. Unknown method
	unknownResp := sendReq(mcp.Request{
		JSONRPC: "2.0",
		ID:      8,
		Method:  "nonexistent/method",
	})
	if unknownResp.Error == nil || unknownResp.Error.Code != -32601 {
		t.Errorf("expected -32601 method not found, got %+v", unknownResp.Error)
	}

	_ = inWriter.Close()
	_ = outWriter.Close()
}

func TestMCPParseError(t *testing.T) {
	srv := mcp.NewServer(nil)
	in := bytes.NewBufferString("invalid json line\n")
	var out bytes.Buffer

	_ = srv.Serve(in, &out)

	var resp mcp.Response
	if err := json.Unmarshal(out.Bytes(), &resp); err != nil {
		t.Fatalf("failed to parse response: %v", err)
	}
	if resp.Error == nil || resp.Error.Code != -32700 {
		t.Errorf("expected -32700 parse error, got %+v", resp.Error)
	}
}

func TestMCPClientReconnect(t *testing.T) {
	client := mcp.NewClient("fake", "nonexistent-cmd-xyz")
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()

	err := client.Reconnect(ctx, 2, 10*time.Millisecond)
	if err == nil {
		t.Fatal("expected error attempting to reconnect to nonexistent binary, got nil")
	}
}
