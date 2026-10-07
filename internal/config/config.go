package config

import (
	"os"
	"path/filepath"

	"github.com/pelletier/go-toml/v2"
)

func Default() Config {
	return Config{
		Model:       ModelConfig{Name: "gpt-4o-mini"},
		Provider:    ProviderConfig{Name: "mock", EnvKey: "OPENAI_API_KEY"},
		UI:          UIConfig{Inline: false, Theme: "default"},
		Permissions: PermissionsConfig{Mode: "workspace_write"},
		MCP:         MCPConfig{Servers: map[string]MCPServer{}},
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
	out := ConfigWithSources{Config: Default(), Sources: map[string]string{}}

	candidates := []string{"defaults"}
	if home, err := os.UserHomeDir(); err == nil {
		candidates = append(candidates, filepath.Join(home, ".config", "niki", "niki.toml"))
	}
	candidates = append(candidates, "niki.toml")
	if explicitPath != "" {
		candidates = append(candidates, explicitPath)
	}

	for _, path := range candidates {
		if path == "defaults" {
			for _, s := range []string{"model", "provider", "ui", "permissions", "mcp"} {
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
		for section := range probe {
			out.Sources[section] = path
		}
		if err := toml.Unmarshal(data, &out.Config); err != nil {
			return out, err
		}
	}
	return out, nil
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
