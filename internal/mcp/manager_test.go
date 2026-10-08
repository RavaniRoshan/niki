package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// fakeServerCmd returns a command that runs the
// testdata fake MCP server. The binary is built
// once per test run (TestMain) so server startup
// is fast and deterministic.
func fakeServerCmd() *exec.Cmd {
	return exec.Command(fakeServerBin)
}

var fakeServerBin string

func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "nikicode-mcp-bin")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fakeServerBin = filepath.Join(dir, "fakeserver")
	build := exec.Command("go", "build", "-o", fakeServerBin, "testdata/fakeserver/main.go")
	build.Stderr = os.Stderr
	if err := build.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "building fake server: %v\n", err)
		os.Exit(1)
	}
	code := m.Run()
	_ = os.RemoveAll(dir)
	os.Exit(code)
}

// startFakeManager builds a Manager over one fake
// server and starts it eagerly.
func startFakeManager(t *testing.T, required bool) (*Manager, *[]protocol.EngineEvent, *sync.Mutex) {
	t.Helper()
	dir, err := os.MkdirTemp("", "nikicode-mcp")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(dir) })

	// Events are appended from the manager's own
	// goroutines (optional servers start in the
	// background), so the slice is mutex-guarded.
	var events []protocol.EngineEvent
	var evMu sync.Mutex
	cache := NewCatalogCache(filepath.Join(dir, "catalog.json"))
	m := NewManager(cache, func(evt protocol.EngineEvent) {
		evMu.Lock()
		events = append(events, evt)
		evMu.Unlock()
	})
	cmd := fakeServerCmd()
	m.Add(ServerConfig{
		Name:     "fake server",
		Command:  cmd.Path,
		Args:     cmd.Args[1:],
		Required: required,
	})
	return m, &events, &evMu
}

// TestManagerEagerRequired (M1/M4): a required
// server starts eagerly, reports ready with a
// sanitized qualified name, and its tools are
// routable.
func TestManagerEagerRequired(t *testing.T) {
	m, events, evMu := startFakeManager(t, true)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	m.StartRequired(ctx)
	defer m.Stop()

	statuses := m.Statuses()
	if len(statuses) != 1 {
		t.Fatalf("statuses=%v", statuses)
	}
	st := statuses[0]
	if st.State != StateReady {
		t.Fatalf("required server not ready: %+v", st)
	}
	if st.Qualified != "fake_server" {
		t.Fatalf("qualified name not sanitized: %q", st.Qualified)
	}
	if len(st.Tools) != 2 {
		t.Fatalf("tools=%v", st.Tools)
	}

	// Truthful status events were emitted in order.
	evMu.Lock()
	var seq []protocol.EventType
	for _, e := range *events {
		if e.Type == protocol.EventMcpServerStarting ||
			e.Type == protocol.EventMcpServerReady ||
			e.Type == protocol.EventMcpServerFailed {
			seq = append(seq, e.Type)
		}
	}
	evMu.Unlock()
	if len(seq) != 2 || seq[0] != protocol.EventMcpServerStarting || seq[1] != protocol.EventMcpServerReady {
		t.Fatalf("status sequence=%v", seq)
	}

	// Qualified routing works end to end.
	out, err := m.CallTool(ctx, "fake_server_echo", json.RawMessage(`{"message":"hi <b>"}`))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out, "hi") {
		t.Fatalf("echo result=%s", out)
	}
}

// TestManagerOutputSanitized (M5): MCP output is
// untrusted; control characters in a tool result
// are stripped before the text reaches the caller.
func TestManagerOutputSanitized(t *testing.T) {
	m, _, _ := startFakeManager(t, true)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	m.StartRequired(ctx)
	defer m.Stop()

	// The fake server echoes the message verbatim.
	// json.Marshal escapes the control characters
	// for transport; the manager decodes them back
	// out of the envelope and must strip them.
	args, err := json.Marshal(map[string]string{"message": "line\x01bad\x1b[0m"})
	if err != nil {
		t.Fatal(err)
	}
	out, err := m.CallTool(ctx, "fake_server_echo", args)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(out, "\x01") || strings.Contains(out, "\x1b") {
		t.Fatalf("unsanitized MCP output reached caller: %q", out)
	}
	if !strings.Contains(out, "line") || !strings.Contains(out, "bad") {
		t.Fatalf("legitimate text damaged: %q", out)
	}
}

// TestManagerOptionalLazyWhenCached (M2): an
// optional server with a persisted catalog serves
// the cached tools immediately (no connection
// wait) and connects in the background.
func TestManagerOptionalLazyWhenCached(t *testing.T) {
	m, events, evMu := startFakeManager(t, false)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	// Seed a persisted catalog so the optional
	// server can serve before connecting.
	cache := m.cache
	if err := cache.Save([]string{"echo", "sum"}); err != nil {
		t.Fatal(err)
	}

	m.StartOptional(ctx, "fake server")
	defer m.Stop()

	// Cached catalog is served immediately, without
	// waiting for the connection.
	statuses := m.Statuses()
	if len(statuses) != 1 {
		t.Fatalf("statuses=%v", statuses)
	}
	if len(statuses[0].Tools) != 2 {
		t.Fatalf("cached tools not served: %v", statuses[0].Tools)
	}
	if statuses[0].State != StateReady && statuses[0].State != StateStarting {
		t.Fatalf("unexpected state %s", statuses[0].State)
	}

	// Wait for the background connection to finish.
	deadline := time.After(30 * time.Second)
	for m.Statuses()[0].State != StateReady {
		select {
		case <-deadline:
			t.Fatalf("optional server never connected; status=%+v", m.Statuses()[0])
		case <-time.After(50 * time.Millisecond):
		}
	}

	// The catalog cache was refreshed from the live
	// server's tools/list.
	tools, ok := m.cache.Load()
	if !ok || len(tools) != 2 {
		t.Fatalf("cache not refreshed: %v ok=%v", tools, ok)
	}

	// Lifecycle events were emitted truthfully.
	evMu.Lock()
	var sawStarting, sawReady bool
	for _, e := range *events {
		switch e.Type {
		case protocol.EventMcpServerStarting:
			sawStarting = true
		case protocol.EventMcpServerReady:
			sawReady = true
		case protocol.EventMcpServerFailed:
			evMu.Unlock()
			t.Fatalf("optional server failed unexpectedly: %s", e.Error)
		}
	}
	evMu.Unlock()
	if !sawStarting || !sawReady {
		t.Fatalf("starting=%v ready=%v", sawStarting, sawReady)
	}
}

// TestManagerFailedRequiredSurfaced (M4): a
// required server that cannot start is reported as
// failed with its error, not hidden.
func TestManagerFailedRequiredSurfaced(t *testing.T) {
	var events []protocol.EngineEvent
	m := NewManager(nil, func(evt protocol.EngineEvent) {
		events = append(events, evt)
	})
	m.Add(ServerConfig{
		Name:     "broken",
		Command:  "/nonexistent/nikicode-mcp-binary",
		Required: true,
	})
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	m.StartRequired(ctx)

	st := m.Statuses()[0]
	if st.State != StateFailed {
		t.Fatalf("broken required server state=%s", st.State)
	}
	if st.Error == "" {
		t.Fatal("failed required server must surface its error")
	}
	var sawFailed bool
	for _, e := range events {
		if e.Type == protocol.EventMcpServerFailed {
			sawFailed = true
			if e.Error == "" {
				t.Fatal("failure event carried no error")
			}
		}
	}
	if !sawFailed {
		t.Fatal("no failure event emitted for broken required server")
	}
}

// TestManagerFailClosedRouting (M1): a tool no
// server owns is refused rather than guessed.
func TestManagerFailClosedRouting(t *testing.T) {
	m, _, _ := startFakeManager(t, true)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	m.StartRequired(ctx)
	defer m.Stop()

	if _, err := m.CallTool(ctx, "nobody_else_echo", nil); err == nil {
		t.Fatal("expected fail-closed routing error")
	}
}

// TestQualifySanitizesNames (M1): raw names with
// hostile characters map to safe qualified names.
func TestQualifySanitizesNames(t *testing.T) {
	cases := map[string]string{
		"GitHub CLI":     "github_cli",
		"files./\\*:bad": "files_____bad",
		"Ünïcode Server": "n_code_server",
		"":               "server",
		"!!!":            "server",
		"a":              "a",
	}
	for raw, want := range cases {
		if got := Qualify(raw); got != want {
			t.Fatalf("Qualify(%q)=%q want %q", raw, got, want)
		}
	}
}

// TestManagerOneClientPerServer (M1): each added
// server owns exactly one client instance.
func TestManagerOneClientPerServer(t *testing.T) {
	m := NewManager(nil, func(protocol.EngineEvent) {})
	m.Add(ServerConfig{Name: "one", Command: "true"})
	m.Add(ServerConfig{Name: "two", Command: "true"})
	m.Add(ServerConfig{Name: "One", Command: "true"}) // qualifies to a collision with "one"

	m.mu.RLock()
	defer m.mu.RUnlock()
	if len(m.servers) != 3 {
		t.Fatalf("expected 3 entries, got %d", len(m.servers))
	}
	clients := map[*Client]bool{}
	for _, e := range m.servers {
		if clients[e.client] {
			t.Fatal("client shared between servers")
		}
		clients[e.client] = true
	}
	// Collision produced a distinct qualified name.
	qualifs := map[string]bool{}
	for _, e := range m.servers {
		if qualifs[e.qualified] {
			t.Fatalf("qualified name collision: %q", e.qualified)
		}
		qualifs[e.qualified] = true
	}
}
