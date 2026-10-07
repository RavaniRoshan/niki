package config

type Config struct {
	Model       ModelConfig       `toml:"model"`
	Provider    ProviderConfig    `toml:"provider"`
	UI          UIConfig          `toml:"ui"`
	Permissions PermissionsConfig `toml:"permissions"`
	MCP         MCPConfig         `toml:"mcp"`
}

type ModelConfig struct {
	Name string `toml:"name"`
}

type ProviderConfig struct {
	Name    string `toml:"name"` // openai, mock
	BaseURL string `toml:"base_url"`
	APIKey  string `toml:"api_key"`
	EnvKey  string `toml:"env_key"` // env var to read the key from
}

type UIConfig struct {
	Inline bool   `toml:"inline"`
	Theme  string `toml:"theme"`
}

type PermissionsConfig struct {
	Mode string `toml:"mode"` // readonly, workspace_write, full_access
}

type MCPConfig struct {
	Servers map[string]MCPServer `toml:"servers"`
}

type MCPServer struct {
	Command string   `toml:"command"`
	Args    []string `toml:"args"`
}
