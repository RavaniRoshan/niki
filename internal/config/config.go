package config

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/pelletier/go-toml/v2"

	"github.com/RavaniRoshan/niki/internal/paths"
)

func Default() Config {
	return Config{
		Model: ModelConfig{Name: ""},
		SecondaryModel: SecondaryModelConfig{
			Provider: "",
			Model:    "",
			Force:    false,
		},
		Provider:    ProviderConfig{Name: "", EnvKey: ""},
		UI:          UIConfig{Inline: false, Theme: "default"},
		Permissions: PermissionsConfig{Mode: "workspace_write"},
		MCP:         MCPConfig{Servers: map[string]MCPServer{}},
		Sandbox: SandboxConfig{
			Enabled:           false,
			AutoAllow:         true,
			AllowUnsandboxed:  true,
			FailIfUnavailable: false,
		},
	}
}

// DefaultMock returns a test/demo configuration utilizing the deterministic mock provider.
func DefaultMock() Config {
	cfg := Default()
	cfg.Model.Name = "gpt-4o-mini"
	cfg.SecondaryModel = SecondaryModelConfig{
		Provider: "mock",
		Model:    "gpt-4o-mini",
		Force:    false,
	}
	cfg.Provider = ProviderConfig{Name: "mock", EnvKey: ""}
	return cfg
}

// AutoDetectProvider checks standard environment variables and returns a detected ProviderConfig and ModelConfig,
// or empty configs if unconfigured.
func AutoDetectProvider() (ProviderConfig, ModelConfig) {
	if k := os.Getenv("ANTHROPIC_API_KEY"); k != "" {
		return ProviderConfig{
			Name:   "anthropic",
			APIKey: k,
			EnvKey: "ANTHROPIC_API_KEY",
		}, ModelConfig{Name: "claude-3-5-sonnet-latest"}
	}
	if k := os.Getenv("OPENAI_API_KEY"); k != "" {
		return ProviderConfig{
			Name:   "openai",
			APIKey: k,
			EnvKey: "OPENAI_API_KEY",
		}, ModelConfig{Name: "gpt-4o"}
	}
	if k := os.Getenv("OPENROUTER_API_KEY"); k != "" {
		return ProviderConfig{
			Name:    "openai",
			BaseURL: "https://openrouter.ai/api/v1",
			APIKey:  k,
			EnvKey:  "OPENROUTER_API_KEY",
		}, ModelConfig{Name: "anthropic/claude-3.5-sonnet"}
	}
	if k := os.Getenv("DEEPSEEK_API_KEY"); k != "" {
		return ProviderConfig{
			Name:    "openai",
			BaseURL: "https://api.deepseek.com",
			APIKey:  k,
			EnvKey:  "DEEPSEEK_API_KEY",
		}, ModelConfig{Name: "deepseek-chat"}
	}
	if k := os.Getenv("GEMINI_API_KEY"); k != "" {
		return ProviderConfig{
			Name:    "openai",
			BaseURL: "https://generativelanguage.googleapis.com/v1beta/openai/",
			APIKey:  k,
			EnvKey:  "GEMINI_API_KEY",
		}, ModelConfig{Name: "gemini-2.0-flash"}
	}
	if paths.Env("MOCK") != "" || paths.Env("DEMO_TOUR") != "" {
		return ProviderConfig{Name: "mock"}, ModelConfig{Name: "gpt-4o-mini"}
	}
	return ProviderConfig{}, ModelConfig{}
}

// Load resolves config from defaults -> user (~/.config/nikicode/nikicode.toml,
// with ~/.config/niki/niki.toml as legacy fallback) -> project
// (./nikicode.toml, ./niki.toml legacy) -> explicit path overlay.
// Where both spellings exist, the nikicode spelling wins.
func Load(explicitPath string) (Config, error) {
	cfg, err := LoadWithSources(explicitPath)
	return cfg.Config, err
}

// ConfigWithSources pairs the resolved config with the origin of each section.
type ConfigWithSources struct {
	Config
	Sources map[string]string // section -> source path or "default"
}

// LoadWithSources records which layer supplied each TOML section (C1).
func LoadWithSources(explicitPath string) (ConfigWithSources, error) {
	return LoadWithProfile(explicitPath, "")
}

// LoadWithProfile layers defaults -> user -> profile -> project -> explicit overlay.
func LoadWithProfile(explicitPath, profileName string) (ConfigWithSources, error) {
	out := ConfigWithSources{Config: Default(), Sources: map[string]string{}}

	candidates := []string{"defaults"}
	if home, err := os.UserHomeDir(); err == nil {
		// Legacy spellings first so the canonical ones win.
		candidates = append(candidates, filepath.Join(home, ".config", "niki", "niki.toml"))
		candidates = append(candidates, filepath.Join(home, ".config", "nikicode", "nikicode.toml"))
		if profileName != "" {
			// Canonical home only: legacy profiles migrate
			// into ~/.nikicode on first boot (see paths).
			candidates = append(candidates, filepath.Join(home, ".niki", profileName+".config.toml"))
			candidates = append(candidates, filepath.Join(paths.Dir(), profileName+".config.toml"))
			candidates = append(candidates, filepath.Join(home, ".config", "niki", "profiles", profileName+".toml"))
			candidates = append(candidates, filepath.Join(home, ".config", "nikicode", "profiles", profileName+".toml"))
		}
	}
	candidates = append(candidates, "niki.toml")
	candidates = append(candidates, "nikicode.toml")
	if explicitPath != "" {
		candidates = append(candidates, explicitPath)
	}

	for _, path := range candidates {
		if path == "defaults" {
			for _, s := range []string{"model", "secondary_model", "provider", "ui", "permissions", "mcp", "sandbox"} {
				out.Sources[s] = "default"
			}
			continue
		}
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		var probe map[string]interface{}
		if err := toml.Unmarshal(data, &probe); err != nil {
			return out, err
		}
		// Project trust check (B3 / EXEC safety):
		// An untrusted project cannot start MCP servers or hooks from its own config.
		isProjectConfig := (path == "niki.toml" || path == "nikicode.toml" || !filepath.IsAbs(path))
		trusted := !isProjectConfig || IsProjectTrusted(".")
		if isProjectConfig && !trusted {
			if _, hasMCP := probe["mcp"]; hasMCP {
				delete(probe, "mcp")
				out.Sources["mcp"] = "blocked: untrusted project config"
			}
		}

		for section := range probe {
			out.Sources[section] = path
		}
		if err := toml.Unmarshal(data, &out.Config); err != nil {
			return out, err
		}
		if isProjectConfig && !trusted {
			// Ensure untrusted project MCP servers are not loaded
			out.MCP = Default().MCP
		}
	}
	if out.Provider.Name == "" || out.Model.Name == "" {
		detectedProv, detectedModel := AutoDetectProvider()
		if out.Provider.Name == "" && detectedProv.Name != "" {
			out.Provider = detectedProv
			out.Sources["provider"] = "auto-detected from environment"
		}
		if out.Model.Name == "" && detectedModel.Name != "" {
			out.Model = detectedModel
			out.Sources["model"] = "auto-detected default"
		}
	}
	return out, nil
}

// IsProjectTrusted reports whether the project directory is trusted to start MCP servers and hooks.
func IsProjectTrusted(projectDir string) bool {
	if paths.EnvIs("TRUST_PROJECT", "1", "true") {
		return true
	}
	trustedFile := filepath.Join(paths.Dir(), "trusted_projects")
	data, err := os.ReadFile(trustedFile)
	if err != nil {
		return false
	}
	absDir, _ := filepath.Abs(projectDir)
	for _, line := range strings.Split(string(data), "\n") {
		if line := strings.TrimSpace(line); line != "" && line == absDir {
			return true
		}
	}
	return false
}

// IsConfigured reports whether a real model provider and credentials (or local ollama/mock) are available.
func (c Config) IsConfigured() bool {
	if c.Provider.Name == "" || c.Model.Name == "" {
		return false
	}
	if c.Provider.Name == "ollama" {
		return true
	}
	if c.Provider.Name == "mock" && (paths.Env("MOCK") != "" || paths.Env("DEMO_TOUR") != "") {
		return true
	}
	return c.ResolveAPIKey() != ""
}

// ResolveAPIKey returns the key from config, env, or the configured env var name.
func (c Config) ResolveAPIKey() string {
	if c.Provider.APIKey != "" {
		return c.Provider.APIKey
	}
	if c.Provider.EnvKey != "" {
		if v := os.Getenv(c.Provider.EnvKey); v != "" {
			return v
		}
	}
	switch c.Provider.Name {
	case "anthropic":
		return os.Getenv("ANTHROPIC_API_KEY")
	case "openai":
		return os.Getenv("OPENAI_API_KEY")
	case "openrouter":
		return os.Getenv("OPENROUTER_API_KEY")
	case "deepseek":
		return os.Getenv("DEEPSEEK_API_KEY")
	default:
		return ""
	}
}

// SaveUserConfig persists configuration to the canonical user config file (~/.config/nikicode/nikicode.toml).
func SaveUserConfig(cfg Config) error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	dir := filepath.Join(home, ".config", "nikicode")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	path := filepath.Join(dir, "nikicode.toml")
	data, err := toml.Marshal(cfg)
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}

