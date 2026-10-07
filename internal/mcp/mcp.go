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

	mu     sync.Mutex
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Reader
	pending map[int64]chan *Response
	nextID atomic.Int64
	state   State
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
func (c *Client) Start(ctx context.Context) error {
	c.cmd = exec.CommandContext(ctx, c.Command, c.Args...)
	var err error
	c.stdin, err = c.cmd.StdinPipe()
	if err != nil {
		c.state = StateFailed
		return err
	}
	stdout, err := c.cmd.StdoutPipe()
	if err != nil {
		c.state = StateFailed
		return err
	}
	c.stdout = bufio.NewReader(stdout)
	if err := c.cmd.Start(); err != nil {
		c.state = StateFailed
		return err
	}
	go c.readLoop()
	handshake := Request{JSONRPC: "2.0", ID: c.nextID.Add(1), Method: "initialize", Params: map[string]any{
		"protocolVersion": "2024-11-05",
		"capabilities":    map[string]any{},
		"clientInfo":      map[string]any{"name": "niki", "version": "0.1.0"},
	}}
	resp, err := c.call(ctx, handshake)
	if err != nil {
		c.state = StateFailed
		return err
	}
	_ = resp
	c.state = StateReady
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

// Stop terminates the server process.
func (c *Client) Stop() error {
	if c.cmd != nil && c.cmd.Process != nil {
		return c.cmd.Process.Kill()
	}
	return nil
}

// StateOf returns the server state.
func (c *Client) StateOf() State { return c.state }

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
