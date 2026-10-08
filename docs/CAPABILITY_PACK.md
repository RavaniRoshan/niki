# NIKI Final Capability Pack: Codex and Claude Code Parity

## Principles
P1–P10 hold from the foundation pack.
- **P11 Fail-closed tools:** Every tool's read-only and concurrency-safe flags default to false; every call passes the permission gate before it runs.
- **P12 Isolation by default:** A subagent starts from a fresh context unless inheritance is explicitly requested; a fork must keep its request prefix byte-identical or not fork at all.
- **P13 One core, many transports:** The TUI, the app-server, the ACP server, and headless `exec` are adapters over the same Op/Event stream; the core imports none of them.
- **P14 The plan is data, not prose:** Plan, todo, checkpoint, and memory state is structured, persisted, rendered from real events, and never invented.

## Tool Catalog
1. `web_search`: hosted provider tool (disabled|cached|live|indexed)
2. `web_fetch`: net/http GET, HTTP→HTTPS upgrade, 15m cache, body cap, Markdown conversion, hard cap
3. `view_image`: decode, resize to token budget unless detail=original, base64 data URL, gated on image modality
4. `notebook_edit`: .ipynb structured JSON edit (replace|insert|delete), clears outputs, resets execution_count
5. `update_plan`: structured plan update emitting PlanUpdate event, at most one in_progress, rejected in Plan mode
6. `todo_write`: whole-list atomic rewrite, session-scoped
7. `tool_search`: exact-name fast path, select:A,B,C direct load, mcp__ prefix, BM25 keyword matching
8. `exec_command`: PTY-backed background process manager
9. `write_stdin`: process input / polling / interrupt
10. `bash_output` / `kill_shell`: incremental or full output, process group kill
11. `ask_user_question`: structured multi-question prompt with options and escape hatch
12. `apply_patch`: patch parser for model patch grammar (unified diffs, reverse application, fuzz seek)
13. `edit`: exact search-replace with read-before-edit verification and diff preview
14. `output_capping`: cross-cutting helper for >50k chars with file persistence and preview

## Subagent Model
- Family: `spawn_agent`, `send_input`, `wait_agent`, `close_agent`, `resume_agent`
- Identity: runtime ID + hierarchical canonical path (`/root/worker`)
- Graph: `AgentGraphStore` interface
- Context: explicit per spawn (`none`, `all`, `N`; default `none`)
- Worktree isolation: git worktree from HEAD on request
- Runaway controls: depth limit (1–3), concurrency semaphore (~6), delegation allowlist, token/spend budget
- Safety: propagate non-interactive approval policy and always-allow rules
- UI: `/agents` panel

## Plan and Checkpoints
- Plan mode: read-only exploration state, withholding write/exec tools, approval required to exit
- Checkpoints: snapshot files before edits, keyed by turn, hash verification
- Rewind: `/rewind` rolls back files and/or conversation with preview

## Memory and Context Depth
- Memory: `MEMORY.md` index (<=200 lines / 25 KB cap) + topic files, retrieval side-query, end-of-turn extraction, background consolidation
- Instructions: `AGENTS.md` root-to-cwd chain
- Compaction: two-layer threshold, three tiers (microcompact, notes reuse, full fork summary), circuit breaker

## Extension Plane
- Hooks: shared event vocabulary, command hooks with JSON stdin and structured stdout/exit codes, timeouts, SHA-256 trust gating
- Plugins: bundle skills, agents, hooks, MCP servers
- Skills depth: hot reload, compat paths (.agents/skills), `context: fork`

## MCP Depth and Integration
- MCP depth: resources, prompts, OAuth bearer token flow, reconnect loop with backoff
- Server mode: expose NIKI as an MCP server over stdio
- IDE seam: Codex app-server adapter and ACP adapter
- CI: headless `exec` with JSONL artifact, GitHub check run posting

## Model Routing and Cost
- ModelProfile layered from TOML with fallback chain (max 3), non-fallback error filtering
- Usage and cost events, local pricing table, machine-readable output in headless mode
- Polish: statusline, notification row, command palette, token-driven themes
