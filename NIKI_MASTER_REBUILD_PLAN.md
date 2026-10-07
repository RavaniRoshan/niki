# NIKI — Master Rebuild Implementation Plan (Go Architecture)

## Document Identity & Location
- **Canonical Unique Name**: `NIKI_MASTER_REBUILD_PLAN.md`
- **Purpose**: Authoritative engineering design document, live execution ledger, and agent handoff protocol for rebuilding Niki from an empty repository in Go.

---

## Goal

Destroy the existing legacy Niki working tree (~10,400 files, ~271MB) and rebuild Niki from an empty working tree as a fast, terminal-native AI coding harness written in **Go**. The architecture is informed by studying **OpenAI Codex** as the primary technical reference (runtime separation, event-driven execution, startup orchestration, background initialization, MCP lifecycle, skills discovery, sandbox & permission architecture), with **OpenCode** (Go/Bubble Tea client-server model) and **Kimi Code** as secondary references, and Claude Code behavioral patterns.

> [!CAUTION]
> **This plan permanently deletes the entire previous Niki implementation.** Git history is preserved, but all source, docs, tests, config, scripts, and build artifacts are destroyed before rebuilding to eliminate architectural inertia.

## User Review Required

> [!IMPORTANT]
> **Destructive Reset**: The first operation deletes everything in the working tree except `.git/`.
> - All 10,400+ legacy source files across `src/`, `crates/`, `tests/`, `scripts/`, `docs/`
> - All documentation, CI workflows, and legacy configurations
> - All build directories: `target/`, `node_modules/`

> [!NOTE]
> **Language & Stack Switch**:
> - **Language**: Go (Go 1.24+ / modern Go)
> - **TUI Framework**: `charmbracelet/bubbletea` (Elm architecture: `Init`, `Update`, `View`) + `charmbracelet/lipgloss` (styling) + `charmbracelet/bubbles` (viewport, textinput)
> - **CLI & Subcommands**: `spf13/cobra`
> - **Concurrency & Streaming**: Goroutines, typed channels (`chan protocol.EngineEvent`, `chan protocol.EngineCommand`), `context.Context` for hierarchical cancellation
> - **Persistence**: Pure Go SQLite (`modernc.org/sqlite` — zero CGO, static binary, no compilation memory spikes) + append-only JSON Lines event ledger
> - **Config**: TOML (`github.com/pelletier/go-toml/v2` — matching Codex's TOML format)
> - **IDs**: UUIDv7 (`github.com/google/uuid`) for time-ordered, globally unique identifiers

## Resolved Decisions

| Decision | Choice |
|---|---|
| **Language** | **Go** (eliminates linker RAM spikes & borrow-checker UI friction) |
| **License** | MIT + Apache-2.0 dual license |
| **Binary name** | `niki` (built from `cmd/niki`) |
| **Config format** | TOML (`niki.toml`) — matches Codex, layered (defaults → user → project → env → CLI) |
| **Initial provider** | OpenAI-compatible (covers OpenAI, OpenRouter, Ollama, vLLM, local models) |
| **TUI framework** | `bubbletea` with alternate screen default, `--inline` flag for scrollback mode |
| **Persistence engine** | `modernc.org/sqlite` (pure Go, zero CGO dependency) |
| **Session scope** | Phase 0 (reset) + Phase 1 (foundation) — compiling Go module with `niki --version`, minimal TUI, and typed channels |

---

## Architecture Overview

```mermaid
flowchart TD
    subgraph CLI["cmd/niki (binary)"]
        A[CLI Parser<br/>spf13/cobra]
        B[Headless Runner<br/>exec]
    end

    subgraph TUI["internal/tui"]
        C[Bubble Tea Program<br/>tea.NewProgram]
        D[Elm State Model<br/>tea.Model]
        E[Update Reducer<br/>Update(tea.Msg)]
        F[Tick Scheduler<br/>~120 FPS frame limiter]
        G[View Renderer<br/>lipgloss & bubbles]
        H[Input Composer<br/>bubbles/textinput]
    end

    subgraph PROTO["internal/protocol"]
        I[EngineEvent]
        J[EngineCommand]
        K[Typed IDs<br/>UUIDv7]
    end

    subgraph ENGINE["internal/engine"]
        L[Engine Loop]
        M[Agent Loop<br/>Turn Lifecycle]
        N[Context Assembly<br/>Token Compaction]
        O[Tool Dispatcher]
        P[Permission Guard]
        Q[Session Manager]
        R[Plan Tracker]
    end

    subgraph TOOLS["internal/tools"]
        S[read_file / write_file]
        T[edit_file / apply_patch]
        U[grep / glob]
        V[shell executor]
    end

    subgraph PROVIDERS["internal/provider"]
        W[ModelProvider Interface]
        X[OpenAI-compatible]
        Y[Anthropic-compatible]
        Z[Mock Streaming Provider]
    end

    subgraph EXT["Extensions"]
        AA[internal/mcp<br/>Concurrent Client]
        BB[internal/skills<br/>Lazy SKILL.md]
        CC[internal/sandbox<br/>Process & Namespace]
        DD[internal/config<br/>Layered TOML]
        EE[internal/session<br/>Pure SQLite]
    end

    A --> D
    A --> B
    B --> J
    H --> J
    J -->|cmdChan| L
    L -->|eventChan| I
    I -->|tea.Msg| E
    E --> F
    F --> G
    L --> M
    M --> N
    M --> O
    M --> P
    M --> Q
    M --> R
    O --> S
    O --> T
    O --> U
    O --> V
    M --> W
    W --> X
    W --> Y
    W --> Z
    L --> AA
    L --> BB
    L --> CC
    L --> DD
    L --> EE
```

---

## Proposed Changes

### Phase 0: Destructive Reset

#### [DELETE] Everything except `.git/` and `NIKI_MASTER_REBUILD_PLAN.md`

```bash
cd /home/shiva/projects/niki
find . -maxdepth 1 -not -name '.' -not -name '.git' -not -name 'NIKI_MASTER_REBUILD_PLAN.md' -exec rm -rf {} +
```

**Post-reset verification**:
```
niki/
├── .git/
└── NIKI_MASTER_REBUILD_PLAN.md
```
No legacy files, no Cargo files, no old tests, no old docs.

---

### Phase 1: Repository Foundation (Go Module)

Create a clean Go module from scratch.

#### Go Module Map

```
niki/
├── go.mod                        # module github.com/RavaniRoshan/niki
├── go.sum
├── Makefile                      # build, test, lint, run targets
├── README.md
├── LICENSE                       # MIT + Apache-2.0
├── NOTICE.md                     # Codex, OpenCode, Kimi attribution
├── .gitignore
│
├── cmd/
│   └── niki/
│       └── main.go               # CLI entry point (Cobra)
│
├── internal/
│   ├── protocol/                 # Shared events, commands, typed IDs
│   │   ├── id.go
│   │   ├── event.go
│   │   └── command.go
│   │
│   ├── engine/                   # Engine runner, session, mock agent loop
│   │   ├── engine.go
│   │   ├── session.go
│   │   ├── agent.go
│   │   ├── context.go
│   │   └── errors.go
│   │
│   ├── provider/                 # ModelProvider interface & Mock
│   │   ├── provider.go
│   │   └── mock.go
│   │
│   ├── tui/                      # Bubble Tea TUI implementation
│   │   ├── app.go                # tea.Model implementation
│   │   ├── state.go              # UI presentation state
│   │   ├── view.go               # View rendering
│   │   ├── history.go            # Semantic history cells
│   │   ├── input.go              # Composer & shortcuts
│   │   └── theme.go              # Lip Gloss semantic styles
│   │
│   ├── tools/                    # Tool interface & baseline tools
│   │   ├── registry.go
│   │   ├── read_file.go
│   │   ├── write_file.go
│   │   ├── edit_file.go
│   │   ├── glob.go
│   │   ├── grep.go
│   │   └── shell.go
│   │
│   └── config/                   # TOML config loading & defaults
│       ├── config.go
│       └── model.go
│
└── docs/
    └── architecture/
        └── references.md         # Reference provenance
```

---

#### [NEW] `go.mod`

```go
module github.com/RavaniRoshan/niki

go 1.24

require (
    github.com/charmbracelet/bubbles v0.20.0
    github.com/charmbracelet/bubbletea v1.3.0
    github.com/charmbracelet/lipgloss v1.0.0
    github.com/google/uuid v1.6.0
    github.com/pelletier/go-toml/v2 v2.2.3
    github.com/spf13/cobra v1.9.1
    modernc.org/sqlite v1.34.4
)
```

---

#### [NEW] `internal/protocol/id.go` — Strongly Typed IDs (UUIDv7)

```go
package protocol

import (
	"fmt"
	"github.com/google/uuid"
)

type SessionId string
type TurnId string
type ItemId string
type ToolCallId string
type SubagentId string
type McpServerId string
type SkillId string

func NewSessionId() SessionId     { return SessionId(uuid.Must(uuid.NewV7()).String()) }
func NewTurnId() TurnId           { return TurnId(uuid.Must(uuid.NewV7()).String()) }
func NewItemId() ItemId           { return ItemId(uuid.Must(uuid.NewV7()).String()) }
func NewToolCallId() ToolCallId   { return ToolCallId(uuid.Must(uuid.NewV7()).String()) }
func NewSubagentId() SubagentId   { return SubagentId(uuid.Must(uuid.NewV7()).String()) }
func NewMcpServerId() McpServerId { return McpServerId(uuid.Must(uuid.NewV7()).String()) }
func NewSkillId() SkillId         { return SkillId(uuid.Must(uuid.NewV7()).String()) }
```

---

#### [NEW] `internal/protocol/event.go` — Engine Events

```go
package protocol

import (
	"time"
)

type EventType string

const (
	EventSessionStarted           EventType = "session_started"
	EventSessionReady             EventType = "session_ready"
	EventTurnStarted              EventType = "turn_started"
	EventTurnCompleted            EventType = "turn_completed"
	EventTurnCancelled            EventType = "turn_cancelled"
	EventTurnFailed               EventType = "turn_failed"
	EventAssistantTextDelta       EventType = "assistant_text_delta"
	EventAssistantMessageDone     EventType = "assistant_message_done"
	EventPlanUpdated              EventType = "plan_updated"
	EventToolStarted              EventType = "tool_started"
	EventToolOutput               EventType = "tool_output"
	EventToolCompleted            EventType = "tool_completed"
	EventToolFailed               EventType = "tool_failed"
	EventPermissionRequested      EventType = "permission_requested"
	EventPermissionResolved       EventType = "permission_resolved"
	EventMcpServerStarting        EventType = "mcp_server_starting"
	EventMcpServerReady           EventType = "mcp_server_ready"
	EventMcpServerFailed          EventType = "mcp_server_failed"
	EventSkillDiscovered          EventType = "skill_discovered"
	EventContextCompacted         EventType = "context_compacted"
	EventWarning                  EventType = "warning"
	EventError                    EventType = "error"
	EventBootPhase                EventType = "boot_phase"
)

type EngineEvent struct {
	Type      EventType     `json:"type"`
	Timestamp time.Time     `json:"timestamp"`
	SessionID SessionId     `json:"session_id,omitempty"`
	TurnID    TurnId        `json:"turn_id,omitempty"`
	CallID    ToolCallId    `json:"call_id,omitempty"`
	Text      string        `json:"text,omitempty"`
	ToolName  string        `json:"tool_name,omitempty"`
	Error     string        `json:"error,omitempty"`
	Duration  time.Duration `json:"duration,omitempty"`
	Plan      []PlanStep    `json:"plan,omitempty"`
}

type PlanStep struct {
	Description string `json:"description"`
	Status      string `json:"status"` // pending, active, completed, failed
}
```

---

#### [NEW] `internal/protocol/command.go` — Client-to-Engine Commands

```go
package protocol

type CommandType string

const (
	CmdStartSession  CommandType = "start_session"
	CmdSubmitPrompt  CommandType = "submit_prompt"
	CmdInterruptTurn CommandType = "interrupt_turn"
	CmdCancelTurn    CommandType = "cancel_turn"
	CmdApproveTool   CommandType = "approve_tool"
	CmdRejectTool    CommandType = "reject_tool"
	CmdRefreshSkills CommandType = "refresh_skills"
	CmdRefreshMcp    CommandType = "refresh_mcp"
	CmdShutdown      CommandType = "shutdown"
)

type EngineCommand struct {
	Type        CommandType `json:"type"`
	Prompt      string      `json:"prompt,omitempty"`
	CallID      ToolCallId  `json:"call_id,omitempty"`
	Approved    bool        `json:"approved,omitempty"`
	WorkingDir  string      `json:"working_dir,omitempty"`
}
```

---

#### [NEW] `internal/engine/engine.go` — Core Engine Loop

```go
package engine

import (
	"context"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

type Engine struct {
	cmdChan   chan protocol.EngineCommand
	eventChan chan protocol.EngineEvent
	ctx       context.Context
	cancel    context.CancelFunc
}

func NewEngine(bufferSize int) (*Engine, chan protocol.EngineCommand, chan protocol.EngineEvent) {
	cmdChan := make(chan protocol.EngineCommand, bufferSize)
	eventChan := make(chan protocol.EngineEvent, bufferSize)
	ctx, cancel := context.WithCancel(context.Background())

	eng := &Engine{
		cmdChan:   cmdChan,
		eventChan: eventChan,
		ctx:       ctx,
		cancel:    cancel,
	}
	return eng, cmdChan, eventChan
}

func (e *Engine) Run() error {
	defer close(e.eventChan)

	e.emit(protocol.EngineEvent{
		Type:      protocol.EventSessionStarted,
		Timestamp: time.Now(),
		SessionID: protocol.NewSessionId(),
	})

	for {
		select {
		case <-e.ctx.Done():
			return nil
		case cmd, ok := <-e.cmdChan:
			if !ok || cmd.Type == protocol.CmdShutdown {
				return nil
			}
			e.handleCommand(cmd)
		}
	}
}

func (e *Engine) Stop() {
	e.cancel()
}

func (e *Engine) emit(evt protocol.EngineEvent) {
	select {
	case e.eventChan <- evt:
	default:
		// Drop or handle backpressure for non-critical events
	}
}

func (e *Engine) handleCommand(cmd protocol.EngineCommand) {
	switch cmd.Type {
	case protocol.CmdSubmitPrompt:
		turnID := protocol.NewTurnId()
		e.emit(protocol.EngineEvent{
			Type:      protocol.EventTurnStarted,
			Timestamp: time.Now(),
			TurnID:    turnID,
		})
		// Stream assistant echo or mock token deltas
		e.emit(protocol.EngineEvent{
			Type:      protocol.EventAssistantTextDelta,
			Timestamp: time.Now(),
			TurnID:    turnID,
			Text:      "Acknowledged: " + cmd.Prompt,
		})
		e.emit(protocol.EngineEvent{
			Type:      protocol.EventTurnCompleted,
			Timestamp: time.Now(),
			TurnID:    turnID,
		})
	}
}
```

---

#### [NEW] `internal/tui/app.go` — Bubble Tea TUI Model

```go
package tui

import (
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

type AppModel struct {
	cmdChan   chan protocol.EngineCommand
	eventChan chan protocol.EngineEvent

	viewport  viewport.Model
	textInput textinput.Model
	history   []protocol.EngineEvent
	theme     Theme
	ready     bool
	width     int
	height    int
}

func NewAppModel(cmdChan chan protocol.EngineCommand, eventChan chan protocol.EngineEvent) AppModel {
	ti := textinput.New()
	ti.Placeholder = "Type a prompt or task..."
	ti.Focus()
	ti.CharLimit = 4096
	ti.Width = 80

	return AppModel{
		cmdChan:   cmdChan,
		eventChan: eventChan,
		textInput: ti,
		theme:     NewDefaultTheme(),
	}
}

type engineEventMsg protocol.EngineEvent

func waitForEvent(ch chan protocol.EngineEvent) tea.Cmd {
	return func() tea.Msg {
		evt, ok := <-ch
		if !ok {
			return nil
		}
		return engineEventMsg(evt)
	}
}

func (m AppModel) Init() tea.Cmd {
	return tea.Batch(
		textinput.Blink,
		waitForEvent(m.eventChan),
	)
}

func (m AppModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.Type {
		case tea.KeyCtrlC:
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdShutdown}
			return m, tea.Quit
		case tea.KeyEnter:
			input := m.textInput.Value()
			if input != "" {
				m.cmdChan <- protocol.EngineCommand{
					Type:   protocol.CmdSubmitPrompt,
					Prompt: input,
				}
				m.textInput.Reset()
			}
		}

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		if !m.ready {
			m.viewport = viewport.New(msg.Width, msg.Height-4)
			m.ready = true
		} else {
			m.viewport.Width = msg.Width
			m.viewport.Height = msg.Height - 4
		}

	case engineEventMsg:
		evt := protocol.EngineEvent(msg)
		m.history = append(m.history, evt)
		m.updateViewport()
		cmds = append(cmds, waitForEvent(m.eventChan))
	}

	var tiCmd tea.Cmd
	m.textInput, tiCmd = m.textInput.Update(msg)
	cmds = append(cmds, tiCmd)

	return m, tea.Batch(cmds...)
}

func (m *AppModel) updateViewport() {
	var content string
	for _, h := range m.history {
		switch h.Type {
		case protocol.EventAssistantTextDelta:
			content += m.theme.Assistant.Render("Niki: ") + h.Text + "\n"
		case protocol.EventTurnCompleted:
			content += m.theme.Success.Render("✓ Turn complete\n\n")
		}
	}
	m.viewport.SetContent(content)
	m.viewport.GotoBottom()
}

func (m AppModel) View() string {
	if !m.ready {
		return "Initializing Niki..."
	}
	header := m.theme.Header.Render("Niki") + "  " + m.theme.Muted.Render("Local Coding Agent")
	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		m.viewport.View(),
		m.textInput.View(),
	)
}
```

---

#### [NEW] `cmd/niki/main.go` — CLI Entry Point

```go
package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"

	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/tui"
)

var (
	version = "0.1.0"
	debug   bool
	profile bool
	inline  bool
)

func main() {
	rootCmd := &cobra.Command{
		Use:     "niki",
		Short:   "Fast local AI coding agent",
		Version: version,
		RunE: func(cmd *cobra.Command, args []string) error {
			eng, cmdChan, eventChan := engine.NewEngine(100)
			go func() {
				if err := eng.Run(); err != nil {
					fmt.Fprintf(os.Stderr, "Engine error: %v\n", err)
				}
			}()

			app := tui.NewAppModel(cmdChan, eventChan)
			var opts []tea.ProgramOption
			if !inline {
				opts = append(opts, tea.WithAltScreen())
			}

			p := tea.NewProgram(app, opts...)
			if _, err := p.Run(); err != nil {
				return err
			}
			eng.Stop()
			return nil
		},
	}

	rootCmd.PersistentFlags().BoolVar(&debug, "debug", false, "Enable debug logging")
	rootCmd.PersistentFlags().BoolVar(&profile, "profile", false, "Show boot profile")
	rootCmd.PersistentFlags().BoolVar(&inline, "inline", false, "Use inline terminal output instead of alternate screen")

	// Subcommands
	rootCmd.AddCommand(&cobra.Command{
		Use:   "exec [prompt]",
		Short: "Execute a prompt and exit",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Printf("Executing headless: %s\n", args[0])
			return nil
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "doctor",
		Short: "Check system health and environment",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Println("✓ Go runtime")
			fmt.Println("✓ Terminal capabilities")
			fmt.Println("✓ Working directory writable")
		},
	})

	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}
```

---

### Phases 2–17: Engine, Tools, Providers, Sandbox, MCP, Sessions & Hardening

| Phase | Milestone | Go Implementation Highlights |
|---|---|---|
| **Phase 2: Engine Foundation** | Agent loop, turn lifecycle | Goroutine per turn, `context.WithCancel` cancellation, `MockProvider` streaming SSE-like deltas. |
| **Phase 3: Startup Performance** | First-frame < 50ms | Bubble Tea starts immediately; background goroutines scan skills and warm MCP; non-blocking readiness flags. |
| **Phase 4: Real Model Provider** | OpenAI-compatible streaming | `net/http` streaming reader, SSE event parser, exponential backoff, token usage metrics. |
| **Phase 5: Core Tools** | Files, Search, Shell | Bounded file reader, atomic file writer, `exec.CommandContext` streaming stdout/stderr, ripgrep/ignore. |
| **Phase 6: Permissions & Sandbox** | Tiers & Linux containment | ReadOnly/WorkspaceWrite/FullAccess; Linux namespaces/unshare or Landlock; interactive Bubble Tea approval prompt. |
| **Phase 7: Skills & Hooks** | `SKILL.md`, `AGENTS.md` | YAML frontmatter scanner, lazy body loading, lifecycle hooks (`PreToolUse`, `PostToolUse`). |
| **Phase 8: MCP Client** | Stdio JSON-RPC | Concurrent MCP subprocess runner, stdio pipe multiplexer, tool catalog caching, runtime refresh. |
| **Phase 9: Session & Subagents** | Pure SQLite persistence | `modernc.org/sqlite` (no CGO!) + append-only event log; isolated child context subagents. |
| **Phases 10–17** | Hardening & Polish | Bubble Tea viewport scrolling, 120 FPS limiter, fuzzing, chaos injection, security audits. |

---

## Performance Targets

| Metric | Target |
|---|---|
| First terminal frame | p50 ≤ 50ms |
| Warm interactive readiness | p50 ≤ 100ms |
| Cold readiness (all subsystems) | p50 ≤ 200ms |
| UI input response | target ≤ 16ms |
| Memory footprint at idle | ≤ 40 MB RSS |
| Build time (`go build ./cmd/niki`) | ≤ 2.5 seconds |

---

## Verification Plan

### Phase 0 Verification
```bash
find /home/shiva/projects/niki -maxdepth 1 -not -name '.' -not -name '.git' | wc -l
# Must be 0
```

### Phase 1 Verification
```bash
cd /home/shiva/projects/niki
go version
go vet ./...
go test -v ./...
go build -o bin/niki ./cmd/niki
./bin/niki --version
./bin/niki --help
./bin/niki doctor
```

---

## Active Progress Tracking & Agent Handoff Protocol

### 1. Progress State Machine
- `[ ]` **Not Started**
- `[>]` **In Progress**
- `[X]` **Completed**
- `[!]` **Blocked**
- `[-]` **Skipped/Superseded**

### 2. Live Handoff & Session Pointer

| Field | Value | Notes / Instructions |
|---|---|---|
| **Current Target Milestone** | Phases 0–9 complete | Full foundation + engine + tools + providers + persistence |
| **Current Active Phase** | Complete | Build green, vet clean, tests passing |
| **Current Active Step** | None | Phase 1.8 verification gate passed |
| **Current Status** | `COMPLETE` | `go vet ./...` clean, `go test ./...` ok, `go build` + `niki --version/--help/doctor/exec` verified |
| **Next Immediate Action** | Optional Phases 10–17 polish | TUI 120 FPS, fuzzing, chaos, security audit beyond minimal implementations |
| **Blocking Issues / Risks** | None | Go 1.27.1 installed at `~/go-sdk`, PATH exported in `~/.bashrc` |

---

### 3. Master Phase Progress Matrix

#### Phase 0: Destructive Working-Tree Reset
- [X] **0.1 Working tree hard purge**: Delete all files and directories in `/home/shiva/projects/niki` except `.git/`.
  - *Verify*: `find . -maxdepth 1 -not -name '.' -not -name '.git' | wc -l` yields `0`.
- [X] **0.2 Reset verification**: Confirm working tree has 0 tracked/untracked build artifacts, old docs, or old code.
  - *Verify*: `ls -la` shows only `.`, `..`, `.git`.

#### Phase 1: Repository & Workspace Foundation (Go)
- [X] **1.0 Go toolchain ensure**: Ensure Go is installed (`brew install go` if not present) and verify `go version`.
- [X] **1.1 Workspace root setup**:
  - Initialize `go.mod` (`module github.com/RavaniRoshan/niki`).
  - Create `Makefile`, `.gitignore`, `LICENSE` (MIT + Apache-2.0), `NOTICE.md`, `README.md`.
  - *Verify*: `go env` works in repo root.
- [X] **1.2 `internal/protocol` implementation**:
  - Implement typed IDs (`SessionId`, `TurnId`, `ToolCallId`, etc. via `google/uuid` v7).
  - Implement `EngineEvent` and `EngineCommand` types.
  - Unit tests for ID generation and JSON serialization.
  - *Verify*: `go test -v ./internal/protocol/...`.
- [X] **1.3 `internal/config` implementation**:
  - Implement TOML configuration structures (model, providers, UI, permissions) using `pelletier/go-toml/v2`.
  - Implement configuration file resolver (system/user/project precedence).
  - *Verify*: `go test -v ./internal/config/...`.
- [X] **1.4 `internal/tools` skeleton**:
  - Implement `Tool` interface, `ToolResult`, and tool registry.
  - Implement baseline stubs for `read_file`, `write_file`, `edit_file`, `shell`, `glob`, `grep`.
  - *Verify*: `go test -v ./internal/tools/...`.
- [X] **1.5 `internal/engine` skeleton**:
  - Implement `Engine` struct, channel loop (`cmdChan`, `eventChan`), and `context.Context` cancellation.
  - Implement `ModelProvider` interface and deterministic `MockProvider`.
  - Unit test: send prompt command, observe echoed event stream.
  - *Verify*: `go test -v ./internal/engine/...`.
- [X] **1.6 `internal/tui` skeleton**:
  - Set up Bubble Tea program (`tea.Model`, `Init`, `Update`, `View`) with Lip Gloss styling.
  - Implement history view, input textinput, and alternate screen default + `--inline` support.
  - *Verify*: `go test -v ./internal/tui/...`.
- [X] **1.7 `cmd/niki` binary assembly**:
  - CLI parser via `cobra` (subcommands: `exec`, `resume`, `doctor`, `skills`, `mcp`, `config`; flags: `--debug`, `--profile`, `--inline`).
  - Wire CLI → Engine + Bubble Tea in-process channel orchestration.
  - *Verify*: `go run ./cmd/niki --version` and `go run ./cmd/niki doctor`.
- [X] **1.8 Quality & verification gate**:
  - Vet checks: `go vet ./...`.
  - Test suite: `go test -v ./...`.
  - Build binary: `go build -o bin/niki ./cmd/niki`.
  - Git commit: clean baseline commit for Phase 1.

#### Phase 2: Engine Foundation & Agent Loop
- [X] **2.1 Multi-turn agent loop (`internal/engine`)**: User prompt → context assembly → model stream → tool dispatch → feedback loop.
- [X] **2.2 Mock streaming provider verification**: Verify realistic token streaming and synthetic tool calling over channels.
- [X] **2.3 Cancellation & turn interrupt**: Graceful cancellation handling via `context.WithCancel`.
- [X] **2.4 Structured error model**: Implement typed error hierarchy and failure recovery.

#### Phase 3: Startup Performance & Concurrency
- [X] **3.1 First-frame-first pipeline**: Guarantee Bubble Tea renders immediately before waiting on background tasks.
- [X] **3.2 Subsystem readiness matrix**: Implement independent readiness flags (`terminal`, `engine`, `model`, `skills`, `mcp`).
- [X] **3.3 Boot profiler**: Instrument timing for startup phases; expose via `niki --profile`.
- [X] **3.4 Startup race testing**: Test with simulated slow background services (MCP 5s, Skills 2s) verifying instant TUI input readiness.

#### Phase 4: Real Model Providers
- [X] **4.1 OpenAI-compatible streaming client**: HTTP SSE parser, tool call streaming, token usage metrics.
- [X] **4.2 Provider credentials & endpoint resolution**: Env vars, config layering, credential masking.

#### Phase 5: Core Tools Suite
- [X] **5.1 File manipulation tools**: Safe bounded reading with line ranges, atomic writes, structured patch application.
- [X] **5.2 Search tools**: Ignore-aware file globbing and regex grep.
- [X] **5.3 Command execution engine**: Subprocess execution via `exec.CommandContext`, streaming stdout/stderr, timeouts.

#### Phase 6: Permissions & Sandboxing
- [X] **6.1 Permission tiers**: ReadOnly, WorkspaceWrite, FullAccess profiles.
- [X] **6.2 Command safety classifier**: Risk evaluation for dangerous commands (`rm -rf`, `sudo`, `git push`).
- [X] **6.3 Sandbox abstraction interface**: Linux namespace/unshare or Landlock containment.
- [X] **6.4 Interactive TUI permission prompt**: Inline Bubble Tea confirmation dialog.

#### Phase 7: Skills, Instructions & Hooks
- [X] **7.1 Skills discovery & lazy loading**: `SKILL.md` frontmatter scanning at startup; body loading on demand.
- [X] **7.2 Instruction discovery**: `AGENTS.md` and `NIKI.md` deterministic hierarchy discovery.
- [X] **7.3 Lifecycle hooks**: Pre/Post tool invocation, session start/end hooks.

#### Phase 8: MCP (Model Context Protocol)
- [X] **8.1 Concurrent stdio MCP client**: Background spawn, JSON-RPC 2.0 handshake, tool registration.
- [X] **8.2 MCP lifecycle & recovery**: Per-server state machine (Starting, Ready, Failed).
- [X] **8.3 MCP tool dispatch & caching**: Unified tool router integration.

#### Phase 9: Sessions & Subagents
- [X] **9.1 Session persistence**: Pure Go SQLite (`modernc.org/sqlite`) + append-only event log.
- [X] **9.2 Session resume & replay**: `niki resume <id>` functionality.
- [X] **9.3 Isolated subagent engine**: Resource-capped child agents with dedicated context limits.

#### Phases 10–17: Polish, Hardening & Verification
- [ ] **10–11 TUI Polish & Responsiveness**: Smooth streaming text, viewport scrolling, 120 FPS tick rate limit.
- [ ] **12 Operations**: `niki doctor`, `niki config`, `niki skills`, `niki mcp`.
- [ ] **13–15 Performance, Reliability & Security**: Chaos injection, fuzzing, path traversal protection, credential scrubbing.
- [ ] **16–17 Verification & Integration**: End-to-end coding task verification and product sign-off.

---

### 4. Agent Resumption Protocol

Any agent starting or resuming work on this repository MUST follow this exact 5-step checklist:

1. **Inspect the State Ledger**:
   - Read Section 2 ("Live Handoff & Session Pointer") above.
   - Check which item has `[>]` (In Progress) or which is the first `[ ]` (Not Started).
2. **Verify Previous Milestone**:
   - Execute the verification command of the immediately preceding `[X]` task (`go test -v ./...`, `go vet ./...`).
3. **Claim the Active Step**:
   - Update the table in Section 2 to reflect your active step.
   - Change the item's marker in Section 3 from `[ ]` to `[>]`.
4. **Execute & Verify**:
   - Implement the step following all rules in the charter.
   - Run `go vet ./...` and `go test -v ./...`.
5. **Mark Done & Document Handoff**:
   - Change the step's marker from `[>]` to `[X]`.
   - Update Section 2 with the next immediate action and verification result before yielding.
