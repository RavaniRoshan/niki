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
provider = "anthropic"            # "anthropic", "responses", "openai", "mock"
api_key = "sk-..."                # never written to disk; keychain or env_key
env_key = "ANTHROPIC_API_KEY"     # env var the key is read from
base_url = ""                     # custom OpenAI-compatible endpoint
disable_preconnect = false        # background TCP/TLS preconnect after first paint

# TUI & Terminal
[ui]
inline = false                    # commit settled rows to native scrollback
theme = "default"                 # default, dark, light, monochrome
layout = "classic"                # "workbench" (three-rail, full-screen), "classic" (default; the alias "stream" maps to it), or "ledger" (experimental)
reduced_motion = false            # static composer cursor, static sweep frames

# Approval policy
[permissions]
mode = "workspace_write"          # readonly, workspace_write, manual, full_access

# Per-run budgets (0 / "" disables a dimension)
[budgets]
wall_time = "10m"
cost_usd = 2.0
steps = 100

# Sandbox & Security
[sandbox]
enabled = false
auto_allow = true
allow_unsandboxed = true
fail_if_unavailable = false
excluded_commands = []

# Self-extension protocol
self_extend = "off"               # "off" (default) or "propose"

# MCP Servers (global or trusted project only)
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "."]
```

Inline mode prints the layout header once to native scrollback and keeps
only the live area (activity line, composer, footer) in View(); alt-screen
mode (the default) keeps the header on screen. Layout specifics: see
docs/tui/WORKBENCH.md (workbench, the three-rail full-screen layout) and
docs/ui/CLASSIC.md (classic, the default inline flow, aliased "stream").

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
