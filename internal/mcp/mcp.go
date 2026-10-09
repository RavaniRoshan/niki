package mcp

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sync"
	"sync/atomic"
)

// Client is a stdio JSON-RPC MCP client for one server.
type Client struct {
	Name    string
	Command string
	Args    []string

	mu      sync.Mutex
	cmd     *exec.Cmd
	stdin   io.WriteCloser
	stdout  *bufio.Reader
	pending map[int64]chan *Response
	nextID  atomic.Int64
	state   State
}

// setState records a state transition. The state
// is read from other goroutines (status polls), so
// every transition is guarded by mu.
func (c *Client) setState(s State) {
	c.mu.Lock()
	c.state = s
	c.mu.Unlock()
}

type State string

const (
	StateStarting State = "starting"
	StateReady    State = "ready"
	StateFailed   State = "failed"
)

type Request struct {
	JSONRPC string `json:"jsonrpc"`
	ID      int64  `json:"id"`
	Method  string `json:"method"`
	Params  any    `json:"params,omitempty"`
}

type Response struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      int64           `json:"id"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *struct {
		Code    int    `json:"code"`
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

func NewClient(name, command string, args ...string) *Client {
	return &Client{Name: name, Command: command, Args: args, pending: map[int64]chan *Response{}, state: StateStarting}
}

// Start spawns the server and runs the initialize handshake.
// The server process lives until Stop(): the ctx passed
// here bounds only the handshake, so callers may cancel
// it once Start returns without killing the server.
func (c *Client) Start(ctx context.Context) error {
	c.mu.Lock()
	c.cmd = exec.CommandContext(context.Background(), c.Command, c.Args...)
	var err error
	c.stdin, err = c.cmd.StdinPipe()
	if err != nil {
		c.state = StateFailed
		c.mu.Unlock()
		return err
	}
	stdout, err := c.cmd.StdoutPipe()
	if err != nil {
		c.state = StateFailed
		c.mu.Unlock()
		return err
	}
	c.stdout = bufio.NewReader(stdout)
	c.mu.Unlock()
	if err := c.cmd.Start(); err != nil {
		c.setState(StateFailed)
		return err
	}
	go c.readLoop()
	handshake := Request{JSONRPC: "2.0", ID: c.nextID.Add(1), Method: "initialize", Params: map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities":    map[string]any{},
		"clientInfo":      map[string]any{"name": "nikicode", "version": "0.1.0"},
	}}
	resp, err := c.call(ctx, handshake)
	if err != nil {
		c.setState(StateFailed)
		// The handshake failed: the server is unusable,
		// so reap it now rather than leak the process.
		c.mu.Lock()
		process := c.cmd.Process
		c.mu.Unlock()
		if process != nil {
			_ = process.Kill()
		}
		return err
	}
	_ = resp
	c.setState(StateReady)
	return nil
}

func (c *Client) readLoop() {
	for {
		line, err := c.stdout.ReadBytes('\n')
		if err != nil {
			return
		}
		var resp Response
		if err := json.Unmarshal(line, &resp); err != nil {
			continue
		}
		c.mu.Lock()
		ch, ok := c.pending[resp.ID]
		c.mu.Unlock()
		if ok {
			ch <- &resp
		}
	}
}

func (c *Client) call(ctx context.Context, req Request) (*Response, error) {
	ch := make(chan *Response, 1)
	c.mu.Lock()
	c.pending[req.ID] = ch
	c.mu.Unlock()
	defer func() {
		c.mu.Lock()
		delete(c.pending, req.ID)
		c.mu.Unlock()
	}()
	data, _ := json.Marshal(req)
	if _, err := c.stdin.Write(append(data, '\n')); err != nil {
		return nil, err
	}
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case resp := <-ch:
		if resp.Error != nil {
			return nil, fmt.Errorf("mcp error %d: %s", resp.Error.Code, resp.Error.Message)
		}
		return resp, nil
	}
}

// Call invokes a method on the server.
func (c *Client) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	resp, err := c.call(ctx, Request{JSONRPC: "2.0", ID: c.nextID.Add(1), Method: method, Params: params})
	if err != nil {
		return nil, err
	}
	return resp.Result, nil
}

// ListTools fetches the server's tool names via
// tools/list.
func (c *Client) ListTools(ctx context.Context) ([]string, error) {
	raw, err := c.Call(ctx, "tools/list", map[string]any{})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Tools []struct {
			Name string `json:"name"`
		} `json:"tools"`
	}
	if err := json.Unmarshal(raw, &payload); err != nil {
		return nil, err
	}
	names := make([]string, 0, len(payload.Tools))
	for _, t := range payload.Tools {
		names = append(names, t.Name)
	}
	return names, nil
}

// CallTool invokes a tool on the server and
// returns the raw JSON result.
func (c *Client) CallTool(ctx context.Context, name string, args json.RawMessage) (json.RawMessage, error) {
	if args == nil {
		args = json.RawMessage("{}")
	}
	return c.Call(ctx, "tools/call", map[string]any{"name": name, "arguments": json.RawMessage(args)})
}

// Stop terminates the server process.
func (c *Client) Stop() error {
	c.mu.Lock()
	cmd := c.cmd
	c.mu.Unlock()
	if cmd != nil && cmd.Process != nil {
		return cmd.Process.Kill()
	}
	return nil
}

// StateOf returns the server state.
func (c *Client) StateOf() State {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.state
}

// CatalogCache persists a server's tool catalog so a cached catalog is usable
// before the connection opens (lazy-when-cached startup).
type CatalogCache struct {
	Path string
}

type cachedCatalog struct {
	Tools []string `json:"tools"`
}

func NewCatalogCache(path string) *CatalogCache { return &CatalogCache{Path: path} }

func (c *CatalogCache) Save(tools []string) error {
	data, _ := json.Marshal(cachedCatalog{Tools: tools})
	return os.WriteFile(c.Path, data, 0o600)
}

func (c *CatalogCache) Load() ([]string, bool) {
	data, err := os.ReadFile(c.Path)
	if err != nil {
		return nil, false
	}
	var cc cachedCatalog
	if err := json.Unmarshal(data, &cc); err != nil {
		return nil, false
	}
	return cc.Tools, len(cc.Tools) > 0
}
