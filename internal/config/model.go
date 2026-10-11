package config

type Config struct {
	Model          ModelConfig          `toml:"model"`
	SecondaryModel SecondaryModelConfig `toml:"secondary_model,omitempty"`
	Provider       ProviderConfig       `toml:"provider"`
	UI             UIConfig             `toml:"ui"`
	Permissions    PermissionsConfig    `toml:"permissions"`
	MCP            MCPConfig            `toml:"mcp"`
	Budgets        BudgetsConfig        `toml:"budgets"`
	Sandbox        SandboxConfig        `toml:"sandbox"`
	// SelfExtend gates the self-extension protocol (T5.10):
	// "off" (default) or "propose". Anything else is invalid.
	SelfExtend string `toml:"self_extend"`
}

type SecondaryModelConfig struct {
	Provider string `toml:"provider,omitempty"`
	Model    string `toml:"model,omitempty"`
	Force    bool   `toml:"force,omitempty"`
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

// BudgetsConfig sets per-run budgets (L2): wall time, cost, and steps.
// Zero/empty disables that dimension. Defaults are set in Default()
// so the D-04 verifier backstop is active out of the box (D-09).
type BudgetsConfig struct {
	WallTime string  `toml:"wall_time"` // Go duration, e.g. "10m"; "" or "0" disables
	CostUSD  float64 `toml:"cost_usd"`  // USD per run; 0 disables
	Steps    int     `toml:"steps"`     // provider steps per run; 0 disables
}
