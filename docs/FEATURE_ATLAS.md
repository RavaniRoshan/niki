# NIKI Feature Atlas

This document maps all features from reference coding agents (Codex, Kimi Code, pi, Goose, Charm Crush) to NIKI's architecture, package implementation, and current status: **shipped**, **skipped (with rationale)**, or **deferred (with tracking)**.

---

| Area | Feature | Reference Implementation | NIKI Package | Status | Rationale / Details |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **BOOT** | Fast argv path (`--version`) | Codex: rust clap fast path | `cmd/niki` | **shipped** | Bypasses Cobra and heavy imports (<7ms vs Codex 20ms). |
| **BOOT** | Parallel boot DAG | Codex: tokio async boot | `cmd/niki` | **shipped** | `errgroup`-driven boot DAG with B0–B12 stages. |
| **BOOT** | Boot timeline tracing | Codex: internal tracing | `cmd/niki` | **shipped** | `NIKI_BOOT_TRACE=1` logs timestamps to `~/.niki/log/boot-trace.log`. |
| **BOOT** | Provider preconnect | Kimi / Codex: background TLS warm | `cmd/niki` | **shipped** | Runs after first frame; opt-out via `disable_preconnect=true`. |
| **CONFIG** | Multi-layer config hierarchy | Codex: config.toml cascading | `internal/config` | **shipped** | Defaults -> global -> project -> profile -> env -> flags. |
| **CONFIG** | Named profiles | Codex: `[profiles]` in TOML | `internal/config` | **shipped** | Profile layering with `--profile <name>` or `NIKI_PROFILE`. |
| **CONFIG** | Project trust boundary | Codex: untrusted workspace sandbox | `internal/config` | **shipped** | Untrusted projects cannot register MCP servers or commands. |
| **CONFIG** | Source reporting | Codex: config inspection | `cmd/niki` | **shipped** | `niki config show --sources` outputs value + origin layer. |
| **AUTH** | Lazy API key loading | Codex: keyring / env inspection | `internal/provider` | **shipped** | Evaluates env/config during provider instantiation, not boot. |
| **AUTH** | Secret redaction | Charm Crush: secret masking | `internal/tools` | **shipped** | `RedactSecrets` strips known keys/tokens before rendering. |
| **PROVIDERS** | OpenAI Chat Completions | Codex / Aider: SSE streaming | `internal/provider` | **shipped** | `OpenAIProvider` with retry backoff and token accounting. |
| **PROVIDERS** | OpenAI Responses API | Codex: `/v1/responses` | `internal/provider` | **shipped** | `ResponsesProvider` streaming with full usage tracking. |
| **PROVIDERS** | Anthropic Messages API | Claude Code: `/v1/messages` SSE | `internal/provider` | **shipped** | `AnthropicProvider` with backoff retry on 429 and 5xx. |
| **PROVIDERS** | Custom fuzzed SSE parser | Codex: rust eventsource stream | `internal/provider` | **shipped** | `SSEScanner` handles multiline data and LLM boundary drops. |
| **PROVIDERS** | Truncated stream detection | Goose: connection drop recovery | `internal/provider` | **shipped** | Clear "stream truncated" error on premature connection drop. |
| **CONTEXT** | AGENTS.md chaining | Codex: AGENTS.md inheritance | `internal/engine` | **shipped** | Root-to-workspace markdown discovery and context injection. |
| **CONTEXT** | Automatic compaction | Codex: context token window folding | `internal/engine` | **shipped** | Golden-tested compact near token limit or via `/compact`. |
| **CONTEXT** | Skills index caching | Codex / agy: cached skill discovery | `internal/skills` | **shipped** | Cached frontmatter parser; directory mtime-based revalidation. |
| **TOOLS** | Bounded filesystem read/write | Codex: safe file access | `internal/tools` | **shipped** | `read_file`, `write_file`, `edit_file` with path checks. |
| **TOOLS** | Unified patch application | Codex: apply_patch grammar | `internal/tools` | **shipped** | `apply_patch` unified diff parser fuzzed and verified. |
| **TOOLS** | Workspace grep & glob | Codex: ripgrep / glob runner | `internal/tools` | **shipped** | `grep` and `glob` built-ins with safety guards. |
| **TOOLS** | Sandboxed shell execution | Codex: bubblewrap / seatbelt | `internal/tools`, `internal/sandbox` | **shipped** | Isolated `shell` tool routing through `niki sandbox-run`. |
| **SAFETY** | Bubblewrap OS sandbox | Codex: bwrap on Linux | `internal/sandbox` | **shipped** | Read-only root, private `/tmp`, network namespace denied. |
| **SAFETY** | Approvals (safest default) | Codex: approval prompt | `internal/permissions` | **shipped** | Focus defaults to Deny; `Esc` cancels; audit logging. |
| **SAFETY** | Red-team trap defense | Codex: prompt injection defense | `internal/permissions` | **shipped** | Symlink escape, exfiltration trap, lifecycle traps blocked. |
| **MCP** | Stdio JSON-RPC client | Codex / Goose: MCP client | `internal/mcp` | **shipped** | Namespaced tool registry, parallel boot, timeout handling. |
| **MCP** | MCP framing & parsing | Codex: JSON-RPC 2.0 framing | `internal/mcp` | **shipped** | Handles newline-delimited & Content-Length framing, fuzzed. |
| **MCP** | Catalog caching & recovery | Goose: cached tool schemas | `internal/mcp` | **shipped** | Serves cached tools when offline; cooldown on server crash. |
| **SKILLS** | SKILL.md progressive disclosure | agy / Codex: frontmatter index | `internal/skills` | **shipped** | Summary indexed at boot; body loaded on demand. |
| **SKILLS** | `.agents/skills` compatibility | OpenCode / agy compat | `internal/skills` | **shipped** | Transparent fallback lookup for `.agents/skills` path. |
| **INSTRUCTIONS** | `niki init` wizard | Codex: init helper | `cmd/niki` | **shipped** | Scaffolds standard `AGENTS.md` and `.niki/config.toml`. |
| **HOOKS** | Blocking pre-tool hooks | Codex: hook scripts | `internal/engine` | **shipped** | Pre-tool hooks can inspect arguments and reject tool call. |
| **SESSIONS** | JSONL event rollouts | Codex: rollout jsonl | `internal/session` | **shipped** | Append-only store; crash resilience against `kill -9`. |
| **TUI** | Inline mode (default) | Charm Crush: inline Bubble Tea v2 | `internal/tui` | **shipped** | Settled cells emitted to scrollback with `tea.Println`. |
| **TUI** | Live activity & mascot | Kimi Code: mascot state indicator | `internal/tui` | **shipped** | mascot driven purely by engine events (idle, thinking, tool). |
| **TUI** | Unified slash commands | Codex / Charm: slash command menu | `internal/tui` | **shipped** | Central registry (`/help`, `/doctor`, `/compact`, `/quit`). |
| **TUI** | Terminal signal restoration | Charm: termios cleanup | `internal/terminal` | **shipped** | PTY smoke tested: restores termios on Ctrl+C, SIGTERM, exit. |
| **HEADLESS** | Non-interactive `niki exec` | Codex: `codex exec` | `cmd/niki` | **shipped** | Streams prompt/events to stdout, headless execution. |
| **OBSERVABILITY** | System health diagnostics | Codex / Goose: doctor command | `cmd/niki` | **shipped** | `niki doctor` reports sandbox backend, permissions, binaries. |
| **ADVANCED** | Image inspection (`view_image`) | Codex: multimodal attachments | `internal/tools` | **deferred** | Tracked for multimodal phase; requires image terminal rendering. |
| **ADVANCED** | Subagent hierarchy | Goose / agy: subagent workers | `internal/engine` | **deferred** | Multi-agent coordination planned for Phase 2. |
| **ADVANCED** | Proprietary OAuth flows | Claude Code: browser OAuth | N/A | **skipped** | NIKI is personal: user provides direct standard API keys. |
| **ADVANCED** | Proprietary prompt formats | Claude Code / agy proprietary | N/A | **skipped** | Strictly forbidden by reference policy; clean-room only. |
