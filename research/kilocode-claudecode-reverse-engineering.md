# KiloCode + ClaudeCode — Reverse-Engineering Report for Niki

**Status:** Research complete (repo temporarily cloned, deep-analyzed, then removed per the workflow)
**Date:** 2026-08-04
**Method:** Deep research — 3 reverse-engineering subagents (KiloCode architecture, KiloCode features/UX, ClaudeCode production practices) + 1 adversarial verification subagent + 1 resolution re-query (KiloCode pickers — original claim CONFIRMED by re-query; verifier had inspected the wrong directory).
**Scope decision (user):** mine **KiloCode** (open source, upgraded fork of OpenCode — cloned into `research/tmp/kilocode` and deleted after extraction) + **ClaudeCode** (closed source — official-docs + credible third-party research only, inspiration within our rights). OpenCode skipped because KiloCode already is its fork.

---

## Executive Summary

Niki's own architecture (Rust TUI, sandboxed multi-agent pipeline, structured-output recovery, config, cost tracking, memory/knowledge) is genuinely strong and maps well onto KiloCode's design — but KiloCode and ClaudeCode each solve problems Niki hasn't yet: (1) **durable session state + resume** (Niki's `AppState.stages` is in-memory and lost on exit), (2) **context budgeting/compaction** (Niki truncates stream text at 2000 chars with no token-budget preflight), (3) **a generic per-tool permission engine** (`ask/allow/deny` + wildcard rulesets — Niki's sandbox policy is invisible), (4) **structured-error taxonomy + tool-call repair** (mirrors Niki's in-flight structured-output work), and (5) a **hooks/plugin extension surface**. The highest-value copy targets are compact Rust ports of these patterns — not the TypeScript stack.

---

## Part 1 — KiloCode Architecture (verifier: nearly all file claims CONFIRMED against the clone)

### 1.1 Repo map (confirmed present)
```
kilocode/  Turborepo + Bun workspaces
├─ packages/core        V2 runtime — durable orchestration (Effect runtime, SQLite via drizzle)
│  ├─ session/runner/{index,llm,max-steps}.ts  ← the agent loop (stream→tools→settle→recurse)
│  ├─ session/{run-coordinator,history,sql,schema,compaction,context-epoch}.ts
│  ├─ system-context/   baseline/update/removal source renderers
│  ├─ tool/  tool.ts, registry.ts, bash/edit/read/write/glob/grep/task/webfetch
│  ├─ v1/permission.ts  Action=allow|deny|ask + wildcard pattern Ruleset
│  ├─ database/  SQLite (session, message, context_epoch tables)
│  └─ config/ agent.ts, permission.ts, mcp.ts, plugin.ts, provider.ts
├─ packages/opencode    V1 application runtime (CLI/server/UI; actually drives provider turns)
│  ├─ session/{prompt,session,llm,compaction,summary}.ts
│  ├─ agent/ agent.ts (schema w/ requirements), subagent-permissions.ts, task.ts
│  ├─ permission/{evaluate,index}.ts   ask/allow/deny UI
│  ├─ provider/{provider,models,model-cache,model-status}.ts
│  ├─ mcp/ index.ts (stdio/SSE/streamable-http + OAuth) + catalog.ts
│  ├─ plugin/{loader,meta,index}.ts    hook execution
│  └─ storage/storage.ts  JSON-file session store
├─ packages/llm         provider transport: protocols/{anthropic-messages,openai-*,gemini,bedrock}.ts,
│                       route/{executor,client,endpoint}.ts, provider-error.ts
├─ packages/plugin      plugin SDK + named hooks + tui.ts event bus
├─ packages/tui         OpenTUI (SolidJS) TUI — dialog-* pickers + inline prompts
├─ packages/{kilo-sandbox,kilo-memory,kilo-telemetry,kilo-indexing}  Kilo-specific
├─ packages/{kilo-vscode,kilo-jetbrains,kilo-web-ui,kilo-console,kilo-docs}
└─ packages/{server,sdk,gateway,containers,effect-*}
```

### 1.2 Confirmed steal-worthy patterns (each cited to a real file)
1. **Permission ruleset engine** — `core/src/v1/permission.ts`: `Action=allow|deny|ask`, wildcard `pattern`, ruleset merge; every tool declares a `permission:` key (`read`, `edit`, `bash`, `shell_<id>`, `webfetch`). Niki lacks a generic ask/allow/deny gate on sandbox tools → port as Rust enum + wildcard matcher + per-agent ruleset. **Highest impact — it's the safety surface for Podman/Worktree side effects.**
2. **Durable session store** — `core/src/session/sql.ts` (SQLite `session`/`message`/`context_epoch`, JSON-typed `data`) + v1 `opencode/src/storage/storage.ts` JSON files. Niki's `AppState.stages` is in-memory and lost on exit → SQLite (`rusqlite`/`sqlx`) enables resume + real History page.
3. **Run Coordinator** — `core/src/session/run-coordinator.ts`: coalesces `run`/`wake` into ≤1 drain-chain per session, explicit-run dominance → Niki's single mpsc event loop needs this to serialize concurrent multi-agent enqueues.
4. **Hard-clamp + spill-to-file tool outputs** — `core/src/tool-output-store.ts`, `tool/registry.ts::settle()`. Niki streams transcripts into unbounded `String` (`StageInfo.stream`) → bounded projection with disk spill prevents memory blowups on multi-MB logs.
5. **System-context epochs + compaction** — `core/src/system-context/`, `context-epoch.ts`, `compaction.ts` (stable-key sources, immutable baseline per compaction, lazy admission at provider-turn boundary) → right design for Niki's 4-stage revision loop's evolving context.
6. **Subagent permission inheritance** — `opencode/src/agent/subagent-permissions.ts` (subagent gets parent's `deny` + `external_directory` + default `task`/`todowrite` denies) + `task.ts` foreground/background spawn → maps to Planner→Coder→Tester→Reviewer as scoped rulesets, background-offloadable.
7. **Provider protocol adapters + typed error classification + retry** — `packages/llm/src/{protocols,providers}`, `route/executor.ts` (retries 429/503/504/529 honoring `retry-after`, capped `MAX_DELAY_MS`, adaptive `retryDelay`), `provider-error.ts` (context-overflow classification) → single `LLMError` taxonomy for Niki's providers.
8. **Small-model routing** — `opencode/src/provider/{models,model-cache,model-status}.ts`, `kiloSmallModelPriority`, plugin hook `experimental.provider.small_model` → cheap-model routing for summary/title/compaction generation (Niki already tracks per-stage cost).
9. **Plugin/hooks extension surface** — `packages/plugin/src/index.ts` hooks: `chat.params`, `chat.headers`, `permission.ask`, `tool.execute.before/after`, `shell.env`, `command.execute.before`, `session.idle`, `session.created` → a small hooks trait for Niki lets users instrument without forking.
10. **Sandbox Backend trait** — `packages/kilo-sandbox/src/{backend,profile,bubblewrap,seatbelt,network,filesystem}.ts`: `support()`/`prepare(launch)` + network-relay/proxy abstraction → Niki's Podman/Docker/worktree wrapper gains a `Backend` trait + outbound-access relay.

### 1.3 Verifier corrections applied
- **`flock` misattributed** — `storage.ts::file()` exists (storage.ts:63) but the `EffectFlock` locking is in `opencode/src/config/config.ts` (global config read-merge-write). Corrected.
- **"v1 runs provider turns" is a working hypothesis**, not verified — files exist, but no evidence in the tree confirms which package the CLI/daemon entry executes. Carry as hypothesis; verify via bin/ entrypoints if it matters.

---

## Part 2 — KiloCode Features & UX (pickers claim RE-VERIFIED correct)

### 2.1 TUI/UX (confirmed)
- **TUI is OpenTUI (SolidJS)** at `packages/tui`. **Two UI mechanisms** (verified by re-query):
  - **App-shell picker dialogs** in `packages/tui/src/component/dialog-*.tsx` — `dialog-model`, `dialog-provider`, `dialog-agent`, `dialog-skill`, `dialog-mcp`, `dialog-theme-list`, `dialog-session-list`, `dialog-variant`, `dialog-status`, `dialog-console-org`, `dialog-move-session`, `dialog-stash`, `dialog-tag`, `dialog-session-rename`, `dialog-workspace-*`, `dialog-retry-action` — mounted from `packages/tui/src/app.tsx` via `dialog.replace(...)` keybindings (e.g. app.tsx:643 DialogModel, :688 DialogAgent, :697 DialogMcp, :731 DialogVariant, :749 DialogProviderList, :773 DialogStatus, :782 DialogThemeList, :583 DialogSessionList, :551 DialogProviderList), and reachable from the prompt menu (`component/prompt/index.tsx:52` imports DialogSkill, :565 builds the "Skills" picker).
  - **Session inline prompts** in `routes/session/{permission,question,terminal,network,suggest,sidebar,subagent-footer}.tsx`.
  - → **Niki has only one Ctrl-P palette; it lacks typed pickers (model/provider/agent/skill/theme) AND inline permission/question/terminal/network prompts.**
- CLI is a thin client over a local daemon + SDK (`kilocode/cli/cmd/run.ts`, `kilocode/daemon/`). `--auto` disables all permission prompts (README). Plan mode = `<system-reminder>` prompt injection + locked plan file (`session/prompt/plan-mode.txt`).

### 2.2 Commands / agents / skills (confirmed)
- Command schema `core/src/v1/config/command.ts`: `{template, description, agent, model, variant, subtask}`; builtins `/compact /summarize` (`kilocode/session/builtin-commands.ts`); user commands = markdown templates in `.kilo/command/*.md`.
- Agent schema `agent/agent.ts:47-71`: `name, mode(subagent|primary|all), permission(Ruleset), model, variant, prompt, requirements, steps`. Shipped: Code/Plan/Ask/Debug/Review + prompt files.
- **Distinctive `requirements` gate** (confirmed, `core/src/v1/config/agent.ts` + `agent-requirements.ts`): agents *require* skills/MCPs/VS Code extensions and block (blocked/ready/missing states) until satisfied. Niki has no capability gating.
- Skills = `SKILL.md` (frontmatter name/description); discovery scans `.kilo/`, `.claude/`, `.agents/` dirs + **remote pull** (`skill/discovery.ts` fetches index.json, cached, concurrency-limited); a `skill` tool executes them; `trusted` gating prevents shell injection (`.gitignore`-enabled `PRUNE_PROTECTED_TOOLS=["skill"]` in `compaction.ts`).

### 2.3 Context & compaction (constants CONFIRMED exact, in both v1 and v2 — they can drift)
- `session/compaction.ts`: `PRUNE_MIN=20k`, `PRUNE_PROTECT=40k`, `TOOL_OUTPUT_MAX_CHARS=2k`, recent tail + LLM summary (`compaction.txt`).
- `kilocode/session/overflow.ts` (hardened): **`FACTOR=1.3` token overcount** (under-estimation guard for code/JSON), media tokens, **preflight threshold before send** + post-step safety; `usable() = context − reserved − maxOut`.

### 2.4 Addons / plugins / integrations (confirmed)
- Plugin system (`packages/plugin`): tool/shell/tui/provider plugins; can add commands/skills/providers.
- MCP manager + marketplace (`opencode/src/mcp/`, `catalog.ts`) with streaming + paginated list.
- **Agent Manager** (`kilocode/agent-manager/`): parallel git-**worktree** sessions, per-session diff panel, per-session terminals, **PR badges via `gh`** (tracking-ref→branch→SHA), import from branches/PRs.
- **Code reviews + commit-message gen** (`kilocode/review/`, `commit-message/generate.ts`): conventional-commit typing gated on staged diff.
- **Sharing/fork**: `share: manual|auto|disabled`, `/session fork <id>`, read-only links.
- Tool suite (`kilocode/tool/`): `interactive_terminal`, `background_process`, `task`, `agent_manager`, `notebook`, `semantic_search`, `repo_overview`, `generate_image`, `send_file`, `memory_save/recall`, `model_search`, `chart`, `xlsx/ods`, `shell_heredoc`, `notify_user`.
- First-party: `kilo-gateway`, `kilo-web-ui`, `kilo-claw`, `kilo-telemetry`, `packages/sdk`, `kilo-indexing`, `kilo-memory`.

### 2.5 Structured output & errors (confirmed)
- `llm/src/schema/errors.ts`: tagged-union **error taxonomy** with `retryable` per reason — InvalidRequest (classifies `context-overflow`), NoRoute, Authentication (missing/invalid/expired/insufficient-permissions/unknown), RateLimit (`retryAfterMs`) + `HttpContext`.
- **`experimental_repairToolCall`** (`session/llm.ts:371`): on malformed tool call, (1) **case-insensitive tool-name repair** (`trim().toLowerCase()`), else (2) reroute into an `invalid` tool carrying `{tool, error}` so the model sees its own error and self-corrects → **directly copyable for Niki's structured-output recovery.**
- `llm/src/schema/options.ts`: typed `GenerationOptions` (maxTokens/temp/topP/topK/penalties/seed/stop) + `ProviderOptions`.

### 2.6 Nice-to-haves (confirmed)
- Real token/cost accounting: `kilocode/session/model-usage.ts` (`input/output/reasoning/cache.read/cache.write` + steps/cost) and `cost-propagation.ts` (recursive child-subagent cost up-propagated into parent messages, concurrency-locked).
- Inline **Suggest** (`kilocode/suggestion/`); commit-message gen; snapshot/undo (`config.snapshot`); `kilo rules add/list` persistent custom rules; 500+ models + mid-task switch + BYOK.

### 2.7 Anti-recommendations (what Niki should NOT copy)
- TypeScript/Effect/SolidJS stack; server+daemon+SDK split; cloud (gateway/claw/web-ui); account/auth; telemetry; GO-upsell dialogs.
- OpenTUI re-build — keep ratatui page model; adopt only the prompt/permission/picker **patterns**.
- effect-schema/tagged-error libraries → mirror with Rust enums/traits.
- Full MCP marketplace/workspace import-export → a basic MCP client tool layer suffices.
- Filesystem snapshot/undo → Niki's `niki/<id>` git-worktree sandbox already gives hermeticity + revert.

---

## Part 3 — ClaudeCode Production Practices (closed source; companion file on disk)

Full report: `research/claude-code-production-practices.md` (8 sub-questions, all official-docs-sourced with confidence tiers). Highlights and verified-correct framing:

1. **3-phase agent loop** (gather context → take action → verify results), fresh context window per session; continuity comes from **filesystem memory, not the window** (official docs).
2. **Two-tier memory**: `CLAUDE.md` (user-authored, scoped project/user/org, loaded verbatim; treated as context not asserted config) + **auto memory** (Claude-written, per-repo `.claude/`, shared across worktrees, first ~200 lines/25KB).
3. **Compaction** (beta `compact_20260112` — *version-fragile, do not copy the identifier literally*): `context_management` edit with `summary` + `memorized` list; **streamed** via `compaction_delta` (memorized = explicit carry-forward, anti-lost-progress).
4. **Permissions**: default/acceptEdits/plan/bypassPermissions (+ auto); session-scoped write approval; read-only free-run; repo-persisted bash grants. (Mode list is single-third-party-sourced + drifts across writeups — treat mode *naming* as medium confidence; the permission *concept* is official.)
5. **Hooks** at tool boundaries (PreToolUse/PostToolUse) + turn events + on-demand skills; skills as agent skills in `~/.claude/skills`.
6. **Subagents** via Task tool with isolated fresh context, from an explicit delegation brief; background-by-default.
7. **Session store**: continuous save + resume-by-id (`/resume`).
8. **Strict tool use + client/server tool split**; client-side cost estimate as a statusline row; theme presets incl. daltonized/ansi + custom-slug JSON + explicit settings precedence.

---

## Part 4 — Recommended Adoption List for Niki (ranked, from R2 + verifier)

| # | Adopt | Why | Where in Niki | Verification status |
|---|---|---|---|---|
| 1 | **Context budgeting + compaction** (`FACTOR=1.3`, preflight threshold, PRUNE_MIN 20k / PROTECT 40k / TOOL_OUTPUT 2k, recent tail + LLM summary) | Niki's long pipelines are this exact failure domain; currently 2k-char naive truncation | `ContextState` in AppState, shown on Cost/Run + new status line | confirmed exact constants |
| 2 | **Structured-error taxonomy + `repairToolCall`** (case-insensitive repair → `invalid` tool `{tool,error}`) | directly serves Niki's in-flight structured-output recovery | LLM/agent tool layer (`src/agents`, `src/llm`) | confirmed |
| 3 | **Markdown command + skill system** (`.kilo/command/*.md`, `SKILL.md` discovery local+remote, `skill` tool, trusted shell gating) | low-chrome, matches product parity, cheap | `src/command`, `src/skill` | confirmed |
| 4 | **Per-action permission + `--auto`** (`ask/allow/deny` wildcard rulesets, each tool declares a permission key, subagent inheritance) | unlocks CI + strengthens hermetic-auditability story; Niki sandbox policy currently invisible | `src/permission` + TUI permission overlay | confirmed (highest-impact) |
| 5 | **Durable session store + resume** (SQLite message/session tables; JSON-file fallback) | Niki's `AppState.stages` is in-memory, lost on exit; History page is fake sample rows | `src/storage` + History page | confirmed |
| 6 | **Cost propagation + real-token accounting** (`input/output/reasoning/cache.read/write` + child cost up-propagation) | differentiation; Niki already tracks cost | Cost page | confirmed |
| 7 | **Typed pickers + inline prompts** in the TUI (model/provider/agent/skill/theme dialogs; permission/question/terminal prompts) | Niki has one Ctrl-P palette only | `command_palette.rs` + new overlays | re-verified correct |
| 8 | **Plugin/hooks extension surface** (`tool.execute.before/after`, `permission.ask`, `session.idle`) | extension without forking | small hooks trait | confirmed |
| 9 | **Commit-message + PR badge** (conventional-commit gen on staged diff; `gh` PR status) | "best at" for code | Diff/History/Run rows | confirmed; needs sign-off (conflicts with hermetic git-local posture) |
| 10 | **Agent `requirements` gating** (agents require skills/MCPs/extensions, block until satisfied) | distinctive high-signal safety | Niki agent config | confirmed |

---

## Part 5 — Open Questions & Limitations (carried from verification, not dropped)

1. **KiloCode runtime ownership** (v2 `core` vs v1 `opencode`): working hypothesis that v1 drives provider turns; unverified in-tree. Decide which to model Niki's session store on before building.
2. **Generic provider failover**: retry/backoff + model-status + small-model routing confirmed, but no full "try next provider on hard error" chain found. Niki's multi-provider fallback decision depends on the answer.
3. **MCP catalog / persisted MCP auth** not fully traced; Niki's MCP tool layer needs the catalog→tool materialization path + stored-credential flow as a follow-up.
4. **Memory/knowledge injection**: `kilo-memory` capture is rule-based; exact mid-run `recall` summoning not fully traced — the reference for Niki's memory/knowledge capability.
5. **ClaudeCode mode naming** (default/acceptEdits/plan/dontAsk/bypass vs auto) is single-third-party-sourced and drifts — mark medium; concept is official, names vary.
6. **ClaudeCode `compact_20260112`** is beta + date-stamped — version-fragile; copy the *pattern*, not the identifier.
7. **Outbound GitHub (commit messages/PR badges)** conflicts with Niki's hermetic, git-local privacy posture — needs product sign-off.
8. **Session persistence prerequisite**: `/compact`, `/summarize`, resume all need the durable store (adoption #5) first — Niki's AppState is in-memory today.
9. **NO_COLOR scope** (cross-research contradiction): honor TUI-wide vs CLI-only — see also `full-product-redesign-plan.md` Part 6.

---

## Sources
- KiloCode repo (cloned `research/tmp/kilocode`, deleted after extraction): `packages/core`, `packages/opencode`, `packages/llm`, `packages/plugin`, `packages/tui`, `packages/kilo-sandbox` — cited with file paths throughout
- ClaudeCode: `research/claude-code-production-practices.md` (official docs: code.claude.com/docs, platform.claude.com/docs; third-party labeled medium)
- Existing Niki research: `research/claude-code-tui-visual-quality.md`, `research/coding-agent-structured-output-architecture.md`, `research/agent-harness-10x-improvements.md`, `research/features-fixes-tracking.md`
