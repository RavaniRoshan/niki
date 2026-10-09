package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	neturl "net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"

	"github.com/RavaniRoshan/niki/internal/appserver"
	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/intent"
	"github.com/RavaniRoshan/niki/internal/logx"
	"github.com/RavaniRoshan/niki/internal/mcp"
	"github.com/RavaniRoshan/niki/internal/paths"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/recipes"
	"github.com/RavaniRoshan/niki/internal/routing"
	"github.com/RavaniRoshan/niki/internal/sandbox"
	"github.com/RavaniRoshan/niki/internal/session"
	"github.com/RavaniRoshan/niki/internal/skills"
	"github.com/RavaniRoshan/niki/internal/soak"
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

// bootMark closes the currently open span, attributing it to
// the phase that just completed (label), and opens a new span.
// Call it AFTER the work, naming the work: the trace then reads
// as truthful phase durations instead of labeling each span with
// the phase that starts next.
func bootMark(label string) {
	if paths.Env("BOOT_TRACE") == "" {
		return
	}
	now := time.Now()
	if len(bootTraceEntries) == 0 {
		bootTraceEntries = append(bootTraceEntries, bootEntry{start: bootT0})
	}
	last := len(bootTraceEntries) - 1
	bootTraceEntries[last].end = now
	bootTraceEntries[last].task = label
	bootTraceEntries = append(bootTraceEntries, bootEntry{start: now})
}

// writeBootTrace persists the boot timeline (task,
// start, end, ms) required by the boot-trace spec.
func writeBootTrace() {
	if paths.Env("BOOT_TRACE") == "" || len(bootTraceEntries) == 0 {
		return
	}
	last := len(bootTraceEntries) - 1
	if bootTraceEntries[last].end.IsZero() {
		// Drop the trailing still-open span: only completed
		// phases carry durations.
		bootTraceEntries = bootTraceEntries[:last]
	}
	dir := filepath.Join(paths.Dir(), "log")
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

// warmProviderDNS resolves the provider's host in the
// background so the first turn does not pay DNS latency.
// It opens no connections and sends no application data;
// a dead network only costs a background timeout.
func warmProviderDNS(rawURL string) {
	host := providerHost(rawURL)
	if host == "" {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	_, _ = net.DefaultResolver.LookupHost(ctx, host)
}

// providerHost extracts the hostname from a provider base
// URL, or "" when there is nothing to warm (mock/local).
func providerHost(rawURL string) string {
	if rawURL == "" {
		return ""
	}
	u, err := neturl.Parse(rawURL)
	if err != nil {
		return ""
	}
	return u.Hostname()
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
			return demoMockProvider()
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
		fmt.Fprintf(os.Stderr, "nikicode: warning: sandbox enabled but no backend available; commands run with a scrubbed environment only\n")
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
	// B0 fast path: version/help with no other args returns before any
	// command tree or heavy package is touched. It must NOT fire on
	// incidental tokens (e.g. `bench --metric version`).
	if len(os.Args) == 2 {
		switch os.Args[1] {
		case "--version", "-v", "-V", "version":
			fmt.Printf("nikicode version %s\n", version)
			return
		}
	}

	rootCmd := &cobra.Command{
		Use:     "nikicode",
		Short:   "NikiCode, a personal coding-agent harness for the terminal",
		Version: version,
		RunE: func(cmd *cobra.Command, args []string) error {
			defer writeBootTrace()
			// Canonical home + one-time legacy migration run
			// before any home state is touched (never on the
			// --version fast path, which returns in main()).
			paths.Ensure()
			// Terminal capability negotiation (L7) runs ahead
			// of the TUI: the reply round trip costs ~0.1ms
			// against a responding terminal, bounded by the
			// 25ms Detect budget against a mute one.
			type detectResult struct {
				caps terminal.Capabilities
				err  error
			}
			detectCh := make(chan detectResult, 1)
			go func() {
				caps, err := terminal.Detect(25 * time.Millisecond)
				detectCh <- detectResult{caps, err}
			}()
			bootMark("boot")
			cfg, _ := config.Load(configPath)
			bootMark("config")
			// Session store opens in the background (B10): SQLite
			// init costs ~20ms and the first frame must not wait
			// for it. The store path is resolved here; the open,
			// the session row, and the observer below all move to
			// after the engine exists.
			storePath := filepath.Join(paths.Dir(), "sessions.db")
			// Structured logging goes to a file, never
			// to the terminal while the TUI owns it (P5).
			logger, err := logx.Open(filepath.Join(paths.Dir(), "nikicode.log"))
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
			// The session store opens in the background: SQLite
			// init costs ~20ms and neither the first frame nor
			// the first turn waits for it. Events emitted before
			// the store is ready are buffered in order (capped)
			// and flushed once it opens. If the open fails, the
			// session continues in memory without persistence.
			var storeMu sync.Mutex
			var store *session.Store
			type pendingEvent struct {
				id  protocol.SessionId
				evt protocol.EngineEvent
			}
			var pending []pendingEvent
			const maxPendingEvents = 4096
			storeReady := make(chan struct{})
			go func() {
				defer close(storeReady)
				s, err := session.Open(storePath)
				storeMu.Lock()
				defer storeMu.Unlock()
				if err != nil {
					return
				}
				store = s
				eng.SetSessionStore(s)
				_ = store.CreateSession(eng.SessionID(), "tui")
				for _, p := range pending {
					_ = store.AppendEvent(p.id, p.evt)
				}
				pending = nil
			}()
			defer func() {
				<-storeReady
				storeMu.Lock()
				defer storeMu.Unlock()
				if store != nil {
					_ = store.Close()
				}
			}()
			go func() {
				if found, err := skills.Discover("."); err == nil && len(found) > 0 {
					var names []string
					for _, s := range found {
						names = append(names, s.Name)
					}
					eng.AddSystemMessage("Available skills: " + strings.Join(names, ", "))
				}
			}()
			eng.Observe(func(evt protocol.EngineEvent) {
				storeMu.Lock()
				defer storeMu.Unlock()
				if store == nil {
					if len(pending) < maxPendingEvents {
						pending = append(pending, pendingEvent{eng.SessionID(), evt})
					}
					return
				}
				_ = store.AppendEvent(eng.SessionID(), evt)
			})
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
			bootMark("startup")

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
			// Frame rate (B1/B5): bubbletea v1 flushes the first
			// frame only on a renderer tick (standard_renderer.go
			// listen()), so the tick interval directly bounds
			// first paint: 120Hz bounds it to ~8ms after program
			// start (measured: warm 17ms). Each idle tick
			// early-outs with no writes (proven: 0 idle redraws,
			// 0 bytes), but every tick still wakes the process;
			// on this WSL2 VM even an empty 120Hz Go ticker reads
			// ~2.5% CPU, and production settled idle reads 1.8%
			// (recorded deviation, owner 2026-10-08; the <1%
			// gate is held by the deterministic unit test, and
			// bare metal reads lower).
			opts = append(opts, tea.WithFPS(120))
			p := tea.NewProgram(app, opts...)
			bootMark("program")

			// Provider preconnect (B9): warm DNS for the
			// configured provider in the background. It sends
			// no application data and never blocks first paint;
			// the first turn still performs its own handshake.
			if !cfg.Provider.DisablePreconnect && paths.Env("NO_PRECONNECT") == "" {
				go warmProviderDNS(cfg.Provider.BaseURL)
			}

			if _, err := p.Run(); err != nil {
				return err
			}
			bootMark("session-run")
			eng.Stop()
			return nil
		},
	}

	rootCmd.PersistentFlags().BoolVar(&debug, "debug", false, "Enable debug logging")
	rootCmd.PersistentFlags().BoolVar(&profile, "profile", false, "Show boot profile")
	rootCmd.PersistentFlags().BoolVar(&inline, "inline", false, "Use inline terminal output instead of alternate screen")
	rootCmd.PersistentFlags().StringVar(&configPath, "config", "", "Path to nikicode.toml")

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
			// Canonical home + one-time legacy migration run
			// before any home state is touched (never on the
			// --version fast path, which returns in main()).
			paths.Ensure()
			bootMark("boot")
			cfg, _ := config.Load(configPath)
			bootMark("config")
			reg, guard, err := buildRegistry(cfg, permissions.ModeWorkspaceWrite)
			if err != nil {
				return err
			}
			bootMark("registry")
			eng, cmdChan, eventChan := engine.NewEngine(100, buildProvider(cfg), reg, guard)
			bootMark("engine")
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
							"name":       "nikicode",
							"head_sha":   detectGitBranch("."),
							"status":     "completed",
							"conclusion": "success",
							"output": map[string]any{
								"title":   "NikiCode turn execution",
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
							"name":       "nikicode",
							"head_sha":   detectGitBranch("."),
							"status":     "completed",
							"conclusion": "failure",
							"output": map[string]any{
								"title":   "NikiCode turn execution failed",
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
	rootCmd.AddCommand(newCICommand())

	// `nikicode do` routes one natural-language instruction to a recipe,
	// an explanation, or a git workflow — deterministically, with no
	// model call. Multi-step inputs run in order; corrections re-route
	// with context; undo/redo work off the action journal. Refusals
	// exit non-zero with the reason.
	doPlanOnly := false
	doCmd := &cobra.Command{
		Use:   "do [instruction]",
		Short: "Act on a natural-language instruction (routine task, explain code, git workflow)",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			paths.Ensure()
			input := strings.Join(args, " ")
			cwd, _ := os.Getwd()
			all, err := recipes.Discover(cwd)
			if err != nil {
				return err
			}
			cfg, _ := config.Load(configPath)
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
			out, err := doRun(reg, guard, all, cwd, input, doPlanOnly)
			if out != "" {
				fmt.Println(out)
			}
			return err
		},
	}
	doCmd.Flags().BoolVar(&doPlanOnly, "plan", false, "Print the plan without executing")
	rootCmd.AddCommand(doCmd)

	// `nikicode git` exposes each git workflow directly; `nikicode do`
	// reaches the same workflows through natural language.
	gitCmd := &cobra.Command{
		Use:   "git",
		Short: "Git workflows: status, commit, branch, rebase, blame, log, review, changelog, prdraft",
		RunE: func(cmd *cobra.Command, args []string) error {
			return cmd.Help()
		},
	}
	gitDir := func(cmd *cobra.Command) string {
		d, _ := cmd.Flags().GetString("dir")
		if d == "" {
			d, _ = os.Getwd()
		}
		return d
	}
	gitCmd.PersistentFlags().String("dir", "", "Repository directory (default: cwd)")
	gitCmd.AddCommand(&cobra.Command{
		Use: "status", Short: "Show working-tree status",
		RunE: func(cmd *cobra.Command, args []string) error {
			out, err := runDoGitOp(gitDir(cmd), intent.Action{Op: intent.GitStatus})
			if err != nil {
				return err
			}
			fmt.Println(out)
			return nil
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "add [paths...]", Short: "Stage paths",
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return fmt.Errorf("give paths to stage")
			}
			return git.Stage(gitDir(cmd), args...)
		},
	})
	gitCommitMsg := ""
	gitCommitCmd := &cobra.Command{
		Use: "commit", Short: "Commit staged changes (message derived from the staged diff when empty)",
		RunE: func(cmd *cobra.Command, args []string) error {
			dir := gitDir(cmd)
			msg := gitCommitMsg
			if msg == "" {
				var err error
				msg, err = git.ProposeCommitMessage(dir)
				if err != nil {
					return err
				}
			}
			if err := git.Commit(dir, msg); err != nil {
				return err
			}
			fmt.Println("committed: " + strings.SplitN(msg, "\n", 2)[0])
			return nil
		},
	}
	gitCommitCmd.Flags().StringVarP(&gitCommitMsg, "message", "m", "", "Commit message (default: derived from the staged diff)")
	gitCmd.AddCommand(gitCommitCmd)
	gitCmd.AddCommand(&cobra.Command{
		Use: "branch [create|switch] <name>", Short: "Create or switch branches",
		Args: cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			dir := gitDir(cmd)
			var err error
			switch args[0] {
			case "create":
				err = git.BranchCreate(dir, args[1])
			case "switch":
				err = git.BranchSwitch(dir, args[1])
			default:
				return fmt.Errorf("action must be create or switch")
			}
			return err
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "rebase <onto>", Short: "Rebase onto a ref (autostash; conflicts stop with a report)",
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return git.Rebase(gitDir(cmd), args[0])
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "rebase-abort", Short: "Back out of a conflicted rebase",
		RunE: func(cmd *cobra.Command, args []string) error {
			return git.RebaseAbort(gitDir(cmd))
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "rebase-continue", Short: "Continue a rebase after resolving and staging",
		RunE: func(cmd *cobra.Command, args []string) error {
			return git.RebaseContinue(gitDir(cmd))
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "blame <file:line>", Short: "Blame one line: who, when, commit summary",
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			file, line, err := parseBlameTarget(args[0])
			if err != nil {
				return err
			}
			info, err := git.Blame(gitDir(cmd), file, line)
			if err != nil {
				return err
			}
			fmt.Printf("%s:%d: %s — %s (%s, %s)\n", file, line, info.Line, info.Summary, info.Author, info.Date)
			return nil
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "log", Short: "Recent commit history",
		RunE: func(cmd *cobra.Command, args []string) error {
			out, err := runDoGitOp(gitDir(cmd), intent.Action{Op: intent.GitLog, Text: "log"})
			if err != nil {
				return err
			}
			fmt.Println(out)
			return nil
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "review", Short: "Review staged changes per file",
		RunE: func(cmd *cobra.Command, args []string) error {
			out, err := runDoGitOp(gitDir(cmd), intent.Action{Op: intent.GitReview})
			if err != nil {
				return err
			}
			fmt.Println(out)
			return nil
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "changelog", Short: "Render recent history as markdown",
		RunE: func(cmd *cobra.Command, args []string) error {
			out, err := runDoGitOp(gitDir(cmd), intent.Action{Op: intent.GitChangelog, Text: "changelog"})
			if err != nil {
				return err
			}
			fmt.Println(out)
			return nil
		},
	})
	gitCmd.AddCommand(&cobra.Command{
		Use: "prdraft", Short: "Draft a pull request as markdown (no network; saves PR_DRAFT.md)",
		RunE: func(cmd *cobra.Command, args []string) error {
			base := "main"
			if len(args) > 0 {
				base = args[0]
			}
			out, err := git.PRDraft(gitDir(cmd), base, "PR_DRAFT.md")
			if err != nil {
				return err
			}
			fmt.Println(out)
			return nil
		},
	})
	rootCmd.AddCommand(gitCmd)

	rootCmd.AddCommand(&cobra.Command{
		Use:   "serve-codex",
		Short: "Start NikiCode as a Codex App-Server over stdio",
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
		Short: "Start NikiCode as an Agent Client Protocol (ACP) server over stdio",
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
			template := `# AGENTS.md — Instructions for NikiCode

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
			rep := paths.Ensure()
			if rep.Migrated {
				fmt.Printf("✓ Home %s (migrated %d files, %d bytes from legacy %s)\n", paths.Dir(), rep.Files, rep.Bytes, rep.From)
			} else {
				fmt.Printf("✓ Home %s\n", paths.Dir())
			}
			for _, name := range []string{"BOOT_TRACE", "NO_PRECONNECT", "TRUST_PROJECT"} {
				if v, src := paths.EnvSource(name); v != "" {
					fmt.Printf("✓ Env %s=%s (via %s)\n", name, v, src)
				}
			}
			fmt.Println("✓ Go runtime")
			fmt.Println("✓ Terminal capabilities")
			fmt.Println("✓ Working directory writable")
			cfg, _ := config.Load(configPath)
			fmt.Printf("✓ Config resolved (provider=%s, model=%s)\n", cfg.Provider.Name, cfg.Model.Name)
			fmt.Printf("✓ Provider constructed: %s\n", buildProvider(cfg).Name())
			fmt.Printf("✓ MCP servers configured: %d\n", len(cfg.MCP.Servers))
			for name, srv := range cfg.MCP.Servers {
				pctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
				client := mcp.NewClient(name, srv.Command, srv.Args...)
				err := client.Start(pctx)
				cancel()
				if err != nil {
					fmt.Printf("✗ MCP %s: down (%v); continuing without it\n", name, err)
					continue
				}
				_ = client.Stop()
				fmt.Printf("✓ MCP %s: reachable\n", name)
			}
			backend, ok := sandbox.Detect()
			if ok {
				fmt.Printf("✓ Sandbox backend available: %s (enforced: read-only root, writable workspace roots, network denied, dropped capabilities)\n", backend)
			} else {
				fmt.Println("✗ Sandbox backend: none available (fallback: env-scrubbed passthrough)")
			}
			if cfg.Sandbox.Enabled {
				fmt.Printf("✓ Sandbox policy: active (%s)\n", backend)
			} else {
				fmt.Printf("ℹ Sandbox policy: disabled in config (enabled in session or via nikicode sandbox-run)\n")
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
			rep := paths.Ensure()
			home := paths.Dir()
			if rep.Migrated {
				fmt.Printf("✓ Home %s (migrated %d files, %d bytes from legacy %s)\n", home, rep.Files, rep.Bytes, rep.From)
			} else {
				fmt.Printf("✓ Home %s\n", home)
			}
			for _, name := range []string{"BOOT_TRACE", "NO_PRECONNECT", "TRUST_PROJECT"} {
				if v, src := paths.EnvSource(name); v != "" {
					fmt.Printf("✓ Env %s=%s (via %s)\n", name, v, src)
				}
			}
			store, err := session.Open(filepath.Join(paths.Dir(), "sessions.db"))
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
		Short: "Start NikiCode as an MCP server over stdio",
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

	// `nikicode bench` runs the reproducible benchmark harness (G4).
	// Every metric writes raw samples under --out; BENCH.md transcribes.
	benchMetric := ""
	benchN := 30
	benchOut := "docs/bench/raw"
	benchBin := ""
	benchKeyword := ""
	benchPrompt := "say hi"
	benchCmd := &cobra.Command{
		Use:   "bench",
		Short: "Run reproducible benchmarks (version, ttff, echo, peak, footprint, skills, turn)",
		RunE: func(cmd *cobra.Command, args []string) error {
			bin := benchBin
			if bin == "" {
				bin = os.Args[0]
			}
			metrics := []string{"version", "ttff", "echo", "peak", "footprint", "skills", "turn"}
			if benchMetric != "" {
				metrics = []string{benchMetric}
			}
			for _, m := range metrics {
				var rows []benchRow
				samples := map[string][]float64{}
				var err error
				switch m {
				case "version":
					var r benchRow
					var v []float64
					r, v, err = benchVersion(bin, benchN)
					rows, samples[m] = []benchRow{r}, v
				case "ttff":
					rows, samples, err = benchTTFF(bin, benchKeyword, benchN, 2*time.Second)
				case "echo":
					var r benchRow
					var v []float64
					r, v, err = benchEcho(bin, benchKeyword, benchN)
					rows, samples[m] = []benchRow{r}, v
				case "peak":
					var r benchRow
					var v []float64
					r, v, err = benchPeak(bin, benchPrompt, 5)
					rows, samples[m] = []benchRow{r}, v
				case "footprint":
					var r benchRow
					r, err = benchFootprint(bin)
					rows = []benchRow{r}
				case "skills":
					var r benchRow
					var v []float64
					r, v, err = benchSkillsWarm()
					rows, samples[m] = []benchRow{r}, v
				case "turn":
					var r benchRow
					var v []float64
					r, v, err = benchTurnOverhead(10)
					rows, samples[m] = []benchRow{r}, v
				default:
					return fmt.Errorf("unknown metric %q", m)
				}
				if err != nil {
					return fmt.Errorf("metric %s: %w", m, err)
				}
				path, err := writeBenchRaw(benchOut, m, rows, samples)
				if err != nil {
					return err
				}
				for _, r := range rows {
					fmt.Printf("%s %s: n=%d p50=%.2f p95=%.2f %s (raw: %s)\n",
						r.Metric, r.Binary, r.Stats.N, r.Stats.P50, r.Stats.P95, r.Unit, path)
				}
			}
			return nil
		},
	}
	benchCmd.Flags().StringVar(&benchMetric, "metric", "", "Single metric to run (default: all)")
	benchCmd.Flags().IntVar(&benchN, "n", 30, "Samples for sampled metrics")
	benchCmd.Flags().StringVar(&benchOut, "out", "docs/bench/raw", "Raw output directory")
	benchCmd.Flags().StringVar(&benchBin, "bin", "", "Binary to measure (default: this binary)")
	benchCmd.Flags().StringVar(&benchKeyword, "keyword", "", "Ready keyword for PTY metrics (empty: first paint)")
	benchCmd.Flags().StringVar(&benchPrompt, "prompt", "say hi", "Prompt for the peak-RSS exec turn")
	benchCmd.Flags().StringVar(&benchHome, "home", "", "HOME for PTY runs (default: fresh temp dir; references needing auth use their real HOME)")
	rootCmd.AddCommand(benchCmd)

	// `nikicode soak` runs the scripted all-day soak (G5): mock turns
	// with real tool calls, subagents, MCP, and hooks, sampling RSS to
	// CSV. No model spend. Growth over 15MB or any crash fails loudly.
	soakTurns := 200
	soakMCP := ""
	soakHook := ""
	soakWorkDir := ""
	soakOut := ""
	soakCmd := &cobra.Command{
		Use:   "soak",
		Short: "Run the scripted stability soak (mock turns, tools, subagents, MCP, hooks)",
		RunE: func(cmd *cobra.Command, args []string) error {
			out := soakOut
			if out == "" {
				out = fmt.Sprintf("docs/soak/soak-%s.csv", time.Now().UTC().Format("20060102-150405"))
			}
			work := soakWorkDir
			if work == "" {
				work, _ = os.Getwd()
			}
			v, err := soak.Run(soak.Config{Turns: soakTurns, MCPBin: soakMCP, HookCmd: soakHook, WorkDir: work, OutCSV: out})
			if err != nil {
				return err
			}
			fmt.Printf("soak: %d turns, %d crashes, rss %.1f -> %.1f MB (max %.1f, growth %+.1f), heap %.1f MB, goroutines %d\ncsv: %s\n",
				v.Turns, v.Crashes, v.RSSStartMB, v.RSSEndMB, v.RSSMaxMB, v.GrowthMB, v.HeapMB, v.Goroutines, v.CSV)
			if v.Crashes > 0 {
				return fmt.Errorf("soak failed: %d crashes", v.Crashes)
			}
			if v.GrowthMB > 15 {
				return fmt.Errorf("soak failed: RSS grew %+.1f MB over %d turns (leak?)", v.GrowthMB, v.Turns)
			}
			fmt.Println("SOAK PASS: no crash, no leak")
			return nil
		},
	}
	soakCmd.Flags().IntVar(&soakTurns, "turns", 200, "Engine turns to run")
	soakCmd.Flags().StringVar(&soakMCP, "mcp", "", "MCP server binary to exercise")
	soakCmd.Flags().StringVar(&soakHook, "hook", "", "Hook command to run per turn")
	soakCmd.Flags().StringVar(&soakWorkDir, "workdir", "", "Tool-call working directory (default: cwd)")
	soakCmd.Flags().StringVar(&soakOut, "out", "", "CSV output path (default: docs/soak/soak-<stamp>.csv)")
	rootCmd.AddCommand(soakCmd)

	// `nikicode surfaces` prints every user-facing string with its
	// CLAIMS.md tag. Adding a user-facing string without a tag fails
	// the claimcheck test: no probe, no claim.
	rootCmd.AddCommand(&cobra.Command{
		Use:    "surfaces",
		Short:  "Print user-facing strings with claim tags (claimcheck input)",
		Hidden: true,
		Run: func(cmd *cobra.Command, args []string) {
			printSurfaces(rootCmd, "")
		},
	})

	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}

// surfaceClaim tags a command path with its claims. Untagged paths
// emit UNTAGGED, which fails claimcheck: every user-facing string
// must trace to a proven CLAIMS.md row.
func surfaceClaim(path string) string {
	switch {
	case path == "nikicode":
		return "C1"
	case path == "nikicode do":
		return "C6 C18"
	case path == "nikicode bench":
		return "C7 C8 C9"
	case path == "nikicode soak":
		return "C11"
	case strings.HasPrefix(path, "nikicode git"):
		return "C5"
	default:
		return "C17"
	}
}

func printSurfaces(cmd *cobra.Command, prefix string) {
	path := strings.TrimSpace(prefix + " " + strings.Fields(cmd.Use)[0])
	if !cmd.Hidden || path == "nikicode surfaces" {
		fmt.Printf("cmd:%s | %s [%s]\n", path, cmd.Short, surfaceClaim(path))
	}
	for _, sub := range cmd.Commands() {
		printSurfaces(sub, path)
	}
	if path == "nikicode" {
		for _, line := range tui.SurfaceStrings() {
			fmt.Println(line)
		}
		embedded, err := recipes.Embedded()
		if err == nil {
			for _, r := range embedded {
				fmt.Printf("recipe:%s | %s [C3]\n", r.Name, r.Description)
			}
		}
	}
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
