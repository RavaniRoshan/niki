<div align="center">

# ⚡ NIKI

**A personal coding-agent harness in Go. One static binary. Instant to open.**

[![Go Version](https://img.shields.io/badge/Go-1.24+-00ADD8?style=for-the-badge&logo=go)](go.mod)
[![Release](https://img.shields.io/github/v/release/RavaniRoshan/niki?style=for-the-badge&color=blue)](https://github.com/RavaniRoshan/niki/releases)
[![License](https://img.shields.io/badge/License-MIT-green?style=for-the-badge)](LICENSE)
[![Startup](https://img.shields.io/badge/Startup-5.9ms-orange?style=for-the-badge)](#-performance-contracts-no-feelings-just-measurements)
[![Binary Size](https://img.shields.io/badge/Binary-19MB-purple?style=for-the-badge)](#-performance-contracts-no-feelings-just-measurements)
[![Zero CGO](https://img.shields.io/badge/CGO-Disabled-success?style=for-the-badge)](#-architecture--principles)

<br/>

```bash
# Install instantly with Go
go install github.com/RavaniRoshan/niki/cmd/niki@latest

# Or download the pre-compiled binary from GitHub Releases
curl -sSL https://raw.githubusercontent.com/RavaniRoshan/niki/main/install.sh | bash
```

[Quick Start](#-quick-start) • [Installation](#-installation-guide) • [Performance](#-performance-contracts-no-feelings-just-measurements) • [Security & Sandbox](#-security--sandbox-by-default) • [Architecture](#-architecture--principles) • [Documentation](docs/)

</div>

---

## 🌟 Why NIKI?

Most modern coding agents are hundreds of megabytes of Node, Electron, or heavy runtimes that take seconds to boot, consume hundreds of megabytes of RAM just sitting idle, and execute unchecked commands directly on your workstation.

**NIKI is different.** It was built from the ground up for developers who demand complete ownership, uncompromising speed, and verifiable safety:

* 🚀 **Instant Boot**: Boots to an interactive frame in **5.9 ms** and outputs `--version` in **6.8 ms** (almost 3x faster than Codex).
* 🪶 **Featherweight**: Consumes just **13.1 MB** of idle RSS in a single **19 MB** static binary with zero runtime dependencies.
* 🛡️ **Safe by Default**: Sandboxed execution through unprivileged **Bubblewrap** (`bwrap`) with a read-only root, private `/tmp`, network namespace denial, dropped capabilities, and an approval prompt that defaults to **Deny**.
* 🖥️ **Inline Terminal UI**: Custom Bubble Tea v2 inline viewport that commits finalized turns directly to your native terminal scrollback with `tea.Println`—no virtual scroll stutter, no UI thrashing.
* 🔌 **Unblocked Extensibility**: Background parallel Model Context Protocol (MCP) startup, mtime-cached skills discovery (148x speedup), and `.agents/skills` compatibility.
* 🧠 **Multi-Provider Engine**: Native streaming support for Anthropic Claude (`/messages`), OpenAI Responses API (`/v1/responses`), ChatCompletions, and local models (Ollama, vLLM) with automatic exponential backoff on 429/5xx and truncated stream recovery.
* 💾 **Crash Resilience**: Atomic JSONL rollouts allow instant recovery from abrupt mid-turn termination (`kill -9`) with zero lost context.

---

## 📊 Performance Contracts: No Feelings, Just Measurements

Measured on host (`AMD Ryzen 7 4800H`, `Linux WSL2`, `go1.27.1`) using the included PTY probe harness (`tools/ttff`) and `hyperfine 1.20.0`. Every number is real and stored in [`docs/PERF.md`](docs/PERF.md) and [`perf/budgets.toml`](perf/budgets.toml):

| Tool / Agent | `--version` | TTFP (First Paint) | Input Ready | Idle Memory (2s) | Pre-Prompt Bytes | Binary Size |
| :--- | ---:| ---:| ---:| ---:| ---:| ---:|
| **Codex CLI** (`0.152.1`, Rust) | 20.0 ms | 23.4 ms | 23.4 ms | 21.7 MB | 88 B | 244 MB |
| **Google agy** (`1.3.1`, Go) | 19.3 ms | 778.5 ms | 778.5 ms | 225.5 MB | 7 B | 202 MB |
| **Kimi Code** (`2.1.1`, TS/Node) | 188.4 ms | 1253.7 ms | 1253.7 ms | 391.6 MB | 7 B | 75 MB |
| **NIKI** (`0.1.0`, Go) | **6.8 ms** | **5.9 ms** | **5.9 ms** | **13.1 MB** | **16 B** | **19 MB** |
| *Contract Budget* | *≤ 20.0 ms* | *≤ 23.4 ms* | *≤ 23.4 ms* | *≤ 40.0 MB* | *minimized* | *< 25 MB* |

> **Startup Victory**: NIKI boots to first frame **3.96x faster** than Codex, consumes **39% less memory**, and compiles to a binary **12.8x smaller**.

---

## 🚀 Quick Start

### 1. Set Your Model API Key
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# or
export OPENAI_API_KEY="sk-..."
```

### 2. Scaffold Your Project
Initialize your workspace with an `AGENTS.md` instructions hierarchy and a `.niki/` directory:
```bash
niki init
```

### 3. Verify Health & Environment
Run the built-in doctor command to check sandbox capabilities, provider credentials, and terminal state:
```bash
niki doctor
```

### 4. Start Pair Programming
```bash
# Launch interactive inline TUI
niki

# Or run non-interactive headless tasks
niki exec "Audit the repo for open ports and write a summary to ports.md"
```

---

## 📦 Installation Guide

### System Requirements
* **Operating Systems**: Linux (x86_64, arm64), macOS (Apple Silicon & Intel), Windows (x86_64).
* **Sandboxing (Linux)**: Bubblewrap (`bwrap`) is recommended for container-grade unprivileged command isolation.
  ```bash
  # Debian/Ubuntu
  sudo apt-get install bubblewrap

  # Fedora / RHEL
  sudo dnf install bubblewrap

  # Arch Linux
  sudo pacman -S bubblewrap
  ```

---

### Option 1: Pre-built Binaries (GitHub Releases)

Download pre-compiled static binaries directly from the [Releases](https://github.com/RavaniRoshan/niki/releases) page:

```bash
# Linux (amd64)
curl -L https://github.com/RavaniRoshan/niki/releases/latest/download/niki_linux_amd64.tar.gz | tar -xz
sudo mv niki /usr/local/bin/

# macOS (Apple Silicon arm64)
curl -L https://github.com/RavaniRoshan/niki/releases/latest/download/niki_darwin_arm64.tar.gz | tar -xz
sudo mv niki /usr/local/bin/

# macOS (Intel amd64)
curl -L https://github.com/RavaniRoshan/niki/releases/latest/download/niki_darwin_amd64.tar.gz | tar -xz
sudo mv niki /usr/local/bin/
```

---

### Option 2: Go Install
If you have Go 1.24+ installed on your system:
```bash
go install github.com/RavaniRoshan/niki/cmd/niki@latest
```
Ensure your `$GOPATH/bin` or `$HOME/go/bin` is in your `$PATH`.

---

### Option 3: Build from Source
Build a static, stripped binary in under 5 seconds:
```bash
git clone https://github.com/RavaniRoshan/niki.git
cd niki
go build -ldflags="-s -w" -o bin/niki cmd/niki/main.go
sudo cp bin/niki /usr/local/bin/niki
```

---

## 🔒 Security & Sandbox by Default

A coding model with arbitrary shell access is an inherent risk. NIKI implements defense-in-depth from day one:

```
[Untrusted Tool Request]
          │
          ▼
┌───────────────────────────────────────────────┐
│         Permission Approval Guard             │
│  - Safest option (DENY) focused by default    │
│  - Esc key immediately aborts execution       │
│  - Untrusted project configs blocked          │
│  - All decisions logged to audit trail        │
└──────────────────────┬────────────────────────┘
                       │ Approved
                       ▼
┌───────────────────────────────────────────────┐
│     Bubblewrap Sandbox (`niki sandbox-run`)   │
│  - Read-only root filesystem (`/`)            │
│  - Isolated private in-memory `/tmp`          │
│  - Network namespace dropped (`--unshare-net`)│
│  - Linux capabilities stripped (`--cap-drop`) │
│  - Bounded strictly to writable workspace     │
└───────────────────────────────────────────────┘
```

* **Default Deny**: Interactive permission requests always focus **Deny** by default. Hitting `Esc` cancels immediately.
* **Red-Team Verified**: Proven resistance against exfiltration traps (`README.md` curl payloads), symlink directory traversal escapes, and hostile package lifecycle scripts. Tested in [`internal/permissions/redteam_test.go`](internal/permissions/redteam_test.go).
* **Project Trust Boundary**: Untrusted repositories cannot start MCP servers or execute hooks from local `.niki/config.toml` files unless explicitly declared in your global `trusted_projects` list.

Read more in [`docs/SECURITY.md`](docs/SECURITY.md).

---

## 🛠️ Built-In Tool Suite

NIKI ships with an uncompromised, zero-fluff set of core coding tools:

| Tool | Purpose | Safety Guarantees |
| :--- | :--- | :--- |
| `read_file` | Read file contents with line ranges | Bounded size limits, path sanitization |
| `write_file` | Atomic write file replacement | Temporary file swap, prevents corruption |
| `edit_file` | Targeted string replacement | Requires exact match, rejects ambiguous edits |
| `apply_patch` | Unified diff patch applicator | Fuzzed parser (>400k iterations with 0 panics) |
| `glob` | Find files matching wildcard patterns | Bounded directory walking, ignore awareness |
| `grep` | Fast regex content search | Read-only concurrency, skip binary files |
| `shell` | Execute shell commands in workspace | Bubblewrap isolation, network denial, dropped caps |

---

## ⌨️ Slash Commands & Terminal Controls

Within the interactive inline TUI, control NIKI effortlessly:

| Slash Command | Action |
| :--- | :--- |
| `/help` | Display active keybindings, tools, and slash commands |
| `/doctor` | Run comprehensive system and sandbox diagnostics |
| `/compact` | Force context compaction, folding past turn history |
| `/model` | Inspect active model, context window, and token prices |
| `/clear` | Clear visible terminal viewport |
| `/quit` | Cleanly restore terminal termios state and exit |

### Keyboard Shortcuts
* `Enter` — Send message or execute command.
* `Esc` — Interrupt ongoing model stream or deny approval prompt.
* `Ctrl+C` — Cleanly cancel current turn and restore terminal state.
* `Ctrl+Z` — Suspend process to background.

---

## ⚙️ Configuration & Profiles

NIKI merges 6 hierarchical layers: `Defaults` → `~/.niki/config.toml` → `.niki/config.toml` → `[profiles.<name>]` → `Environment Variables` → `CLI Flags`.

Inspect your effective configuration and see exactly where each setting originates:
```bash
niki config show --sources
```

Example configuration (`~/.niki/config.toml`):
```toml
# Model settings
model = "claude-3-5-sonnet"
provider = "anthropic"            # "anthropic", "responses", "openai"

# Performance
disable_preconnect = false        # Warm TLS handshake in background
boot_trace = false                # Write boot timeline to ~/.niki/log/boot-trace.log

# Sandbox & Safety
sandbox_backend = "bubblewrap"
deny_network = true
writable_roots = ["."]
trusted_projects = ["/home/user/projects/trusted"]

# Profiles (switch via `niki --profile fast`)
[profiles.fast]
model = "gpt-4o-mini"
provider = "openai"

[profiles.deep]
model = "claude-3-7-sonnet"
provider = "anthropic"

# Model Context Protocol (MCP)
[mcp.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "."]
required = false
startup_timeout_sec = 10
tool_timeout_sec = 30
```

Read the full configuration guide in [`CONFIG.md`](CONFIG.md).

---

## 📖 Deep-Dive Documentation

* 📐 [Architecture & Dependency Graph](docs/ARCHITECTURE.md) — Package DAG, acyclic invariants, and design principles.
* ⚡ [Performance Ledger](docs/PERF.md) — Inittrace audit, pprof analysis, runtime traces, and PGO benchstat data.
* 🗺️ [Feature Atlas](docs/FEATURE_ATLAS.md) — Comprehensive feature comparison vs Codex, Kimi, pi, and Goose.
* 🛡️ [Security Posture](docs/SECURITY.md) — Bubblewrap sandbox model, trust boundaries, and red-team defenses.
* 📜 [Third-Party & Attributions](THIRD_PARTY.md) — Dependency licenses, attributions, and clean-room provenance statement.
* 🐶 [Dogfooding Journal](docs/DOGFOOD.md) — Real self-hosting sessions, bugs caught, and ergonomic friction logs.

---

## 🤝 Philosophy & Clean-Room Commitment

NIKI is dedicated to personal autonomy, performance, and transparency:
1. **Rule of Proof**: "Works" means a real test or measurement ran and its output was verified.
2. **Zero Proprietary Code**: No leaked, decompiled, or reconstructed proprietary source was ever accessed or copied.
3. **Small and Boring**: Dependencies pass strict admission checks. Zero framework sprawl.

---

## 📄 License

Licensed under the [MIT License](LICENSE).
Copyright (c) 2026 Roshan Ravani.
