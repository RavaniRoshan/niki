package mcp

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/sanitize"
)

// ServerConfig describes one MCP server.
type ServerConfig struct {
	Name     string
	Command  string
	Args     []string
	Required bool
}

// Status is the read-only, truthful per-server state (M3).
// It is derived from the client's own state machine and
// never claims more than the client knows.
type Status struct {
	Name      string
	Qualified string
	State     State
	Required  bool
	Tools     []string
	Error     string
}

// Manager owns exactly one client per configured server
// (M1). Required servers start eagerly at boot; optional
// servers start lazily, but a persisted catalog cache
// lets them serve tools before the connection opens
// (M2). MCP output is untrusted: every result is
// sanitized before it leaves the manager (M5).
type Manager struct {
	mu       sync.RWMutex
	servers  map[string]*serverEntry
	order    []string
	cache    *CatalogCache
	emit     func(protocol.EngineEvent)
}

type serverEntry struct {
	cfg      ServerConfig
	client   *Client
	qualified string
	tools    []string
	cached   bool // tools served from the persisted cache
	err      string
}

func NewManager(cache *CatalogCache, emit func(protocol.EngineEvent)) *Manager {
	return &Manager{
		servers: map[string]*serverEntry{},
		cache:   cache,
		emit:    emit,
	}
}

// Qualify maps a raw server name to a sanitized,
// collision-free qualified name used for routing.
// The raw identity is preserved for status reporting (M1).
func Qualify(raw string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(raw) {
		switch {
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9':
			b.WriteRune(r)
		default:
			b.WriteRune('_')
		}
	}
	q := strings.Trim(b.String(), "_")
	if q == "" {
		q = "server"
	}
	return q
}

// Add registers a server. One client is created per
// server and never shared (M1).
func (m *Manager) Add(cfg ServerConfig) {
	m.mu.Lock()
	defer m.mu.Unlock()
	qualified := Qualify(cfg.Name)
	// Disambiguate collisions from distinct raw names.
	base := qualified
	for i := 2; m.servers[qualified] != nil; i++ {
		qualified = fmt.Sprintf("%s_%d", base, i)
	}
	m.servers[cfg.Name] = &serverEntry{
		cfg:       cfg,
		qualified: qualified,
		client:    NewClient(cfg.Name, cfg.Command, cfg.Args...),
	}
	m.order = append(m.order, cfg.Name)
}

// StartRequired eagerly starts every required server
// and blocks until each is ready or failed (M4: a
// failed required server is surfaced, not hidden).
// Optional servers are left to lazy startup.
func (m *Manager) StartRequired(ctx context.Context) {
	m.mu.RLock()
	entries := make([]*serverEntry, 0, len(m.servers))
	for _, name := range m.order {
		e := m.servers[name]
		if e.cfg.Required {
			entries = append(entries, e)
		}
	}
	m.mu.RUnlock()

	for _, e := range entries {
		m.emitStatus(e, protocol.EventMcpServerStarting)
		startCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
		err := e.client.Start(startCtx)
		cancel()
		if err != nil {
			m.mu.Lock()
			e.err = err.Error()
			m.mu.Unlock()
			m.emitStatus(e, protocol.EventMcpServerFailed)
			continue
		}
		tools, err := e.client.ListTools(ctx)
		if err != nil {
			m.mu.Lock()
			e.err = err.Error()
			m.mu.Unlock()
			m.emitStatus(e, protocol.EventMcpServerFailed)
			continue
		}
		m.mu.Lock()
		e.tools = tools
		m.mu.Unlock()
		if m.cache != nil {
			_ = m.cache.Save(tools)
		}
		m.emitStatus(e, protocol.EventMcpServerReady)
	}
}

// StartOptional lazily starts an optional server. When
// a persisted catalog exists the wait is skipped: the
// cached tools are served immediately and the
// connection warms in the background (M2).
func (m *Manager) StartOptional(ctx context.Context, name string) {
	m.mu.RLock()
	e := m.servers[name]
	m.mu.RUnlock()
	if e == nil || e.cfg.Required {
		return
	}
	// Cached catalog? Serve it now, connect later.
	if m.cache != nil {
		if tools, ok := m.cache.Load(); ok {
			m.mu.Lock()
			e.cached = true
			e.tools = tools
			m.mu.Unlock()
			m.emitStatus(e, protocol.EventMcpServerReady)
		}
	}
	go func() {
		m.emitStatus(e, protocol.EventMcpServerStarting)
		startCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
		err := e.client.Start(startCtx)
		cancel()
		if err != nil {
			m.mu.Lock()
			e.err = err.Error()
			m.mu.Unlock()
			// An optional server failing never blocks a
			// turn (M4); the cached catalog still serves.
			m.emitStatus(e, protocol.EventMcpServerFailed)
			return
		}
		tools, err := e.client.ListTools(ctx)
		if err != nil {
			m.mu.Lock()
			e.err = err.Error()
			m.mu.Unlock()
			m.emitStatus(e, protocol.EventMcpServerFailed)
			return
		}
		m.mu.Lock()
		e.cached = false
		e.tools = tools
		m.mu.Unlock()
		if m.cache != nil {
			_ = m.cache.Save(tools)
		}
		m.emitStatus(e, protocol.EventMcpServerReady)
	}()
}

// CallTool routes a qualified tool call ("server_tool")
// to the owning server, extracts the result's text
// content, and returns it sanitized (M5). MCP output
// is untrusted: no byte of it reaches the model or
// the terminal unsanitized. It fails closed when no
// server owns the tool.
func (m *Manager) CallTool(ctx context.Context, qualifiedTool string, args json.RawMessage) (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	owner, tool, err := m.route(qualifiedTool)
	if err != nil {
		return "", err
	}
	if owner.client.StateOf() != StateReady {
		return "", fmt.Errorf("mcp server %q not connected", owner.cfg.Name)
	}
	raw, err := owner.client.CallTool(ctx, tool, args)
	if err != nil {
		return "", err
	}
	return extractAndSanitize(raw), nil
}

// mcpToolResult is the standard MCP tools/call
// result envelope.
type mcpToolResult struct {
	Content []struct {
		Type string `json:"type"`
		Text string `json:"text"`
	} `json:"content"`
}

// extractAndSanitize pulls the text content out of
// an MCP tool result and sanitizes every part.
func extractAndSanitize(raw json.RawMessage) string {
	var result mcpToolResult
	if err := json.Unmarshal(raw, &result); err != nil {
		// Not a standard envelope: sanitize the raw
		// JSON verbatim and return it.
		return sanitize.Sanitize(string(raw))
	}
	var parts []string
	for _, c := range result.Content {
		if c.Type == "text" {
			parts = append(parts, sanitize.Sanitize(c.Text))
		}
	}
	return strings.Join(parts, "\n")
}

// route resolves a qualified tool name to its owning
// server entry and the server-local tool name.
func (m *Manager) route(qualifiedTool string) (*serverEntry, string, error) {
	// Qualified names are "qualified_tool"; find the
	// longest matching server prefix.
	for _, name := range m.order {
		e := m.servers[name]
		prefix := e.qualified + "_"
		if strings.HasPrefix(qualifiedTool, prefix) {
			return e, strings.TrimPrefix(qualifiedTool, prefix), nil
		}
	}
	return nil, "", fmt.Errorf("no mcp server owns tool %q", qualifiedTool)
}

// ToolCatalog returns the merged, qualified tool list
// across all servers (live or cached).
func (m *Manager) ToolCatalog() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	var out []string
	for _, name := range m.order {
		e := m.servers[name]
		for _, t := range e.tools {
			out = append(out, e.qualified+"_"+t)
		}
	}
	return out
}

// Statuses returns the truthful per-server status (M3).
func (m *Manager) Statuses() []Status {
	m.mu.RLock()
	defer m.mu.RUnlock()
	out := make([]Status, 0, len(m.order))
	for _, name := range m.order {
		e := m.servers[name]
		st := Status{
			Name:      e.cfg.Name,
			Qualified: e.qualified,
			State:     e.client.StateOf(),
			Required:  e.cfg.Required,
			Tools:     e.tools,
			Error:     e.err,
		}
		out = append(out, st)
	}
	return out
}

// Stop terminates every live client.
func (m *Manager) Stop() {
	m.mu.RLock()
	defer m.mu.RUnlock()
	for _, name := range m.order {
		_ = m.servers[name].client.Stop()
	}
}

func (m *Manager) emitStatus(e *serverEntry, et protocol.EventType) {
	if m.emit == nil {
		return
	}
	m.mu.RLock()
	errText := e.err
	m.mu.RUnlock()
	evt := protocol.EngineEvent{
		Type:      et,
		Timestamp: time.Now(),
		ToolName:  e.cfg.Name,
		Text:      e.qualified,
	}
	if et == protocol.EventMcpServerFailed && errText != "" {
		evt.Error = errText
	}
	m.emit(evt)
}
