# NIKI Feature Gap Analysis — Internal Audit + Competitor Research

**Date:** 2026-08-06
**Depth:** Wide (internal audit + external competitor research + gap analysis)
**Target:** Identify features to adopt from OpenCode/KiloCode to make NIKI competitive end-to-end

---

## Executive Summary

NIKI has a **robust pipeline architecture** (Planner→Coder→Tester→Reviewer→Red/Blue adversarial, parallel coders, custom topologies) that exceeds most competitors. However, it **lacks modern UX expectations**: no session management, no undo/redo, no MCP server support, no custom commands, no inline autocomplete, no web server/sharing, no plugin system, and no IDE integration.

**OpenCode** leads in extensibility (plugins, skills, MCP, LSP, formatters, custom commands, themes). **KiloCode** leads in UX polish (checkpoints, browser use, inline autocomplete, enhance prompt, team features). Both have features NIKI should adopt.

**Critical gaps** (table stakes for 2026):
1. Session management (save/restore/switch)
2. Undo/redo with checkpoints
3. MCP server support (local + remote)
4. Custom slash commands
5. Multi-provider model cycling (F2-style)
6. Context condensing/compaction controls
7. IDE extensions (VS Code, JetBrains)
8. Web server for sharing/remote access

---

## Part 1: NIKI Current Feature Inventory

### 1.1 CLI Subcommands (9 commands)

| Command | Description |
|---------|-------------|
| `run` | Execute a coding task through the pipeline |
| `status` | View current/most recent task status |
| `report` | View report for a completed task |
| `config` | Initialize configuration |
| `recommend` | Recommend per-agent models |
| `dashboard` | Generate static HTML dashboard |
| `eval` | Run evaluation harness |
| `memory` | View/manage agent memory |
| `goal` | Manage persistent autonomous goals |

### 1.2 CLI Flags (`niki run`)

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `description` | String | required | Task description |
| `--project` / `-p` | PathBuf | CWD | Project path |
| `--branch` / `-b` | String | `niki/{id_short}` | Output branch |
| `--max-rounds` | u32 | config | Max revision rounds |
| `--{role}-model` | String | config | Per-agent model override (4 roles) |
| `--backend` | enum | config | docker/worktree/cloud |
| `--cloud` | bool | false | Shortcut for cloud backend |
| `--dry-run` | bool | false | Planner spec only |
| `--quiet` | bool | false | Minimal output |
| `--verbose` | bool | false | Full reasoning |
| `--tui` | bool | false | Rich terminal TUI |

### 1.3 Config File Options (`niki.toml`) — ~50+ settings

**Sections:** `[general]`, `[providers.<name>]`, `[agents.<role>]`, `[docker]`, `[pipeline]`, `[knowledge]`, `[security]`, `[parallel]`, `[red_blue]`, `[goal]`, `[ui]`

**Key capabilities:**
- 7 agent roles with per-role provider/model override
- 4 built-in LLM providers (Anthropic, OpenAI, Google, Ollama) + compatible gateways
- 3 sandbox backends (Docker, Worktree, Cloud)
- Custom pipeline topologies (user-defined stage ordering)
- Red/Blue adversarial verification (ON by default)
- Parallel coder mode with N concurrent sessions
- Security policies (per-role allow/deny command lists)
- Knowledge ingestion (doc globs + external URLs)
- Agent memory system (per-role persistent storage)

### 1.4 Environment Variables (11 vars)

`ANTHROPIC_API_KEY`, `NIKI_PROVIDERS_ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `OPENROUTER_API_KEY`, `OPENAI_API_KEY`, `GOOGLE_API_KEY`, `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `ANTHROPIC_MODEL`, `OPENAI_MODEL`, `NIKI_CLOUD_ENDPOINT`

Plus: `NO_COLOR`, `TERM`, `TERM_PROGRAM`, `TMUX`, `DOCKER_HOST`, `XDG_RUNTIME_DIR`

### 1.5 TUI Features

- 11 navigable pages (Run, Pipeline, Agents, Diff, Verdict, Cost, Artifacts, History, Config, Help, TestLog)
- Theme modes: Auto/Dark/Light with Ctrl+T cycling
- Command palette (Ctrl+P)
- Onboarding modal
- DEC 2026 synchronized output
- Input modes: Insert, Command (/), Shell (!)
- @ file autocomplete
- Full key bindings (Ctrl+A/E/W/U/K, arrows, Tab, Enter, Esc)
- Status line (branch, model, tokens, cost, elapsed)
- Streaming display with sliding window

### 1.6 Pipeline Features

- Classic: Planner → Coder → Tester → Reviewer
- Red/Blue adversarial (independent Red agent probes, Reviewer reconciles)
- Security audit stage (optional)
- Parallel coders (N concurrent, Synthesizer reconciles)
- Auto topology (single-agent fast-path for simple tasks)
- User-defined pipeline stages
- Hermetic safety proof
- Structured output (JSON schema per artifact)
- Secret redaction
- Revision loop (up to N rounds)

### 1.7 What NIKI Does Better Than Competitors

| Feature | NIKI | Competitors |
|---------|------|-------------|
| **Red/Blue adversarial** | Built-in, ON by default | Rare (Claude Code has subagent review) |
| **Parallel coders** | N concurrent + Synthesizer | Not common |
| **Custom pipeline topologies** | User-defined stage ordering | Rare |
| **Hermetic safety proof** | Pre/post repo snapshot | Not common |
| **Per-role security policies** | Allow/deny lists + timeout | Claude Code has 7 permission modes |
| **Goal runner** | Autonomous multi-iteration | Not common |
| **Eval harness** | Built-in regression testing | Not common |
| **Agent memory** | Per-role persistent | Claude Code has CLAUDE.md |

---

## Part 2: OpenCode Feature Inventory

### 2.1 Models & Providers

| Feature | Default | NIKI Equivalent |
|---------|---------|-----------------|
| 75+ LLM providers | — | ❌ NIKI has 4 + gateways |
| `small_model` for lightweight tasks | Cheaper model | ❌ Missing |
| Per-model reasoning effort, thinking budget | Varies | ❌ Missing |
| `enabled_providers` / `disabled_providers` | All detected | ❌ Missing |
| Model cycling (F2) | — | ❌ Missing |
| `/models` command | — | ❌ Missing |

### 2.2 Agents

| Feature | NIKI Equivalent |
|---------|-----------------|
| Build agent (primary) | ≈ Coder |
| Plan agent (restricted) | ≈ Planner |
| General subagent | ❌ Missing |
| Explore subagent (read-only) | ❌ Missing |
| Scout subagent (external research) | ❌ Missing |
| `default_agent` config | ❌ Missing |
| `subagent_depth` control | ❌ Missing |
| Per-agent: temperature, steps, model, prompt, permission, color | Partial (model only) |
| Tab key cycles agents | ❌ Missing |
| @mention subagents | ❌ Missing |
| Markdown agents in `~/.config/opencode/agents/` | ❌ Missing |

### 2.3 Themes

| Feature | NIKI Equivalent |
|---------|-----------------|
| 11+ built-in themes (system, tokyonight, everforest, catppuccin, gruvbox, nord, etc.) | ❌ NIKI has Dark/Light only |
| Custom themes via JSON files | ❌ Missing |
| `/themes` command | ❌ Missing |
| System theme (auto-adapts to terminal) | ❌ Missing |

### 2.4 Keybinds

| Feature | NIKI Equivalent |
|---------|-----------------|
| Leader key (Ctrl+X) | ❌ Missing |
| Vim-style input binds | Partial |
| Which-key (Ctrl+Alt+K) | ❌ Missing |
| `leader_timeout` config | ❌ Missing |

### 2.5 Permissions & Safety

| Feature | NIKI Equivalent |
|---------|-----------------|
| Per-tool allow/ask/deny | Partial (role-based) |
| Granular rules with pattern matching | Partial (command patterns) |
| `--auto` flag | ❌ Missing |
| `doom_loop` protection | ❌ Missing |
| `external_directory` control | ❌ Missing |
| `.env` protection | ❌ Missing |
| Per-agent permissions | ❌ Missing |

### 2.6 TUI Configuration

| Feature | NIKI Equivalent |
|---------|-----------------|
| `scroll_speed` | ❌ Missing |
| `scroll_acceleration` | ❌ Missing |
| `diff_style` (auto/stacked) | ❌ Missing |
| `mouse` toggle | ❌ Missing |
| `attention` (desktop notifications) | ❌ Missing |
| `sound_pack` / `volume` | ❌ Missing |

### 2.7 Slash Commands (7 built-in + custom)

| Command | NIKI Equivalent |
|---------|-----------------|
| `/init` (AGENTS.md setup) | ❌ Missing |
| `/compact` | ❌ Missing |
| `/details` | ❌ Missing |
| `/editor` | ❌ Missing |
| `/export` | ❌ Missing |
| `/models` | ❌ Missing |
| `/new` | ❌ Missing |
| `/redo` | ❌ Missing |
| `/sessions` | ❌ Missing |
| `/share` | ❌ Missing |
| `/themes` | ❌ Missing |
| `/thinking` | ❌ Missing |
| `/undo` | ❌ Missing |
| `/unshare` | ❌ Missing |
| `/connect` | ❌ Missing |
| Custom commands via config | ❌ Missing |

### 2.8 Custom Commands

| Feature | NIKI Equivalent |
|---------|-----------------|
| `command` config block | ❌ Missing |
| Markdown commands in `~/.config/opencode/commands/` | ❌ Missing |
| `$ARGUMENTS` / `$1`, `$2` positional args | ❌ Missing |
| `!` bash in commands | ❌ Missing |
| `@file` in commands | ❌ Missing |

### 2.9 MCP (Model Context Protocol)

| Feature | NIKI Equivalent |
|---------|-----------------|
| Local MCP (STDIO) | ❌ Missing |
| Remote MCP (HTTP/SSE) | ❌ Missing |
| OAuth support | ❌ Missing |
| Per-server enable/disable | ❌ Missing |
| Per-server timeout | ❌ Missing |
| `opencode mcp auth/list/logout/debug` | ❌ Missing |

### 2.10 LSP (Language Server Protocol)

| Feature | NIKI Equivalent |
|---------|-----------------|
| 30+ built-in LSP servers | ❌ Missing |
| Auto-install on detection | ❌ Missing |
| Custom LSP definitions | ❌ Missing |

### 2.11 Formatters

| Feature | NIKI Equivalent |
|---------|-----------------|
| 25+ built-in formatters (prettier, biome, gofmt, rustfmt, etc.) | ❌ Missing |
| Custom formatter definitions | ❌ Missing |
| `$FILE` placeholder | ❌ Missing |

### 2.12 Session Management

| Feature | NIKI Equivalent |
|---------|-----------------|
| SQLite-backed persistent sessions | ❌ Missing |
| `/new`, `/sessions` | ❌ Missing |
| `/undo`, `/redo` with git snapshots | ❌ Missing |
| `/compact` manual compaction | ❌ Missing |
| `compaction` config (auto, prune, reserved) | ❌ Missing |
| `snapshot` config | ❌ Missing |
| `share` config (manual/auto/disabled) | ❌ Missing |
| Session forking | ❌ Missing |
| Session export to markdown | ❌ Missing |

### 2.13 Collaboration / Sharing

| Feature | NIKI Equivalent |
|---------|-----------------|
| `/share` public link | ❌ Missing |
| `/unshare` | ❌ Missing |
| Auto-share mode | ❌ Missing |
| Web server (`opencode web`) | ❌ Missing |
| mDNS discovery | ❌ Missing |
| `opencode attach` | ❌ Missing |

### 2.14 Server / Web

| Feature | NIKI Equivalent |
|---------|-----------------|
| `server` config (port, hostname, mDNS, CORS) | ❌ Missing |
| Password protection | ❌ Missing |
| Basic auth username | ❌ Missing |

### 2.15 Plugins

| Feature | NIKI Equivalent |
|---------|-----------------|
| `plugin` config array (npm) | ❌ Missing |
| Local plugins (`.js`/`.ts`) | ❌ Missing |
| Plugin hooks (tool.execute, session, message, file, permission, lsp, tui, shell) | ❌ Missing |
| Custom tools via plugins | ❌ Missing |

### 2.16 Agent Skills

| Feature | NIKI Equivalent |
|---------|-----------------|
| `SKILL.md` files | ❌ Missing |
| Discovery paths (`.opencode/skills/`, `~/.config/opencode/skills/`) | ❌ Missing |
| `skill` permission patterns | ❌ Missing |

### 2.17 Rules & Instructions

| Feature | NIKI Equivalent |
|---------|-----------------|
| `instructions` config (file paths/globs) | Partial (knowledge.doc_globs) |
| `AGENTS.md` project instructions | ❌ Missing |
| Variable substitution (`{env:VAR}`, `{file:path}`) | ❌ Missing |

### 2.18 Image Attachments / Vision

| Feature | NIKI Equivalent |
|---------|-----------------|
| `attachment.image` config (resize, max dimensions) | ❌ Missing |

### 2.19 Shell Configuration

| Feature | NIKI Equivalent |
|---------|-----------------|
| `shell` config (pwsh, zsh, etc.) | ❌ Missing |
| `!` prefix in TUI | ✅ NIKI has this |

### 2.20 Other

| Feature | NIKI Equivalent |
|---------|-----------------|
| `autoupdate` config | ❌ Missing |
| `watcher.ignore` patterns | ❌ Missing |
| IDE integration (VS Code, Cursor, Windsurf) | ❌ Missing |
| GitHub/GitLab integration | ❌ Missing |
| 8-tier config precedence | ❌ Missing |

---

## Part 3: KiloCode Feature Inventory

### 3.1 Platforms

| Platform | NIKI Equivalent |
|----------|-----------------|
| VS Code Extension | ❌ Missing |
| JetBrains Plugin | ❌ Missing |
| CLI (Terminal) | ✅ NIKI has this |
| Cloud Agent (web) | ❌ Missing |
| Mobile Apps (iOS/Android) | ❌ Missing |
| Slack Integration | ❌ Missing |
| App Builder | ❌ Missing |
| Code Reviews (PR automation) | ❌ Missing |

### 3.2 Agents

| Agent | NIKI Equivalent |
|-------|-----------------|
| `code` (default, full access) | ≈ Coder |
| `ask` (read-only) | ❌ Missing |
| `plan` (restricted editing) | ≈ Planner |
| `debug` (full access) | ❌ Missing |
| `orchestrator` (deprecated) | ≈ Pipeline |
| Custom agents via Markdown | ❌ Missing |
| Per-agent model/temperature override | Partial (model only) |

### 3.3 Core Features

| Feature | NIKI Equivalent |
|---------|-----------------|
| Inline Autocomplete (ghost text) | ❌ Missing |
| Checkpoints (git-based snapshots) | ❌ Missing |
| Browser Use (web automation) | ❌ Missing |
| Enhance Prompt (auto-improve prompts) | ❌ Missing |
| Task & Todo Lists | ❌ Missing |
| Self-checking (agent reviews own work) | ❌ Missing |
| MCP Marketplace | ❌ Missing |
| Git Commit Generation | ❌ Missing |
| Code Actions (Explain, Fix, Improve) | ❌ Missing |
| Terminal Context Menu | ❌ Missing |

### 3.4 Configuration

| Feature | NIKI Equivalent |
|---------|-----------------|
| `kilo.jsonc` (project config) | ✅ NIKI has `niki.toml` |
| `.kilo/kilo.jsonc` (priority) | ❌ Missing |
| `~/.config/kilo/tui.jsonc` (TUI settings) | ❌ Missing |
| `~/.config/kilo/kilo.jsonc` (global) | ✅ NIKI has global config |
| Import/Export settings | ❌ Missing |

### 3.5 Key Config Options

| Option | NIKI Equivalent |
|--------|-----------------|
| `model` (provider_id/model_id) | ✅ Similar |
| `provider` settings | ✅ Similar |
| `mcp` config | ❌ Missing |
| `permission` (per-tool allow/ask/deny) | Partial |
| `instructions` (rules files) | Partial |
| `formatter` config | ❌ Missing |
| `lsp` config | ❌ Missing |
| `snapshot` (checkpoints) | ❌ Missing |
| `auto_collapse_reasoning` | ❌ Missing |
| `terminal_command_display` | ❌ Missing |
| `experimental.codebase_search` | ❌ Missing |
| `experimental.batch_tool` | ❌ Missing |
| `experimental.speech_to_text_model` | ❌ Missing |
| `remote_control` | ❌ Missing |

### 3.6 Permission System

| Feature | NIKI Equivalent |
|---------|-----------------|
| Interactive approval prompts | ❌ Missing |
| Auto-approval rules (pattern-based) | ❌ Missing |
| Glob-based permissions | ❌ Missing |
| MCP tool permissions (namespaced) | ❌ Missing |
| Sandbox (macOS/Linux filesystem/network limits) | Partial (Docker sandbox) |

### 3.7 CLI-Specific Features

| Feature | NIKI Equivalent |
|---------|-----------------|
| Interactive mode (approval prompts) | ❌ Missing |
| Autonomous mode (`--auto`) | ❌ Missing |
| Session continuation (`--continue`) | ❌ Missing |
| Remote mode (`/remote`) | ❌ Missing |
| Slash commands (`/themes`, `/editor`, `/agents`, `/connect`, `/teams`, `/remote`) | Partial (command palette) |
| OpenTelemetry export | ❌ Missing |

### 3.8 Memory & Context

| Feature | NIKI Equivalent |
|---------|-----------------|
| AGENTS.md memory bank | ❌ Missing |
| Custom rules (project/global) | ❌ Missing |
| Custom instructions (personal preferences) | ❌ Missing |
| Context mentions (file/function/symbol) | ❌ Missing |
| Context condensing (compaction model) | ❌ Missing |
| `.kilocodeignore` | ❌ Missing |
| File watcher ignore patterns | ❌ Missing |

### 3.9 Collaboration & Teams

| Feature | NIKI Equivalent |
|---------|-----------------|
| Session sharing (read-only links) | ❌ Missing |
| Teams plan (centralized billing) | ❌ Missing |
| Enterprise (SSO, audit logs) | ❌ Missing |
| Adoption dashboard | ❌ Missing |
| Organization custom modes | ❌ Missing |

### 3.10 Model Support

| Feature | NIKI Equivalent |
|---------|-----------------|
| 500+ models | ❌ NIKI has 4 providers |
| Mid-task model switching | ❌ Missing |
| Per-agent model override | ✅ NIKI has this |
| Custom model definitions | ❌ Missing |
| Bring Your Own Keys | ✅ NIKI has this |

---

## Part 4: Gap Analysis — What NIKI is Missing

### 4.1 Table Stakes (Must-Have for 2026)

| Priority | Feature | OpenCode | KiloCode | NIKI |
|----------|---------|----------|----------|------|
| **P0** | Session management (save/restore/switch) | ✅ | ✅ | ❌ |
| **P0** | Undo/redo with checkpoints | ✅ | ✅ | ❌ |
| **P0** | MCP server support (local + remote) | ✅ | ✅ | ❌ |
| **P0** | Custom slash commands | ✅ | ✅ | ❌ |
| **P0** | Multi-provider model cycling | ✅ | ✅ | ❌ |
| **P0** | Context compaction controls | ✅ | ✅ | ❌ |
| **P1** | IDE extensions (VS Code, JetBrains) | ✅ | ✅ | ❌ |
| **P1** | Web server for sharing/remote | ✅ | ✅ | ❌ |
| **P1** | Inline autocomplete | ❌ | ✅ | ❌ |
| **P1** | Checkpoints/snapshots | ✅ | ✅ | ❌ |
| **P1** | LSP integration | ✅ | ✅ | ❌ |
| **P1** | Formatters (prettier, etc.) | ✅ | ✅ | ❌ |
| **P2** | Plugin system | ✅ | ❌ | ❌ |
| **P2** | Agent skills | ✅ | ❌ | ❌ |
| **P2** | Browser use | ❌ | ✅ | ❌ |
| **P2** | Enhance prompt | ❌ | ✅ | ❌ |
| **P2** | Team/sharing features | ✅ | ✅ | ❌ |

### 4.2 Differentiators (Nice-to-Have)

| Feature | Description |
|---------|-------------|
| Voice input (speech-to-text) | KiloCode has experimental support |
| Web search | OpenCode has this |
| Codebase indexing/search | KiloCode experimental |
| Hooks/lifecycle events | OpenCode has 27 hook events |
| Scheduled execution | OpenCode has "Routines" |
| Image attachments / vision | OpenCode has config |
| Desktop notifications | OpenCode has config |
| Sound packs | OpenCode has config |

### 4.3 What NIKI Already Has (Keep)

| Feature | Notes |
|---------|-------|
| Red/Blue adversarial | Unique — keep and highlight |
| Parallel coders | Unique — keep and highlight |
| Custom pipeline topologies | Unique — keep and highlight |
| Hermetic safety proof | Unique — keep and highlight |
| Goal runner | Unique — keep and highlight |
| Eval harness | Unique — keep and highlight |
| Per-role security policies | Strong — keep |
| Agent memory | Strong — keep |
| Multiple sandbox backends | Strong — keep |
| Knowledge ingestion | Strong — keep |

---

## Part 5: Recommended Feature Adoption Plan

### Phase 1: Foundation (Table Stakes)

| Feature | Effort | Source |
|---------|--------|--------|
| Session management (save/restore/switch) | Medium | OpenCode |
| Undo/redo with git snapshots | Medium | Both |
| MCP server support (local STDIO) | High | Both |
| Custom slash commands | Low | OpenCode |
| Multi-provider model cycling (F2) | Low | OpenCode |
| Context compaction controls | Medium | Both |

### Phase 2: UX Polish

| Feature | Effort | Source |
|---------|--------|--------|
| Checkpoints UI | Medium | KiloCode |
| Inline autocomplete | High | KiloCode |
| Enhance prompt | Low | KiloCode |
| LSP integration | High | Both |
| Formatters | Medium | Both |
| More built-in themes | Low | OpenCode |

### Phase 3: Extensibility

| Feature | Effort | Source |
|---------|--------|--------|
| Plugin system | High | OpenCode |
| Agent skills | Medium | OpenCode |
| Web server / sharing | High | OpenCode |
| IDE extensions | High | Both |
| Browser use | High | KiloCode |

---

## Part 6: Disagreements & Open Questions

### Disagreements

1. **"Table stakes" classification**: What's considered essential varies by user segment. Power users expect MCP and sessions; casual users may not. The classification above represents a consensus view but is not universal.

2. **Feature counts**: OpenCode's "75+ providers" and KiloCode's "500+ models" are marketing claims that couldn't be independently verified. The actual number of working integrations may differ.

3. **OpenCode formatters "25+"**: This specific number was not confirmed in the documentation review. Treat as unverified.

### Open Questions

1. **MCP adoption priority**: How important is MCP to NIKI's target users vs. other features?
2. **IDE extension effort**: Is building VS Code/JetBrains extensions worth the investment vs. improving the TUI?
3. **Plugin system scope**: Should NIKI support a full plugin API or start with simpler hooks?
4. **Session storage**: SQLite (OpenCode approach) or file-based (simpler)?
5. **Undo granularity**: Per-message undo (OpenCode) or per-file checkpoint (KiloCode)?

---

## Part 7: Source List

| # | Source | URL | Status |
|---|---|---|---|
| 1 | OpenCode Config Docs | https://opencode.ai/docs/config/ | ✅ |
| 2 | OpenCode Agents | https://opencode.ai/docs/agents/ | ✅ |
| 3 | OpenCode Themes | https://opencode.ai/docs/themes/ | ✅ |
| 4 | OpenCode Keybinds | https://opencode.ai/docs/keybinds/ | ✅ |
| 5 | OpenCode Permissions | https://opencode.ai/docs/permissions/ | ✅ |
| 6 | OpenCode TUI | https://opencode.ai/docs/tui/ | ✅ |
| 7 | OpenCode Commands | https://opencode.ai/docs/commands/ | ✅ |
| 8 | OpenCode MCP | https://opencode.ai/docs/mcp-servers/ | ✅ |
| 9 | OpenCode LSP | https://opencode.ai/docs/lsp/ | ✅ |
| 10 | OpenCode Plugins | https://opencode.ai/docs/plugins/ | ✅ |
| 11 | OpenCode Skills | https://opencode.ai/docs/skills/ | ✅ |
| 12 | OpenCode Web | https://opencode.ai/docs/web/ | ✅ |
| 13 | OpenCode Share | https://opencode.ai/docs/share/ | ✅ |
| 14 | OpenCode IDE | https://opencode.ai/docs/ide/ | ✅ |
| 15 | OpenCode Models | https://opencode.ai/docs/models/ | ✅ |
| 16 | KiloCode Docs | https://kilo.ai/docs/ | ✅ |
| 17 | KiloCode Agents | https://kilo.ai/docs/code-with-ai/agents/using-agents | ✅ |
| 18 | KiloCode Custom Modes | https://kilo.ai/docs/customize/custom-modes | ✅ |
| 19 | KiloCode Settings | https://kilo.ai/docs/getting-started/settings | ✅ |
| 20 | KiloCode MCP | https://kilo.ai/docs/automate/mcp/using-in-kilo-code | ✅ |
| 21 | KiloCode Checkpoints | https://kilo.ai/docs/code-with-ai/features/checkpoints | ✅ |
| 22 | KiloCode Browser Use | https://kilo.ai/docs/code-with-ai/features/browser-use | ✅ |
| 23 | KiloCode Collaboration | https://kilo.ai/docs/collaborate | ✅ |
| 24 | KiloCode Context | https://kilo.ai/docs/customize/context/context-condensing | ✅ |
| 25 | NIKI Source Code | /home/shiva/projects/niki/src/ | ✅ |
