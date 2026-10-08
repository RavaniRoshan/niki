package config

type Config struct {
	Model       ModelConfig       `toml:"model"`
	Provider    ProviderConfig    `toml:"provider"`
	UI          UIConfig          `toml:"ui"`
	Permissions PermissionsConfig `toml:"permissions"`
	MCP         MCPConfig         `toml:"mcp"`
	Sandbox     SandboxConfig     `toml:"sandbox"`
}

type ModelConfig struct {
	Name      string   `toml:"name"`
	Fallbacks []string `toml:"fallbacks,omitempty"`
}

type ProviderConfig struct {
	Name              string `toml:"name"` // openai, mock
	BaseURL           string `toml:"base_url"`
	APIKey            string `toml:"api_key"`
	EnvKey            string `toml:"env_key"` // env var to read the key from
	DisablePreconnect bool   `toml:"disable_preconnect"`
}

type UIConfig struct {
	Inline bool   `toml:"inline"`
	Theme  string `toml:"theme"`
	// ReducedMotion disables non-essential animation
	// (U8): a static cursor instead of a blinking one.
	ReducedMotion bool `toml:"reduced_motion"`
}

type PermissionsConfig struct {
	Mode string `toml:"mode"` // readonly, workspace_write, full_access
}

type MCPConfig struct {
	Servers map[string]MCPServer `toml:"servers"`
}

// SandboxConfig controls process containment for
// shell commands (S1). Defaults mirror Claude
// Code's: the sandbox is opt-in, sandboxed shell
// commands are auto-allowed, an explicit
// unsandboxed escape hatch exists, and a missing
// backend degrades to a warning rather than a
// hard failure.
type SandboxConfig struct {
	Enabled           bool     `toml:"enabled"`
	AutoAllow         bool     `toml:"auto_allow"`
	AllowUnsandboxed  bool     `toml:"allow_unsandboxed"`
	FailIfUnavailable bool     `toml:"fail_if_unavailable"`
	ExcludedCommands  []string `toml:"excluded_commands"`
}

type MCPServer struct {
	Command string   `toml:"command"`
	Args    []string `toml:"args"`
}
