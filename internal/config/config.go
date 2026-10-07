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
	cfg := Default()

	candidates := []string{}
	if home, err := os.UserHomeDir(); err == nil {
		candidates = append(candidates, filepath.Join(home, ".config", "niki", "niki.toml"))
	}
	candidates = append(candidates, "niki.toml")
	if explicitPath != "" {
		candidates = append(candidates, explicitPath)
	}

	for _, path := range candidates {
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		if err := toml.Unmarshal(data, &cfg); err != nil {
			return cfg, err
		}
	}
	return cfg, nil
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
