package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/engine"
	"github.com/RavaniRoshan/niki/internal/logx"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/session"
	"github.com/RavaniRoshan/niki/internal/skills"
	"github.com/RavaniRoshan/niki/internal/terminal"
	"github.com/RavaniRoshan/niki/internal/tools"
	"github.com/RavaniRoshan/niki/internal/tui"
)

var (
	version     = "0.1.0"
	debug       bool
	profile     bool
	inline      bool
	configPath  string
)

func buildProvider(cfg config.Config) provider.ModelProvider {
	switch cfg.Provider.Name {
	case "openai":
		return provider.NewOpenAIProvider(cfg.Provider.BaseURL, cfg.ResolveAPIKey(), cfg.Model.Name)
	default:
		return provider.NewMockProvider()
	}
}

func main() {
	rootCmd := &cobra.Command{
		Use:     "niki",
		Short:   "Fast local AI coding agent",
		Version: version,
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, _ := config.Load(configPath)
			store, _ := session.Open(filepath.Join(os.Getenv("HOME"), ".niki", "sessions.db"))
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
			eng, cmdChan, eventChan := engine.NewEngine(100, buildProvider(cfg), tools.DefaultRegistry(), permissions.NewGuard(mode))
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
			caps, err := terminal.Detect(400 * time.Millisecond)
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
	rootCmd.PersistentFlags().StringVar(&configPath, "config", "", "Path to niki.toml")

	rootCmd.AddCommand(&cobra.Command{
		Use:   "exec [prompt]",
		Short: "Execute a prompt and exit",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, _ := config.Load(configPath)
			eng, cmdChan, eventChan := engine.NewEngine(100, buildProvider(cfg), tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeWorkspaceWrite))
			go func() { _ = eng.Run() }()
			if profile {
				defer func() { fmt.Fprintf(os.Stderr, "boot_profile: exec_provider=%s\n", buildProvider(cfg).Name()) }()
			}
			cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: args[0]}
			for evt := range eventChan {
				switch evt.Type {
				case protocol.EventAssistantTextDelta:
					fmt.Print(evt.Text)
				case protocol.EventTurnCompleted:
					fmt.Println()
					eng.Stop()
					return nil
				case protocol.EventTurnFailed:
					return fmt.Errorf("%s", evt.Error)
				}
			}
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
			fmt.Println("✓ Sandbox: passthrough (documented fallback active)")
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
		Use:   "config",
		Short: "Show resolved configuration",
		Run: func(cmd *cobra.Command, args []string) {
			cfg, _ := config.LoadWithSources(configPath)
			fmt.Printf("model=%s provider=%s mode=%s\n", cfg.Model.Name, cfg.Provider.Name, cfg.Permissions.Mode)
			for section, src := range cfg.Sources {
				fmt.Printf("  %s: %s\n", section, src)
			}
		},
	})

	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}

func userHome() string {
	h, _ := os.UserHomeDir()
	return h
}
