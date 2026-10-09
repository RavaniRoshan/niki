// Package soak runs the scripted all-day soak (G5): many mock-provider
// turns with real tool calls, subagent manager lifecycles, an MCP
// server, and a hook on every turn, sampling RSS/heap/goroutines to
// CSV. No model spend: the mock provider responds instantly, so the
// run measures harness stability (leaks, crashes), not intelligence.
package soak

import (
	"context"
	"encoding/csv"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"time"

	"github.com/RavaniRoshan/niki/internal/agent"
	"github.com/RavaniRoshan/niki/internal/bench"
	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/mcp"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/plugins"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
)

// Config tunes a soak run.
type Config struct {
	Turns   int    // engine turns to run
	MCPBin  string // fake MCP server binary ("" skips MCP)
	HookCmd string // hook command per turn ("" skips hooks)
	WorkDir string // tool-call working directory
	SampleN int    // sample every N turns (default 1)
	OutCSV  string // CSV output path
}

// Verdict summarizes stability.
type Verdict struct {
	Turns      int
	Crashes    int
	RSSStartMB float64
	RSSEndMB   float64
	RSSMaxMB   float64
	GrowthMB   float64
	HeapMB     float64
	Goroutines int
	CSV        string
}

// Run executes the soak. Any error aborts with the turn number.
func Run(cfg Config) (Verdict, error) {
	if cfg.Turns <= 0 {
		cfg.Turns = 100
	}
	if cfg.SampleN <= 0 {
		cfg.SampleN = 1
	}
	if err := os.MkdirAll(filepath.Dir(cfg.OutCSV), 0o755); err != nil {
		return Verdict{}, err
	}
	f, err := os.Create(cfg.OutCSV)
	if err != nil {
		return Verdict{}, err
	}
	defer f.Close()
	w := csv.NewWriter(f)
	defer w.Flush()
	_ = w.Write([]string{"turn", "rss_mb", "heap_mb", "goroutines", "elapsed_ms"})

	reg := tools.DefaultRegistry()
	guard := permissions.NewGuard(permissions.ModeFullAccess)
	prov := provider.NewMockProvider()
	eng, cmdChan, eventChan := engine.NewEngine(1000, prov, reg, guard)
	go func() { _ = eng.Run() }()
	defer eng.Stop()

	// Subagent manager lifecycle alongside the turns.
	agents := agent.NewManager(agent.NewMemoryGraphStore(), 3, 6, func(protocol.EngineEvent) {})

	// Optional MCP server (required=eager, proves startup path).
	var mgr *mcp.Manager
	if cfg.MCPBin != "" {
		mgr = mcp.NewManager(mcp.NewCatalogCache(filepath.Join(os.TempDir(), "soak-catalog.json")), func(protocol.EngineEvent) {})
		mgr.Add(mcp.ServerConfig{Name: "soak", Command: cfg.MCPBin, Required: true})
		mctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		mgr.StartRequired(mctx)
		cancel()
		defer mgr.Stop()
	}

	// Optional per-turn hook.
	hooks := plugins.NewManager()

	sample := func(turn int, t0 time.Time) {
		var ms runtime.MemStats
		runtime.ReadMemStats(&ms)
		_ = w.Write([]string{
			strconv.Itoa(turn),
			fmt.Sprintf("%.2f", bench.ProcRSS(os.Getpid())),
			fmt.Sprintf("%.2f", float64(ms.HeapAlloc)/1024/1024),
			strconv.Itoa(runtime.NumGoroutine()),
			fmt.Sprintf("%d", time.Since(t0).Milliseconds()),
		})
	}

	t0 := time.Now()
	crashes := 0
	var rssStart, rssEnd, rssMax float64
	ctx := context.Background()
	for i := 1; i <= cfg.Turns; i++ {
		// 1. Engine turn (mock model, instant).
		cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: fmt.Sprintf("soak turn %d", i)}
		deadline := time.After(30 * time.Second)
	turnLoop:
		for {
			select {
			case evt, ok := <-eventChan:
				if !ok {
					crashes++
					return Verdict{Crashes: crashes}, fmt.Errorf("event channel closed at turn %d", i)
				}
				if evt.Type == protocol.EventTurnCompleted {
					break turnLoop
				}
			case <-deadline:
				crashes++
				return Verdict{Crashes: crashes}, fmt.Errorf("turn %d timed out", i)
			}
		}
		// 2. Real tool calls every turn (the mock model makes
		// none, so the soak drives them directly and honestly).
		workArg := cfg.WorkDir
		if workArg == "" {
			workArg = "."
		}
		_, _ = reg.Run(ctx, "glob", json.RawMessage(`{"pattern":"*","root":"`+workArg+`"}`))
		_, _ = reg.Run(ctx, "read_file", json.RawMessage(`{"path":"`+filepath.Join(workArg, "soak.txt")+`"}`))
		_, _ = reg.Run(ctx, "shell", json.RawMessage(`{"command":"echo soak-turn","dir":"`+workArg+`"}`))
		// 3. Subagent lifecycle every 10 turns.
		if i%10 == 0 {
			sctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
			id, _, err := agents.Spawn(sctx, "root", fmt.Sprintf("soak-%d", i), "summarize", "none", false, 0)
			if err == nil {
				_, _ = agents.SendInput(sctx, id, "more")
				_, _, _, _ = agents.Wait(sctx, id, 10*time.Second)
				_ = agents.Close(sctx, id)
			}
			cancel()
		}
		// 4. MCP tool call every 25 turns.
		if mgr != nil && i%25 == 0 {
			mctx, cancel := context.WithTimeout(ctx, 10*time.Second)
			_, _ = mgr.CallTool(mctx, "mcp__soak__echo", json.RawMessage(`{"text":"soak"}`))
			cancel()
		}
		// 5. Hook every turn.
		if cfg.HookCmd != "" {
			_, _ = hooks.ExecuteHook(plugins.HookConfig{Point: "post_tool_use", Command: cfg.HookCmd, Timeout: 5}, cfg.WorkDir, []byte(`{"turn":`+strconv.Itoa(i)+`}`))
		}
		if i%cfg.SampleN == 0 || i == cfg.Turns {
			sample(i, t0)
			rss := bench.ProcRSS(os.Getpid())
			if i == 1 {
				rssStart = rss
			}
			rssEnd = rss
			if rss > rssMax {
				rssMax = rss
			}
		}
	}
	w.Flush()
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	return Verdict{
		Turns: cfg.Turns, Crashes: crashes,
		RSSStartMB: rssStart, RSSEndMB: rssEnd, RSSMaxMB: rssMax,
		GrowthMB:   rssEnd - rssStart,
		HeapMB:     float64(ms.HeapAlloc) / 1024 / 1024,
		Goroutines: runtime.NumGoroutine(),
		CSV:        cfg.OutCSV,
	}, nil
}
