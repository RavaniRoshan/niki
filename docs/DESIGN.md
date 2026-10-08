# NIKI — Final Capability Pack Design (Phase F0)
Status: DRAFT for owner approval. Rule of proof holds: all items start UNVERIFIED.

## 1. Final Tool List & Permission Classes (Fail-Closed Defaults)
- web_search: read-only=true, safe=true, class=read, mode=disabled|cached|live|indexed. Hosted provider spec.
- web_fetch: read-only=true, safe=true, class=network, 15m TTL cache, body cap, markdown extract, hard context cap.
- view_image: read-only=true, safe=true, class=read, resize to token budget, base64 data URL, image modality check.
- notebook_edit: read-only=false, safe=false, class=workspace_write, .ipynb JSON replace|insert|delete, reset output/exec_count.
- update_plan: read-only=false, safe=false, class=read, emit PlanUpdate, <=1 in_progress, rejected in plan mode.
- todo_write: read-only=false, safe=false, class=read, atomic rewrite, session-scoped.
- tool_search: read-only=true, safe=true, class=read, exact-name fast path, select:A,B,C, mcp__ prefix, BM25 fallback.
- exec_command: read-only=false, safe=false, class=shell_exec, PTY-backed (creack/pty), process group, return pid+preview.
- write_stdin: read-only=false, safe=false, class=shell_exec, empty=poll, text=stdin, supports non-tty interrupt.
- bash_output / kill_shell: read-only=true/false, safe=true/false, class=read/shell_exec, incremental read, group SIGTERM/KILL.
- ask_user_question: read-only=true, safe=false, class=read, 1-4 questions, 2-4 options, escape hatch, unavailable in subagents.
- apply_patch: read-only=false, safe=false, class=workspace_write, hand-rolled unified patch parser, fuzzy seek, reverse apply.
- edit: read-only=false, safe=false, class=workspace_write, read-before-edit hash check, exact replace, diff preview.
- Output Capping: helper for >50k chars; saves session/tool-results/<id>.txt, returns 2 KB preview; read tool paginates.

## 2. Subagent Model & Delegation
- Verbs: spawn_agent, send_input, wait_agent, close_agent, resume_agent. Addressable by runtime ID and path (/root/worker).
- Storage: AgentGraphStore interface (parent->child edges, status, descendants).
- Context & Fork: fork_turns in {none, all, N} (default none: prompt-only child). Full-history fork prefix byte-identical.
- Isolation & Safety: git worktree from HEAD on demand (keep on commits). Propagate approval policy and sandbox posture.
- Runaway Controls: depth cap (1-3, spawn withheld at max), concurrency semaphore (~6), delegation allowlist, spend budget.
- Output & Transcript: child returns final report only; full transcript streamed to sidechain file in session dir.

## 3. Plan Mode, Checkpoints & Rewind
- Plan Mode: read-only exploration; write/exec tools withheld; update_plan rejected. User approval required to exit.
- Live Panels: plan checklist (pending ○, in_progress ▶, completed ✓) and todo list rendered from real events.
- Checkpoints: snapshot modified files keyed by turn (max 100). Guard against clobbering with per-file SHA-256 hash.
- Rewind: /rewind rolls back code, conversation, or both with pre-apply diff preview.

## 4. Memory & Compaction
- Memory: MEMORY.md pointer index (capped at 200 lines / 25 KB, line-then-byte truncate) + topic files.
- Retrieval: cheap-model side-query ranks manifest, attaches <=5 memories. Extraction runs as stop hook on no-tool turn.
- Instructions: AGENTS.md chain walked root->cwd, concatenated top-down, byte-budget capped.
- Compaction: effective = window - min(maxOutput, 20000); threshold = effective - 13000. 3 tiers: microcompact tool results,
  session-notes reuse, full forked summary. 3-failure circuit breaker and recursion guard. State persisted across compaction.

## 5. Extension Plane (Hooks, Plugins, Skills)
- Hooks: SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop, PreCompact, PostCompact, SubagentStart, SubagentStop.
  JSON on stdin; exit code 0=success, 2=blocking; stdout permissionDecision: allow|deny|ask|defer. Per-event timeouts, SHA trust.
- Plugins: manifest bundles skills, agents, hooks, MCP servers; plugin:<name>: namespace; strict-append/matcher-replace merge.
- Skills Depth: cached frontmatter index, hot reload (fsnotify) after session configured, compat .agents/skills, context: fork.

## 6. MCP Depth & Server Mode
- Depth: resources, prompts, OAuth bearer flow, reconnect loop with exponential backoff (1s->30s, Last-Event-ID resume).
- Server Mode: niki serve-mcp exposes NIKI over stdio (tools, prompts, resources) for external agents.

## 7. IDE & CI Adapters
- IDE Seam: Codex app-server (JSON-RPC stdio: thread/*, turn/*) and ACP server (JSON-RPC 2.0: session/*). Adapters over Op/Event.
- CI Flow: headless exec produces JSONL artifact; net/http posts GitHub check run with annotations and blocking conclusion.

## 8. Model Routing, Cost & Polish
- Routing: ModelProfile in TOML, fallback chain <=3 (excludes auth/invalid/overflow), subagents support cheaper model.
- Cost: per-turn Usage/Cost events, local pricing table, machine-readable headless cost, reset on /clear, optional OTLP.
- Polish: statusline (model, mode, branch, meter, cost), /agents panel, question modal, notification row, command palette.

## 9. Parity Checklist (Status: All UNVERIFIED)
- Boot and core (boot DAG, streaming, sessions, compaction) [UNVERIFIED]
- Tools (web_search, web_fetch, view_image, notebook_edit, update_plan, todo_write, tool_search, background terminals, ask_user_question, capping) [UNVERIFIED]
- Agent depth (spawn/send/wait/close, fork, worktree, limits, /agents panel) [UNVERIFIED]
- Plan and history (plan mode, checkpoints, rewind) [UNVERIFIED]
- Memory and context (MEMORY.md, retrieval, extraction, AGENTS.md chain, compaction tiers, breaker) [UNVERIFIED]
- Extensions (hooks, plugins, skills depth) [UNVERIFIED]
- MCP (resources, prompts, OAuth, reconnect, server mode) [UNVERIFIED]
- Integration (app-server, ACP, CI/GitHub) [UNVERIFIED]
- Routing, cost, polish (ModelProfile, fallback, cost/usage, statusline, palette, themes) [UNVERIFIED]
