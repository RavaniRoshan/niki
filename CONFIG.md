# NIKI Configuration Guide (`CONFIG.md`)

NIKI uses a layered configuration system designed for predictable overrides, profile switching, and strict security sandboxing.

---

## 1. Configuration Precedence Layers

When NIKI loads configuration, values are merged in order from lowest to highest precedence:

1. **Built-in Defaults**: Hardcoded, reliable fallbacks (`gpt-4o`, inline TUI mode, sandbox enabled).
2. **Global Configuration**: `~/.niki/config.toml` (system-wide user preferences).
3. **Project Configuration**: `.niki/config.toml` (workspace-specific project settings).
4. **Active Profile Layer**: `[profiles.<name>]` sections selected via `--profile` or `NIKI_PROFILE`.
5. **Environment Variables**: `NIKI_*` variables (e.g. `NIKI_MODEL`, `NIKI_BOOT_TRACE`, `NIKI_NO_PRECONNECT`).
6. **Command-Line Flags**: Explicit CLI flags (e.g. `--model`, `--profile`).

To inspect effective configuration values along with their source layer, run:
```bash
niki config show --sources
```

---

## 2. Configuration Schema & Example

```toml
# Model & Provider
model = "claude-3-5-sonnet"
provider = "anthropic"            # "anthropic", "responses", "openai"
api_key = "sk-..."                # or set ANTHROPIC_API_KEY / OPENAI_API_KEY

# Performance & Boot
disable_preconnect = false        # set true to disable background TCP/TLS preconnect
boot_trace = false                # or set NIKI_BOOT_TRACE=1 to write boot timeline

# TUI & Terminal
mode = "inline"                   # "inline" (default) or "fullscreen"
reduced_motion = false            # disables mascot and spinner animations

# Sandbox & Security
sandbox_backend = "bubblewrap"    # "bubblewrap" or "fallback"
deny_network = true               # blocks outbound network in command sandbox
writable_roots = ["."]            # paths writable by sandboxed commands
trusted_projects = ["/home/user/projects/trusted"]

# Profiles
[profiles.fast]
model = "gpt-4o-mini"
provider = "openai"

[profiles.deep]
model = "claude-3-7-sonnet"
provider = "anthropic"

# MCP Servers (global or trusted project only)
[mcp.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "."]
required = false
startup_timeout_sec = 10
tool_timeout_sec = 30
```

---

## 3. Project Trust Boundary

For defense-in-depth, project-level configurations (`.niki/config.toml`) cannot register arbitrary MCP servers or commands unless the project path is explicitly listed in `trusted_projects` in `~/.niki/config.toml`.

- **Untrusted Projects**: Any `.niki/config.toml` in an unlisted directory will have its `[mcp.*]` sections ignored.
- **Trusted Projects**: Full capabilities enabled. To mark a project trusted:
  ```toml
  # In ~/.niki/config.toml
  trusted_projects = ["/home/shiva/projects/niki"]
  ```

---

## 4. Sandbox Configuration

The command execution sandbox is configured to isolate shell tool runs:

| Field | Description | Default |
| :--- | :--- | :--- |
| `sandbox_backend` | Isolation mechanism: `bubblewrap` (`bwrap`) or `fallback`. | `bubblewrap` |
| `deny_network` | Disables network access inside sandbox (`--unshare-net`). | `true` |
| `writable_roots` | Slice of filesystem paths where write operations are permitted. | `["."]` |
| `bwrap_path` | Custom path to the `bwrap` binary if not in `$PATH`. | `/usr/bin/bwrap` |

To verify sandbox status and capabilities on your current system:
```bash
niki doctor
```
