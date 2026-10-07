<div align="center">

# Niki

*A fast, terminal-native AI coding agent written in Go*

[![Go](https://img.shields.io/badge/Go-1.24+-00ADD8?style=flat-square&logo=go)](go.mod)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-blue?style=flat-square)](LICENSE)

[Features](#features) • [Quick start](#quick-start) • [Usage](#usage) • [Configuration](#configuration) • [Development](#development)

</div>

Niki is a local-first AI coding harness: a streaming agent loop, a Bubble Tea TUI, file/search/shell tools, permission tiers, skills, MCP servers, and SQLite-backed sessions — all in a single Go binary.

## Features

- **Streaming agent loop** — multi-turn turns with tool dispatch, context compaction, and interruptible cancellation
- **Bubble Tea TUI** — alternate-screen default, `--inline` scrollback mode, viewport history, live streaming deltas
- **Core tools** — `read_file`, `write_file`, `edit_file`, `grep`, `glob`, `shell` with bounded output and atomic writes
- **Permission tiers** — `readonly`, `workspace_write`, `full_access`, plus a shell command risk classifier
- **Providers** — OpenAI-compatible streaming SSE and a deterministic mock provider for tests
- **Skills & instructions** — lazy `SKILL.md` discovery plus `AGENTS.md` / `NIKI.md` hierarchy
- **MCP client** — concurrent stdio JSON-RPC servers with lifecycle states
- **Sessions** — pure-Go SQLite (`modernc.org/sqlite`, zero CGO) plus append-only JSONL event log

## Quick start

Requires Go 1.24+.

```bash
# Install from source
go install github.com/RavaniRoshan/niki/cmd/niki@latest

# Or build locally
make build
./bin/niki            # interactive TUI
./bin/niki exec "explain this repo"
./bin/niki doctor
```

Pre-built binaries for Linux/macOS/Windows are published via GitHub Releases (see the Release workflow; tags `v*` produce archives).

For real model calls, point Niki at any OpenAI-compatible endpoint:

```bash
export OPENAI_API_KEY=sk-...
./bin/niki exec "fix the failing test"
```

## Usage

```
niki [flags]
niki [command]

Commands:
  exec [prompt]        Execute a prompt and exit
  doctor               Check system health and environment
  resume [session-id]  List or replay persisted sessions
  skills               List discovered skills
  mcp                  List configured MCP servers
  config               Show resolved configuration

Flags:
  --config string   Path to niki.toml
  --debug           Enable debug logging
  --profile         Show boot profile
  --inline          Use inline terminal output instead of alternate screen
```

## Configuration

Config is layered: built-in defaults → `~/.config/niki/niki.toml` → `./niki.toml` → `--config`.

```toml
[model]
name = "gpt-4o-mini"

[provider]
name = "openai"            # or "mock"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"

[permissions]
mode = "workspace_write"   # readonly | workspace_write | full_access

[mcp.servers.myserver]
command = "my-mcp-server"
args = []
```

## Development

```bash
go vet ./...
go test ./...
go build -o bin/niki ./cmd/niki
```

Architecture and reference provenance live in [docs/architecture/references.md](docs/architecture/references.md).
