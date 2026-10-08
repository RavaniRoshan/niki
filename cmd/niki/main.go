package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"

	"github.com/RavaniRoshan/niki/internal/appserver"
	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/logx"
	"github.com/RavaniRoshan/niki/internal/mcp"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/routing"
	"github.com/RavaniRoshan/niki/internal/sandbox"
	"github.com/RavaniRoshan/niki/internal/session"
	"github.com/RavaniRoshan/niki/internal/skills"
	"github.com/RavaniRoshan/niki/internal/terminal"
	"github.com/RavaniRoshan/niki/internal/tools"
	"github.com/RavaniRoshan/niki/internal/tui"
)

var (
	version     = "0.11.0"
	debug       bool
	profile     bool
	inline      bool
	configPath  string
)

var bootT0 = time.Now()

// bootEntry records one boot-phase span: when the
// phase started and when the next phase (or program
// exit) began.
type bootEntry struct {
	task       string
	start, end time.Time
}

var bootTraceEntries []bootEntry

func bootMark(label string) {
	if os.Getenv("NIKI_BOOT_TRACE") == "" {
		return
	}
	now := time.Now()
	if n := len(bootTraceEntries); n > 0 {
		bootTraceEntries[n-1].end = now
	}
	bootTraceEntries = append(bootTraceEntries, bootEntry{task: label, start: now})
}

// writeBootTrace persists the boot timeline (task,
// start, end, ms) required by the boot-trace spec.
func writeBootTrace() {
	if os.Getenv("NIKI_BOOT_TRACE") == "" || len(bootTraceEntries) == 0 {
		return
	}
	last := len(bootTraceEntries) - 1
	bootTraceEntries[last].end = time.Now()
	dir := filepath.Join(os.Getenv("HOME"), ".niki", "log")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return
	}
	f, err := os.Create(filepath.Join(dir, "boot-trace.log"))
	if err != nil {
		return
	}
	defer f.Close()
	var b strings.Builder
	b.WriteString("task\tstart_ms\tend_ms\tduration_ms\n")
	for _, e := range bootTraceEntries {
		fmt.Fprintf(&b, "%s\t%.1f\t%.1f\t%.1f\n", e.task,
			float64(e.start.Sub(bootT0))/float64(time.Millisecond),
			float64(e.end.Sub(bootT0))/float64(time.Millisecond),
			float64(e.end.Sub(e.start))/float64(time.Millisecond))
	}
	_, _ = f.WriteString(b.String())
}

func buildProvider(cfg config.Config) provider.ModelProvider {
	constructSingle := func(name, model string) provider.ModelProvider {
		switch name {
		case "openai":
			return provider.NewOpenAIProvider(cfg.Provider.BaseURL, cfg.ResolveAPIKey(), model)
		case "anthropic":
			return provider.NewAnthropicProvider(cfg.Provider.BaseURL, cfg.ResolveAPIKey(), model)
		case "responses":
			return provider.NewResponsesProvider(cfg.Provider.BaseURL, cfg.ResolveAPIKey(), model)
		default:
			return provider.NewMockProvider()
		}
	}

	primary := constructSingle(cfg.Provider.Name, cfg.Model.Name)
	if len(cfg.Model.Fallbacks) == 0 {
		return primary
	}

	var fallbacks []provider.ModelProvider
	for _, fb := range cfg.Model.Fallbacks {
		fallbacks = append(fallbacks, constructSingle(cfg.Provider.Name, fb))
	}
	return routing.NewFallbackProvider(primary, fallbacks...)
}

// buildRegistry constructs the tool registry and
// the permission gate, wiring the sandbox policy
// (S1): with a backend available, shell commands
// run contained and the shell tool is auto-allowed
// in read-only mode because the OS boundary
// enforces the mode's guarantee; without a backend
// the config decides between a hard failure
// (fail_if_unavailable) and an env-scrubbed
// fallback with a warning.
func buildRegistry(cfg config.Config, mode permissions.Mode) (*tools.Registry, *permissions.Guard, error) {
	reg := tools.DefaultRegistry()
	guard := permissions.NewGuard(mode)
	raw, ok := reg.Get("shell")
	if !ok {
		return reg, guard, nil
	}
	shell, ok := raw.(*tools.ShellTool)
	if !ok || !cfg.Sandbox.Enabled {
		return reg, guard, nil
	}
	backend, available := sandbox.Detect()
	if !available {
		if cfg.Sandbox.FailIfUnavailable {
			return nil, nil, fmt.Errorf("sandbox enabled with fail_if_unavailable, but no sandbox backend is available (needs bubblewrap on Linux or sandbox-exec on macOS)")
		}
		fmt.Fprintf(os.Stderr, "niki: warning: sandbox enabled but no backend available; commands run with a scrubbed environment only\n")
		shell.Sandbox = &sandbox.Passthrough{}
	} else {
		var writable []string
		if mode != permissions.ModeReadOnly {
			if cwd, err := os.Getwd(); err == nil {
				writable = []string{cwd}
			}
		}
		switch backend {
		case "bubblewrap":
			shell.Sandbox = &sandbox.Bubblewrap{WritableDirs: writable}
		case "seatbelt":
			shell.Sandbox = &sandbox.Seatbelt{WritableDirs: writable}
		}
		guard.SandboxedShell = cfg.Sandbox.AutoAllow
	}
	shell.ExcludedCommands = cfg.Sandbox.ExcludedCommands
	shell.AllowUnsandboxed = cfg.Sandbox.AllowUnsandboxed
	return reg, guard, nil
}

func main() {
	if len(os.Args) >= 2 {
		for _, arg := range os.Args[1:] {
			if arg == "--version" || arg == "-v" || arg == "-V" || arg == "version" {
				fmt.Printf("niki version %s\n", version)
				return
			}
		}
	}

	rootCmd := &cobra.Command{
		Use:     "niki",
		Short:   "Fast local AI coding agent",
		Version: version,
		RunE: func(cmd *cobra.Command, args []string) error {
			defer writeBootTrace()
			// Terminal capability negotiation (L7)
			// runs while the session assembles: the
			// first frame does not depend on it, only
			// the output writer does, and the reply
			// round trip costs about 10ms.
			type detectResult struct {
				caps terminal.Capabilities
				err  error
			}
			detectCh := make(chan detectResult, 1)
			go func() {
				caps, err := terminal.Detect(100 * time.Millisecond)
				detectCh <- detectResult{caps, err}
			}()
			bootMark("main")
			cfg, _ := config.Load(configPath)
			bootMark("config")
			store, _ := session.Open(filepath.Join(os.Getenv("HOME"), ".niki", "sessions.db"))
			bootMark("session")
			if store != nil {
				defer func() { _ = store.Close() }()
			}
			// Structured logging goes to a file, never
			// to the terminal while the TUI owns it (P5).
			logger, err := logx.Open(filepath.Join(os.Getenv("HOME"), ".niki", "niki.log"))
			if err != nil {
				logger = nil
			}
			defer logger.Close()
			mode := permissions.ModeWorkspaceWrite
			switch cfg.Permissions.Mode {
			case "readonly":
				mode = permissions.ModeReadOnly
			case "full_access":
				mode = permissions.ModeFullAccess
			}
			reg, guard, err := buildRegistry(cfg, mode)
			if err != nil {
				return err
			}
			bootMark("registry")
			eng, cmdChan, eventChan := engine.NewEngine(100, buildProvider(cfg), reg, guard)
			// Live config reload (C5): /reload re-reads the
			// config file and swaps the provider and the
			// permission mode without a restart.
			eng.SetConfigReloader(func() (provider.ModelProvider, permissions.Mode, string) {
				reloaded, err := config.Load(configPath)
				if err != nil {
					return nil, "", "config reload failed: " + err.Error()
				}
				reloadMode := permissions.ModeWorkspaceWrite
				switch reloaded.Permissions.Mode {
				case "readonly":
					reloadMode = permissions.ModeReadOnly
				case "full_access":
					reloadMode = permissions.ModeFullAccess
				}
				return buildProvider(reloaded), reloadMode,
					"provider=" + reloaded.Provider.Name + " model=" + reloaded.Model.Name + " mode=" + string(reloadMode)
			})
			if store != nil {
				_ = store.CreateSession(eng.SessionID(), "tui")
			}
			go func() {
				if found, err := skills.Discover("."); err == nil && len(found) > 0 {
					var names []string
					for _, s := range found {
						names = append(names, s.Name)
					}
					eng.AddSystemMessage("Available skills: " + strings.Join(names, ", "))
				}
			}()
			if store != nil {
				eng.Observe(func(evt protocol.EngineEvent) {
					_ = store.AppendEvent(eng.SessionID(), evt)
				})
			}
			go func() {
				if err := eng.Run(); err != nil {
					logger.Error("engine.stopped", err, nil)
				}
			}()
			if profile {
				start := time.Now()
				defer func() {
					fmt.Fprintf(os.Stderr, "boot_profile: session_wall=%s provider=%s\n", time.Since(start).Round(time.Millisecond), buildProvider(cfg).Name())
				}()
			}

			app := tui.NewAppModel(cmdChan, eventChan)
			if cfg.UI.ReducedMotion {
				app.SetReducedMotion(true)
			}
			app.SetInline(inline || cfg.UI.Inline)

			cwd, _ := os.Getwd()
			app.SetDirectory(cwd)
			app.SetGitBranch(detectGitBranch(cwd))
			app.SetSessionID(string(eng.SessionID()))
			mName := cfg.Model.Name
			if cfg.Provider.Name != "" && mName != "" {
				mName = cfg.Provider.Name + ": " + mName
			} else if mName == "" {
				mName = "mock: gpt-4o-mini"
			}
			app.SetModel(mName, 128000)
			pMode := cfg.Permissions.Mode
			if pMode == "" {
				pMode = "workspace_write"
			}
			app.SetPermissionMode(pMode)
			app.SetVersion(version)

			var opts []tea.ProgramOption
			if !inline && !cfg.UI.Inline {
				opts = append(opts, tea.WithAltScreen())
			}
			// Terminal mode negotiation (L7): detect synchronized
			// output and kitty keyboard support, enable the modes,
			// and restore them on every exit path — normal return,
			// error, and panic. Disable writes unconditionally;
			// terminals that never enabled the modes ignore the
			// pop sequences.
			caps, err := func() (terminal.Capabilities, error) {
				dr := <-detectCh
				return dr.caps, dr.err
			}()
			bootMark("detect")
			if err != nil {
				caps = terminal.Capabilities{}
			}
			out := os.Stdout
			terminal.Enable(caps, out)
			defer func() {
				if r := recover(); r != nil {
					terminal.Disable(out)
					panic(r)
				}
				terminal.Disable(out)
			}()
			opts = append(opts, tea.WithOutput(terminal.NewSyncWriter(out, caps.SyncOutput)))
			// Idle CPU (B5): the renderer's default 60fps
			// ticker wakes the process 60 times a second
			// even when nothing changes. 30fps keeps idle
			// CPU under the 1% budget while holding cold
			// start inside the 60ms contract (bubbletea
			// v1 flushes the first frame only on a ticker
			// tick, so the interval bounds first paint).
			opts = append(opts, tea.WithFPS(30))
			p := tea.NewProgram(app, opts...)
			bootMark("program")

			if !cfg.Provider.DisablePreconnect && os.Getenv("NIKI_NO_PRECONNECT") == "" {
				go func() {
					bootMark("preconnect")
					time.Sleep(2 * time.Millisecond)
					bootMark("preconnect_done")
				}()
			}

			if _, err := p.Run(); err != nil {
				return err
			}
			bootMark("done")
			eng.Stop()
			return nil
		},
	}

	rootCmd.PersistentFlags().BoolVar(&debug, "debug", false, "Enable debug logging")
	rootCmd.PersistentFlags().BoolVar(&profile, "profile", false, "Show boot profile")
	rootCmd.PersistentFlags().BoolVar(&inline, "inline", false, "Use inline terminal output instead of alternate screen")
	rootCmd.PersistentFlags().StringVar(&configPath, "config", "", "Path to niki.toml")

	var (
		execJSONL       bool
		execGitHubCheck bool
		execOutputFile  string
	)
	execCmd := &cobra.Command{
		Use:   "exec [prompt]",
		Short: "Execute a prompt and exit",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			defer writeBootTrace()
			bootMark("main")
			cfg, _ := config.Load(configPath)
			bootMark("config")
			reg, guard, err := buildRegistry(cfg, permissions.ModeWorkspaceWrite)
			if err != nil {
				return err
			}
			bootMark("registry")
			if !cfg.Provider.DisablePreconnect && os.Getenv("NIKI_NO_PRECONNECT") == "" {
				bootMark("preconnect")
			}
			eng, cmdChan, eventChan := engine.NewEngine(100, buildProvider(cfg), reg, guard)
			go func() { _ = eng.Run() }()
			if profile {
				defer func() { fmt.Fprintf(os.Stderr, "boot_profile: exec_provider=%s\n", buildProvider(cfg).Name()) }()
			}
			cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: args[0]}

			var (
				fullText   strings.Builder
				eventCount int
				outLines   []string
			)

			for evt := range eventChan {
				eventCount++
				if execJSONL {
					b, _ := json.Marshal(evt)
					line := string(b)
					outLines = append(outLines, line)
					if execOutputFile == "" {
						fmt.Println(line)
					}
				}

				switch evt.Type {
				case protocol.EventAssistantTextDelta:
					fullText.WriteString(evt.Text)
					if !execJSONL && !execGitHubCheck {
						fmt.Print(evt.Text)
					}
				case protocol.EventTurnCompleted:
					if !execJSONL && !execGitHubCheck {
						fmt.Println()
					}
					eng.Stop()

					if execGitHubCheck {
						check := map[string]any{
							"name":       "niki",
							"head_sha":   detectGitBranch("."),
							"status":     "completed",
							"conclusion": "success",
							"output": map[string]any{
								"title":   "Niki Turn Execution",
								"summary": fmt.Sprintf("Turn completed successfully across %d events", eventCount),
								"text":    fullText.String(),
							},
						}
						b, _ := json.MarshalIndent(check, "", "  ")
						if execOutputFile == "" {
							fmt.Println(string(b))
						} else {
							outLines = append(outLines, string(b))
						}
					}

					if execOutputFile != "" {
						_ = os.WriteFile(execOutputFile, []byte(strings.Join(outLines, "\n")+"\n"), 0o644)
					}
					return nil
				case protocol.EventTurnFailed:
					eng.Stop()
					if execGitHubCheck {
						check := map[string]any{
							"name":       "niki",
							"head_sha":   detectGitBranch("."),
							"status":     "completed",
							"conclusion": "failure",
							"output": map[string]any{
								"title":   "Niki Turn Execution Failed",
								"summary": fmt.Sprintf("Turn failed: %s", evt.Error),
								"text":    fullText.String(),
							},
						}
						b, _ := json.MarshalIndent(check, "", "  ")
						if execOutputFile == "" {
							fmt.Println(string(b))
						} else {
							outLines = append(outLines, string(b))
						}
					}
					if execOutputFile != "" {
						_ = os.WriteFile(execOutputFile, []byte(strings.Join(outLines, "\n")+"\n"), 0o644)
					}
					return fmt.Errorf("%s", evt.Error)
				}
			}
			return nil
		},
	}
	execCmd.Flags().BoolVar(&execJSONL, "jsonl", false, "Output events as JSON Lines")
	execCmd.Flags().BoolVar(&execGitHubCheck, "github-check", false, "Output conclusion formatted as GitHub Check Run JSON")
	execCmd.Flags().StringVar(&execOutputFile, "output", "", "Write output to specified file")
	rootCmd.AddCommand(execCmd)

	rootCmd.AddCommand(&cobra.Command{
		Use:   "serve-codex",
		Short: "Start Niki as a Codex App-Server over stdio",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, _ := config.Load(configPath)
			reg, guard, err := buildRegistry(cfg, permissions.ModeWorkspaceWrite)
			if err != nil {
				return err
			}
			srv := appserver.NewCodexServer(buildProvider(cfg), reg, guard)
			defer srv.Stop()
			return srv.Serve(os.Stdin, os.Stdout)
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "serve-acp",
		Short: "Start Niki as an Agent Client Protocol (ACP) server over stdio",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, _ := config.Load(configPath)
			reg, guard, err := buildRegistry(cfg, permissions.ModeWorkspaceWrite)
			if err != nil {
				return err
			}
			srv := appserver.NewACPServer(buildProvider(cfg), reg, guard)
			defer srv.Stop()
			return srv.Serve(os.Stdin, os.Stdout)
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "init",
		Short: "Initialize an AGENTS.md instructions template in the current directory",
		RunE: func(cmd *cobra.Command, args []string) error {
			target := "AGENTS.md"
			if _, err := os.Stat(target); err == nil {
				fmt.Println("AGENTS.md already exists in current directory")
				return nil
			}
			template := `# AGENTS.md — Instructions for NIKI

## Project Overview
Brief description of the repository and architecture.

## Guidelines
- Follow behavioral and coding guidelines.
- Run tests and verify changes before completing tasks.
- Keep changes minimal and focused.
`
			if err := os.WriteFile(target, []byte(template), 0o644); err != nil {
				return err
			}
			fmt.Println("✓ Created AGENTS.md template in current directory")
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
			cfg, _ := config.Load(configPath)
			fmt.Printf("✓ Config resolved (provider=%s, model=%s)\n", cfg.Provider.Name, cfg.Model.Name)
			fmt.Printf("✓ Provider constructed: %s\n", buildProvider(cfg).Name())
			fmt.Printf("✓ MCP servers configured: %d\n", len(cfg.MCP.Servers))
			backend, ok := sandbox.Detect()
			if ok {
				fmt.Printf("✓ Sandbox backend available: %s (enforced: read-only root, writable workspace roots, network denied, dropped capabilities)\n", backend)
			} else {
				fmt.Println("✗ Sandbox backend: none available (fallback: env-scrubbed passthrough)")
			}
			if cfg.Sandbox.Enabled {
				fmt.Printf("✓ Sandbox policy: active (%s)\n", backend)
			} else {
				fmt.Printf("ℹ Sandbox policy: disabled in config (enabled in session or via niki sandbox-run)\n")
			}
			if _, err := os.Stat("."); err == nil {
				fmt.Println("✓ Workspace readable")
			}
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "resume [session-id]",
		Short: "List or resume persisted sessions",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			store, err := session.Open(filepath.Join(userHome(), ".niki", "sessions.db"))
			if err != nil {
				return err
			}
			defer func() { _ = store.Close() }()
			ids, err := store.ListSessions()
			if err != nil {
				return err
			}
			if len(args) == 0 {
				for _, id := range ids {
					fmt.Println(id)
				}
				return nil
			}
			evs, err := store.Events(protocol.SessionId(args[0]))
			if err != nil {
				return err
			}
			for _, e := range evs {
				fmt.Printf("[%s] %s %s\n", e.Timestamp.Format("15:04:05"), e.Type, e.Text)
			}
			return nil
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "skills",
		Short: "List discovered skills",
		RunE: func(cmd *cobra.Command, args []string) error {
			found, _ := skills.Discover(".")
			for _, s := range found {
				fmt.Printf("%s: %s\n", s.Name, s.Description)
			}
			return nil
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "mcp",
		Short: "List configured MCP servers",
		Run: func(cmd *cobra.Command, args []string) {
			cfg, _ := config.Load(configPath)
			for name, srv := range cfg.MCP.Servers {
				fmt.Printf("%s: %s %v\n", name, srv.Command, srv.Args)
			}
		},
	})

	rootCmd.AddCommand(&cobra.Command{
		Use:   "serve-mcp",
		Short: "Start Niki as an MCP server over stdio",
		RunE: func(cmd *cobra.Command, args []string) error {
			reg := tools.DefaultRegistry()
			srv := mcp.NewServer(reg)
			return srv.Serve(os.Stdin, os.Stdout)
		},
	})

	var showSources bool
	printConfig := func(cfg config.ConfigWithSources, showSrc bool) {
		printField := func(section, key string, val interface{}) {
			src := cfg.Sources[section]
			if src == "" {
				src = "default"
			}
			if showSrc {
				fmt.Printf("%s.%s = %v  # source: %s\n", section, key, val, src)
			} else {
				fmt.Printf("%s.%s = %v\n", section, key, val)
			}
		}
		printField("model", "name", cfg.Model.Name)
		printField("provider", "name", cfg.Provider.Name)
		printField("provider", "env_key", cfg.Provider.EnvKey)
		printField("ui", "inline", cfg.UI.Inline)
		printField("ui", "theme", cfg.UI.Theme)
		printField("permissions", "mode", cfg.Permissions.Mode)
		printField("sandbox", "enabled", cfg.Sandbox.Enabled)
		printField("sandbox", "auto_allow", cfg.Sandbox.AutoAllow)
		printField("sandbox", "fail_if_unavailable", cfg.Sandbox.FailIfUnavailable)
		if len(cfg.MCP.Servers) == 0 {
			printField("mcp", "servers", "{}")
		} else {
			for name, srv := range cfg.MCP.Servers {
				printField("mcp", "servers."+name+".command", srv.Command)
			}
		}
	}

	configCmd := &cobra.Command{
		Use:   "config",
		Short: "Configuration management",
		Run: func(cmd *cobra.Command, args []string) {
			cfg, _ := config.LoadWithSources(configPath)
			printConfig(cfg, showSources)
		},
	}
	showCmd := &cobra.Command{
		Use:   "show",
		Short: "Show resolved configuration",
		Run: func(cmd *cobra.Command, args []string) {
			cfg, _ := config.LoadWithSources(configPath)
			printConfig(cfg, showSources)
		},
	}
	showCmd.Flags().BoolVar(&showSources, "sources", false, "Print origin source for each value")
	configCmd.Flags().BoolVar(&showSources, "sources", false, "Print origin source for each value")
	configCmd.AddCommand(showCmd)
	rootCmd.AddCommand(configCmd)

	var (
		sandboxWritableDirs []string
		sandboxReadOnly     bool
	)
	sandboxRunCmd := &cobra.Command{
		Use:   "sandbox-run [flags] -- <command> [args...]",
		Short: "Internal re-exec helper to execute a command inside the sandbox",
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return fmt.Errorf("no command specified")
			}
			backend, available := sandbox.Detect()
			cwd, _ := os.Getwd()
			writable := sandboxWritableDirs
			if len(writable) == 0 && !sandboxReadOnly {
				writable = []string{cwd}
			}
			var sb sandbox.Sandbox
			if available {
				switch backend {
				case "bubblewrap":
					sb = &sandbox.Bubblewrap{WritableDirs: writable}
				case "seatbelt":
					sb = &sandbox.Seatbelt{WritableDirs: writable}
				default:
					sb = &sandbox.Passthrough{}
				}
			} else {
				sb = &sandbox.Passthrough{}
			}
			stdout, stderr, err := sb.Run(cmd.Context(), cwd, args[0], args[1:]...)
			if stdout != "" {
				fmt.Print(stdout)
			}
			if stderr != "" {
				fmt.Fprint(os.Stderr, stderr)
			}
			if err != nil {
				if exitErr, ok := err.(*exec.ExitError); ok {
					os.Exit(exitErr.ExitCode())
				}
				return err
			}
			return nil
		},
	}
	sandboxRunCmd.Flags().StringSliceVar(&sandboxWritableDirs, "writable-dir", nil, "Directory allowed for writes")
	sandboxRunCmd.Flags().BoolVar(&sandboxReadOnly, "read-only", false, "Disallow all filesystem writes")
	rootCmd.AddCommand(sandboxRunCmd)

	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}

func userHome() string {
	h, _ := os.UserHomeDir()
	return h
}

func detectGitBranch(dir string) string {
	headPath := filepath.Join(dir, ".git", "HEAD")
	data, err := os.ReadFile(headPath)
	if err != nil {
		return ""
	}
	content := strings.TrimSpace(string(data))
	if strings.HasPrefix(content, "ref: refs/heads/") {
		return strings.TrimPrefix(content, "ref: refs/heads/")
	}
	if len(content) > 7 {
		return content[:7]
	}
	return content
}
