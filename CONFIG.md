# NikiCode Configuration Guide (`CONFIG.md`)

NikiCode uses a layered configuration system designed for predictable overrides, profile switching, and strict security sandboxing.

> G1 rename note: the canonical home is `~/.nikicode` (migrated one-time
> from legacy `~/.niki`), the project file is `nikicode.toml` (`niki.toml`
> still honored), instruction files are `AGENTS.md` / `NIKICODE.md`
> (`NIKI.md` still honored), and env vars are `NIKICODE_*` (`NIKI_*`
> still honored). The §2 schema example is under accuracy review in G6.

---

## 1. Configuration Precedence Layers

When NikiCode loads configuration, values are merged in order from lowest to highest precedence (later layers win; `nikicode config show --sources` prints each value's source):

1. **Built-in Defaults**: hardcoded fallbacks (see `config.Default()`).
2. **Global Configuration**: `~/.config/niki/niki.toml` (legacy), then `~/.config/nikicode/nikicode.toml` (canonical wins).
3. **Profile Layer** (API only, no CLI flag yet): `<home>/<profile>.config.toml` and `<xdg>/profiles/<profile>.toml`, legacy spellings first.
4. **Project Configuration**: `./niki.toml` (legacy), then `./nikicode.toml` (canonical wins). An untrusted project cannot register MCP servers (§3).
5. **Explicit File**: `--config <path>` overlay.
6. **Environment Variables**: provider keys via the configured `EnvKey` (e.g. `OPENAI_API_KEY`); NikiCode flags via `NIKICODE_*` with `NIKI_*` fallback (`BOOT_TRACE`, `NO_PRECONNECT`, `TRUST_PROJECT`). `nikicode doctor` reports which spelling fired.
7. **Command-Line Flags**: `--config <path>` today (`--model`/`--profile` selectors are not implemented; profiles load via API only).

To inspect effective configuration values along with their source layer, run:
```bash
nikicode config show --sources
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

For defense-in-depth, project-level configurations (`nikicode.toml`, or legacy `niki.toml`) cannot register arbitrary MCP servers or commands unless the project path is explicitly listed in `trusted_projects` in `~/.nikicode/` (migrated from legacy `~/.niki/`).

- **Untrusted Projects**: Any project TOML in an unlisted directory will have its `[mcp.*]` sections ignored.
- **Trusted Projects**: Full capabilities enabled. To mark a project trusted:
  ```toml
  # In ~/.nikicode/trusted_projects (one path per line)
  /home/shiva/projects/niki
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
nikicode doctor
```
