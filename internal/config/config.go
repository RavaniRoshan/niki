package config

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/pelletier/go-toml/v2"
)

func Default() Config {
	return Config{
		Model:       ModelConfig{Name: "gpt-4o-mini"},
		Provider:    ProviderConfig{Name: "mock", EnvKey: "OPENAI_API_KEY"},
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

// Load resolves config from defaults -> user (~/.config/niki/niki.toml) -> project (./niki.toml) -> explicit path overlay.
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
		candidates = append(candidates, filepath.Join(home, ".config", "niki", "niki.toml"))
		if profileName != "" {
			candidates = append(candidates, filepath.Join(home, ".niki", profileName+".config.toml"))
			candidates = append(candidates, filepath.Join(home, ".config", "niki", "profiles", profileName+".toml"))
		}
	}
	candidates = append(candidates, "niki.toml")
	if explicitPath != "" {
		candidates = append(candidates, explicitPath)
	}

	for _, path := range candidates {
		if path == "defaults" {
			for _, s := range []string{"model", "provider", "ui", "permissions", "mcp", "sandbox"} {
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
		isProjectConfig := (path == "niki.toml" || !filepath.IsAbs(path))
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
	return out, nil
}

// IsProjectTrusted reports whether the project directory is trusted to start MCP servers and hooks.
func IsProjectTrusted(projectDir string) bool {
	if os.Getenv("NIKI_TRUST_PROJECT") == "1" || os.Getenv("NIKI_TRUST_PROJECT") == "true" {
		return true
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return false
	}
	trustedFile := filepath.Join(home, ".niki", "trusted_projects")
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
	return os.Getenv("OPENAI_API_KEY")
}
