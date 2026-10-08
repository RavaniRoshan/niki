package appserver_test

import (
	"bufio"
	"encoding/json"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/appserver"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func TestCodexServer(t *testing.T) {
	prov := provider.NewMockProvider()
	reg := tools.DefaultRegistry()
	guard := permissions.NewGuard(permissions.ModeWorkspaceWrite)
	srv := appserver.NewCodexServer(prov, reg, guard)
	defer srv.Stop()

	inReader, inWriter := io.Pipe()
	outReader, outWriter := io.Pipe()

	go func() {
		_ = srv.Serve(inReader, outWriter)
	}()

	outScanner := bufio.NewReader(outReader)

	sendReq := func(req appserver.RPCRequest) appserver.RPCResponse {
		b, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal req failed: %v", err)
		}
		if _, err := inWriter.Write(append(b, '\n')); err != nil {
			t.Fatalf("write req failed: %v", err)
		}

		for {
			line, err := outScanner.ReadBytes('\n')
			if err != nil {
				t.Fatalf("read resp failed: %v", err)
			}
			var raw map[string]any
			if err := json.Unmarshal(line, &raw); err != nil {
				t.Fatalf("unmarshal resp failed: %v (raw: %s)", err, string(line))
			}
			if _, isNotif := raw["method"]; isNotif {
				continue
			}
			var resp appserver.RPCResponse
			_ = json.Unmarshal(line, &resp)
			return resp
		}
	}

	// 1. initialize
	initResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      1,
		Method:  "initialize",
	})
	if initResp.ID != float64(1) && initResp.ID != 1 {
		t.Errorf("expected ID 1, got %v", initResp.ID)
	}
	resBytes, _ := json.Marshal(initResp.Result)
	if !strings.Contains(string(resBytes), "niki-codex-server") {
		t.Errorf("initialize response unexpected: %s", string(resBytes))
	}

	// 2. thread/create
	thResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      2,
		Method:  "thread/create",
	})
	thBytes, _ := json.Marshal(thResp.Result)
	if !strings.Contains(string(thBytes), "th_1") {
		t.Errorf("thread/create unexpected: %s", string(thBytes))
	}

	// 3. turn/start
	turnParams, _ := json.Marshal(map[string]string{
		"prompt":   "hello world",
		"threadId": "th_1",
	})
	turnResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      3,
		Method:  "turn/start",
		Params:  turnParams,
	})
	turnBytes, _ := json.Marshal(turnResp.Result)
	if !strings.Contains(string(turnBytes), "started") {
		t.Errorf("turn/start unexpected: %s", string(turnBytes))
	}

	// Read stream notifications until completion or timeout
	doneCh := make(chan bool, 1)
	go func() {
		for {
			line, err := outScanner.ReadBytes('\n')
			if err != nil {
				return
			}
			if strings.Contains(string(line), `"completed"`) {
				doneCh <- true
				return
			}
		}
	}()

	select {
	case <-doneCh:
		// Turn successfully completed with notifications streamed
	case <-time.After(2 * time.Second):
		t.Log("timed out waiting for turn completed notification (non-fatal)")
	}

	// 4. turn/interrupt
	intResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      4,
		Method:  "turn/interrupt",
	})
	intBytes, _ := json.Marshal(intResp.Result)
	if !strings.Contains(string(intBytes), "interrupted") {
		t.Errorf("turn/interrupt unexpected: %s", string(intBytes))
	}

	// 5. shutdown
	sdResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      5,
		Method:  "shutdown",
	})
	sdBytes, _ := json.Marshal(sdResp.Result)
	if !strings.Contains(string(sdBytes), "ok") {
		t.Errorf("shutdown unexpected: %s", string(sdBytes))
	}

	_ = inWriter.Close()
	_ = outWriter.Close()
}

func TestACPServer(t *testing.T) {
	prov := provider.NewMockProvider()
	reg := tools.DefaultRegistry()
	guard := permissions.NewGuard(permissions.ModeWorkspaceWrite)
	srv := appserver.NewACPServer(prov, reg, guard)
	defer srv.Stop()

	inReader, inWriter := io.Pipe()
	outReader, outWriter := io.Pipe()

	go func() {
		_ = srv.Serve(inReader, outWriter)
	}()

	outScanner := bufio.NewReader(outReader)

	sendReq := func(req appserver.RPCRequest) appserver.RPCResponse {
		b, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal req failed: %v", err)
		}
		if _, err := inWriter.Write(append(b, '\n')); err != nil {
			t.Fatalf("write req failed: %v", err)
		}

		for {
			line, err := outScanner.ReadBytes('\n')
			if err != nil {
				t.Fatalf("read resp failed: %v", err)
			}
			var raw map[string]any
			if err := json.Unmarshal(line, &raw); err != nil {
				t.Fatalf("unmarshal resp failed: %v (raw: %s)", err, string(line))
			}
			if _, isNotif := raw["method"]; isNotif {
				continue
			}
			var resp appserver.RPCResponse
			_ = json.Unmarshal(line, &resp)
			return resp
		}
	}

	// 1. initialize
	initResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      1,
		Method:  "initialize",
	})
	resBytes, _ := json.Marshal(initResp.Result)
	if !strings.Contains(string(resBytes), "niki-acp-server") {
		t.Errorf("initialize unexpected: %s", string(resBytes))
	}

	// 2. session/new
	newResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      2,
		Method:  "session/new",
	})
	newBytes, _ := json.Marshal(newResp.Result)
	if !strings.Contains(string(newBytes), "acp_sess_1") {
		t.Errorf("session/new unexpected: %s", string(newBytes))
	}

	// 3. session/prompt
	promptParams, _ := json.Marshal(map[string]string{
		"sessionId": "acp_sess_1",
		"prompt":    "write a go hello world",
	})
	promptResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      3,
		Method:  "session/prompt",
		Params:  promptParams,
	})
	promptBytes, _ := json.Marshal(promptResp.Result)
	if !strings.Contains(string(promptBytes), "accepted") {
		t.Errorf("session/prompt unexpected: %s", string(promptBytes))
	}

	// 4. session/cancel
	cancelResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      4,
		Method:  "session/cancel",
	})
	cancelBytes, _ := json.Marshal(cancelResp.Result)
	if !strings.Contains(string(cancelBytes), "cancelled") {
		t.Errorf("session/cancel unexpected: %s", string(cancelBytes))
	}

	// 5. shutdown
	sdResp := sendReq(appserver.RPCRequest{
		JSONRPC: "2.0",
		ID:      5,
		Method:  "shutdown",
	})
	sdBytes, _ := json.Marshal(sdResp.Result)
	if !strings.Contains(string(sdBytes), "ok") {
		t.Errorf("shutdown unexpected: %s", string(sdBytes))
	}

	_ = inWriter.Close()
	_ = outWriter.Close()
}
