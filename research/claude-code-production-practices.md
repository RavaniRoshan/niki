# Claude Code Production Practices — Research for Niki

Research: how Claude Code works in production across 8 dimensions, and what to steal for **Niki** (Rust multi-agent coding TUI).

Method: primary source = official docs (`code.claude.com/docs`, `platform.claude.com/docs`); tertiary credibility checks from blogs/Reddit. Claude Code is closed-source, so this is documented-interface research, not source archaeology. Complements (does not re-derive):
- `claude-code-tui-visual-quality.md` — TUI rendering engine + verified clay-orange theme identity. Q8 only extends theme on top of it.
- `coding-agent-structured-output-architecture.md` — 3-layer recovery stack + synthetic-tool trick. Q6 only adds official tool-use/structured-output lines.

---

## Q1 — Agent loop, with-memory-without-losing-the-plot, compaction

**Question:** How does Claude Code structure the main agent loop, keep long-running context coherent across turns, and manage compaction without losing the plot?

**Findings:**
- The agentic loop is three phases: **gather context → take action → verify results**, repeated until the user's goal is met. This is the documented mental model for turn structure, not a hidden primitive. (source: https://code.claude.com/docs/en/how-claude-code-works)
- Each session starts from a **fresh context window**. Before the first prompt, Claude Code loads the prompt pipeline: CLAUDE.md memory, skills, MCP servers, and settings/rules. There is deliberately no persistent context carried in from a prior session's tokens — continuity comes from filesystem memory files, not from the window. (source: https://code.claude.com/docs/en/context-window)
- Two-tier memory model:
  - **CLAUDE.md** — user-authored; scoped project / user / org; loaded verbatim every session into the prompt pipeline. Treated as *context*, not asserted config — to actually block behavior you need a PreToolUse hook, not a CLAUDE.md rule.
  - **Auto memory** — Claude writes; stored per-repo in `.claude/`, shared across worktrees; loaded every session, first ~200 lines or 25KB.
  (source: https://code.claude.com/docs/en/memory)
- **Compaction** (beta, "compact" edits): a `context_management` edit named `compact_20260112` lets you supply custom summarization instructions that run when the conversation is compacted. The edit event carries a `summary` and a `memorized` list. (source: https://platform.claude.com/docs/en/build-with-claude/compaction)
- Compaction is **streamed**: the model receives `compaction` blocks (with `summary`+`memorized`) and mid-conversation `compaction_delta` to keep refining the summary as the window grows. (source: https://platform.claude.com/docs/en/build-with-claude/compaction)
- Compaction prompt memory is scoped how user chooses; the "memorized" concept (explicitly carried-forward statements vs. compressed rest) is the anti-lost-progress mechanism. Community confirmation that this is the known pain point; people publish CLAUDE.md recipes to mitigate context loss across compaction. (source: https://www.reddit.com/r/ClaudeAI/comments/1lne5b1/i_built_a_claude_code_context_loss_keeping_memory_system_how_to_prevent/)
- Third-party guides treat CLAUDE.md as the durable cross-session carrier and recommend curating it as "how to work" instructions + project facts. (source: https://medium.com/@samuelhemergenjr/claude-code-claude-md-memory-guide-3686a16b6464)

**Confidence:**
- High — memory tiers, fresh context, 3-phase loop are first-party documented and stable.
- Medium — exact auto-compaction trigger threshold/percentage is deliberately undocumented (closed source); the beta `compact_20260112` edit behavior may change.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Two-tier memory (CLAUDE.md persistent vs. auto mid-session) | Implement project-opinion file (committed, every session) + worktree-shared auto-memory file; load both at session start | https://code.claude.com/docs/en/memory |
| Fresh window per session, continuity via files | Don't try to persist token context across sessions; snapshot to files, reload on resume | https://code.claude.com/docs/en/context-window |
| Streaming compaction with explicit "carry-forward" list | On compaction, surface a dedicated carried-forward-facts section the agent must preserve, not raw truncation | https://platform.claude.com/docs/en/build-with-claude/compaction |
| Three-phase turn contract (gather→act→verify) | Structure the Rust main-loop turn as explicit gather/act/verify stages; surface verification failure as a real state | https://code.claude.com/docs/en/how-claude-code-works |
| First ~200 lines/25KB auto-memory head | When loading auto-memory, bias toward the most-recent/important head before eviction | https://code.claude.com/docs/en/memory |

**Disagreements:**
- "Auto memory loads every session" vs. "fresh window each session" appear to contradict; resolution: freshness applies to conversation/context, memory files are *re-injected* as context — so behavior is fresh, context is deterministic. Both statements hold simultaneously.
- CLAUDE.md marketed as behavior control, but docs explicitly say it's context (non-enforcing). Treat user-facing docs language ("rules") as marketing — Niki should implement hard constraints as enforcements, not prompt text.

**Open questions:**
- Exact compaction trigger (token % or absolute) — undocumented.
- Whether `compact_20260112` behavior is stable across releases.
- How much "memorized" survives across repeated compactions vs. being recomputed each time.

---

## Q2 — Permissions & safety

**Question:** How does Claude Code's tiered permission system protect the user, and how should Niki model approval/deny/persistence?

**Findings:**
- Tiered permission model: **read-only tools** (file read, glob, grep) require **no approval** within the working directory + `additionalDirectories`. Shell commands need approval unless they hit the built-in read-only allowlist. File modifications and writes require approval. (source: https://code.claude.com/docs/en/permissions)
- Permission persistence scoping (bash "always allow"): choosing **"Yes, don't ask again"** writes an entry to `.claude/settings.local.json` at the git repo root, resolved through worktrees. Latest version still suggests repo-root-git resolution for the settings.local.json path. Permanent per (repo + command) until removed. (source: https://code.claude.com/docs/en/permissions)
- File-modification approvals are **not permanent**: they last only until the session ends. Only bash "always allow" entries persist to disk. (source: https://code.claude.com/docs/en/permissions)
- Permission decisions are themselves hookable: the `PermissionRequest` hook event lets external code approve/deny interactively or programmatically. (source: https://code.claude.com/docs/en/hooks)
- Five operating modes via `defaultMode`/keybinding, third-party documented: **default, acceptEdits (auto-approve file edits), plan, dontAsk (auto-approve everything), bypassPermissions**. Shift+Tab cycles normal → auto-accept-edits → plan. (source: https://claudefa.st/en/blog/ClaudeCode-modes-explained)
- Read-only allowlists include common commands like `ls`, `cat`, `head`, `tail` — enforced so the agent can gather context without prompt spam, matching the "read-only = no prompt" tier. (source: see permissions page allowlist section; corroborated https://claudefa.st/en/blog/ClaudeCode-modes-explained)

**Confidence:**
- High — tiered model, session-scoped file approvals, bash persistence path, hook interception are first-party.
- Medium — the precise built-in read-only command list isn't fully enumerable from docs alone (it's the allowlist) — treat as approximate.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Read-only tools auto-approved, blind read-only command allowlist | Free-run read/grep/glob + a curated safe-command set with zero prompts | https://code.claude.com/docs/en/permissions |
| "Don't ask again" persists per repo+command to a local settings file | Persist grants into a workspace-local config resolved through worktrees, permanent per command | https://code.claude.com/docs/en/permissions |
| File-edit approvals session-scoped (not persisted) | Keep write/edit approvals TTL'd to the session; never persist file mutations | https://code.claude.com/docs/en/permissions |
| PermissionRequest is hookable | Expose an approval/deny event so Niki can have policy hooks replace point decisions | https://code.claude.com/docs/en/hooks |
| 3 quick modes via Shift+Tab | Implement a keybind-cycled degraded trust mode (default → auto-accept-edits → plan-only) | https://claudefa.st/en/blog/ClaudeCode-modes-explained |

**Disagreements:**
- Third-party blogs list modes slightly differently from each other; treat "don't ask" vs "bypass" as near-synonyms and don't over-model the distinction.
- Docs say "read-only needs no approval" but the read-only allowlist is code-curated; Niki shouldn't assume its own allowlist and Claude's match.

**Open questions:**
- Exact contents of the read-only command allowlist.
- Whether permit-only-per-session now (vs older permanent) applies to all write tools uniformly.

---

## Q3 — Subagents & parallel context isolation

**Question:** How does Claude Code scope subagent context, share/isolate state, and parallelize background work?

**Findings:**
- Subagents get a **fresh, isolated context window**: they do not see the parent's conversation, the skills the parent invoked, or the files the parent has already read. (source: https://code.claude.com/docs/en/sub-agents)
- The parent composes a **manual delegation summary** as the subagent's input — isolation means the main loop must explicitly summarize what the subagent needs to know. This is the mechanism for "with-memory" delegation. (source: https://code.claude.com/docs/en/sub-agents)
- Non-fork subagent initial context is **not** the full Claude Code system prompt: it receives its own system prompt plus environment/context details. (source: https://code.claude.com/docs/en/sub-agents)
- Two execution modes:
  - **Foreground**: blocks the main conversation; permission prompts pass through to the primary.
  - **Background**: runs concurrently with the main loop. v2.1.186+ surfaces background-subagent permission prompts in the main session (named after the subagent); before that they were auto-denied. v2.1.198+ subagents run in background by default.
  (source: https://code.claude.com/docs/en/sub-agents)
- Programmatic subagents (Agent SDK): `AgentDefinition` supports `description`, custom `prompt`, `tools` restrictions, and per-agent model override. Tools field constrains what a subagent can call. (source: https://code.claude.com/docs/en/agent-sdk/subagents)
- Lifecycle visibility: `TaskCreated` / `TaskCompleted` hook events (plus `SubagentStart`/`SubagentStop`) let the host observe creation/completion of tasks. (source: https://code.claude.com/docs/en/hooks)
- Subagent-autonomy guidance: delegate as "here's the task, report back" with enough isolated context; avoid making subagents rich in shared state — the parent holds the summary. (source: https://code.claude.com/docs/en/sub-agents)

**Confidence:**
- High — isolation, delegation-summary model, foreground/background split, SDK knobs are first-party.
- Medium — exact scheduling/parallelism internals of background mode are closed-source; behavior is documented at the capability level.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Fresh isolated context per subagent; parent composes delegation summary | Spawn each Niki subagent on a clean context built from an explicit task brief, not inherited parent state | https://code.claude.com/docs/en/sub-agents |
| Background mode with surfaced permission prompts (named) | Run concurrent subagents in background; route their permission asks to the main session labeled with the subagent id | https://code.claude.com/docs/en/sub-agents |
| Tools restriction + model override per AgentDefinition | Give Niki subagents a tool allowlist and per-agent model override at spawn | https://code.claude.com/docs/en/agent-sdk/subagents |
| TaskCreated/TaskCompleted lifecycle events | Emit lifecycle events for every spawned task so the TUI/host can render and record task completion | https://code.claude.com/docs/en/hooks |
| Non-fork = slim, not full system prompt | Don't hand subagents the entire main-agent system prompt; give a trimmed subagent system prompt + env details | https://code.claude.com/docs/en/sub-agents |

**Disagreements:**
- "Subagents run background by default (v2.1.198+)" vs. earlier foreground-first models — behavior changed across versions; Niki should make background the default but expose foreground as an explicit mode, since version-dependent defaults are a moving target.

**Open questions:**
- How background subagents are scheduled (thread pool vs. process) — undocumented internals.
- Actual token cost per subagent spawn.

---

## Q4 — Hooks & extension points

**Question:** What lifecycle hooks and extension mechanisms does Claude Code expose, and which should Niki have?

**Findings:**
- Hook lifecycle granularity: **once per session** (`SessionStart`, `SessionEnd`), **once per turn** (`UserPromptSubmit`, `Stop`, `StopFailure`), and **every tool call** (`PreToolUse`, `PostToolUse`). `EndConversation` skips both Pre/Post tool hooks. (source: https://code.claude.com/docs/en/hooks)
- Full event list includes: `Notification`, `MessageDisplay`, `PermissionRequest`, `SubagentStart`, `SubagentStop`, `TaskCreated`, `TaskCompleted`, `ConfigChange`, `InstructionsLoaded`, `CwdChanged`, `FileChanged`, `DirectoryAdded`, `WorktreeCreate`, `WorktreeRemove`. (source: https://code.claude.com/docs/en/hooks)
- Two transport types: **command hooks** (shell commands, JSON event delivered on stdin) and **HTTP hooks** (POST the event body). Both receive structured data; hooks can block or transform tool calls. (source: https://code.claude.com/docs/en/hooks)
- **Skills** (open-standard, agentskills.io): a SKILL.md is loaded *only when used*, unlike CLAUDE.md which is always loaded; this is the on-demand-context extension. (source: https://code.claude.com/docs/en/skills)
- **Custom commands are merged into skills**: `.claude/commands/deploy.md` and `.claude/skills/deploy/SKILL.md` both register `/deploy`. Frontmatter controls who can invoke a skill and whether it needs an explicit user request. (source: https://code.claude.com/docs/en/skills)
- MCP: extended-scope hierarchy top-down **plugin > connector > local > project > user** (actually the higher-up takes precedence) and servers declared at user/project/local scopes; `claude mcp add` scopes a server; `MCP_TIMEOUT` env controls server timeouts; Tool Search + **deferred/on-demand loading** of tool definitions to save tokens on big servers. (source: https://code.claude.com/docs/en/mcp, https://code.claude.com/docs/en/mcp-figma-design-tools)
- The full extension layer = CLAUDE.md + Skills + Code intelligence (LSP tool behavior) + MCP + Subagents + Agent teams + Hooks. (source: https://code.claude.com/docs/en/features-overview)
- Programmatic MCP over the SDK: `platform.claude.com` MCP connector docs show enterprise server conncection outside the SDK too. (source: https://platform.claude.com/docs/en/agents-and-tools/mcp-connector)

**Confidence:**
- High — hook events/granularity, skills merge, MCP scoping/deferred loading are first-party and concrete.
- Medium — MCP scope-precedence phrasing is easy to misquote; confirm direction mentally (most-specific wins) before coding Niki equivalents.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| PreToolUse/PostToolUse around every tool call | Add before/after hooks at each tool boundary for logging, mutation tracking, and policy checks | https://code.claude.com/docs/en/hooks |
| Turn lifecycle events (UserPromptSubmit / Stop / StopFailure) | Emit turn-start/turn-end/failure events so the TUI can drive dirty-flag redraw + error surface | https://code.claude.com/docs/en/hooks |
| On-demand skill loading vs. always-loaded memory | Load skills' SKILL.md only on first use; keep committed memory always-on — different cost profiles | https://code.claude.com/docs/en/skills |
| Deferred/MCP tool-search loading | For large MCP-style tool sets, load definitions on demand rather than bundling all at once | https://code.claude.com/docs/en/mcp |
| PermissionRequest hookable | Route permission decisions through a hook so Niki policy can overrule default handling | https://code.claude.com/docs/en/hooks |

**Disagreements:**
- MCP scope phrasing ("plugin > connector > local > project > user" precedence) is easy to misremember; Niki should define precedence explicitly and document direction rather than copy.

**Open questions:**
- Whether hooks can truly edit tool *arguments* in-stream vs. only block (docs suggest block/replace; details closed-source).
- Interaction of HTTP hooks (async) with synchronous PreToolUse blocking.

---

## Q5 — Session management & resume

**Question:** How does Claude Code manage sessions, persist history, and resume — and what should Niki's session model be?

**Findings:**
- A session is a **saved conversation tied to a project directory**; each prompt-processor is tied to that session. Sessions are saved **continuously** to local transcript files as you work — not at exit. (source: https://code.claude.com/docs/en/sessions)
- Resume surfaces: `claude --continue` (most recent), `claude --resume` (picker UI), `claude --resume <name>`, `claude --from-pr <number>` (bootstraps a session from a PR), and `/resume` to switch conversations inside an active session. (source: https://code.claude.com/docs/en/sessions)
- Headless (`-p`) / SDK-driven sessions are **not listed in the `--resume` picker**, but are still resumable by passing their session id to `--resume`. (source: https://code.claude.com/docs/en/sessions)
- Sessions are project-scoped, so the same `--continue` behaves per-project; worktree awareness matters (resolved through worktrees where applicable). (source: https://code.claude.com/docs/en/sessions)

**Confidence:**
- High — resume flags, picker behavior, continuous saving, project-tied sessions are first-party and concrete.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Save continuously, not at exit | Append/durably-commit each turn to a local transcript as it happens | https://code.claude.com/docs/en/sessions |
| Resume by id + `--continue` short-circuit | Support resume recent (continue) and a picker (resume), both in-session via a slash command | https://code.claude.com/docs/en/sessions |
| Project-tied sessions | Key sessions to the workspace dir so per-project continuity is automatic, worktree-aware | https://code.claude.com/docs/en/sessions |
| Headless sessions resumable by id but hidden from picker | Keep automation-created sessions out of interactive picker but resumable by id | https://code.claude.com/docs/en/sessions |

**Disagreements:** none significant.

**Open questions:**
- Storage format/rotation for transcripts (how old sessions are pruned) — not specified in docs.

---

## Q6 — Structured output & tool-call reliability (extends existing research)

**Question:** Beyond the synthetic-tool/repair stack already researched, what do Claude Code's official structured-output and tool-use contracts add for Niki?

**Findings:**
- **Structured outputs** = JSON outputs (`output_config.format`) + **strict tool use** (`strict: true`). GA on Claude 4.5+; available via Bedrock/Vertex (strict) and Foundry (structured outputs). The strict flag guarantees schema-adherent tool calls without ad-hoc repair. (source: https://platform.claude.com/docs/en/build-with-claude/structured-outputs)
- **Client tools** (e.g. `bash`, `text_editor`): the model emits `stop_reason: "tool_use"` with a `tool_use` block; the *application* executes it and returns `tool_result`. This is the canonical Niki tool round-trip. (source: https://platform.claude.com/docs/en/build-with-claude/tool-use)
- **Server tools** (`web_search`, `web_fetch`, `code_execution`, `tool_search`) execute on Anthropic infra instead of the host — a useful distinction: some tools shouldn't run in the client. (source: https://platform.claude.com/docs/en/build-with-claude/tool-use)
- Tool-result errors must be structured back into the turn so the loop can react (agent not left guessing); this supports the existing 3-layer recovery idea (constrain → repair → degrade). (source: https://platform.claude.com/docs/en/build-with-claude/tool-use)

**Confidence:**
- High — strict tool use + JSON output + client/server tool split are first-party; these are additive to (not a rewrite of) the existing research file.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Strict tool use (`strict:true`) to guarantee schema-adherent calls | Leverage provider strict tool-use when available to eliminate most repair on Niki tool calls | https://platform.claude.com/docs/en/build-with-claude/structured-outputs |
| Client vs. server tool split | Designate safe/local tools as client-executed, network/heavy tools as host-executed — reduces prompt/approval noise | https://platform.claude.com/docs/en/build-with-claude/tool-use |
| `tool_use` block → execute → `tool_result` round trip | Model Niki's tool execution on the explicit tool-result feedback channel, feeding the verify phase | https://platform.claude.com/docs/en/build-with-claude/tool-use |

**Disagreements:** none — this extends; see existing research file for the 3-layer recovery disagreements.

**Open questions:**
- Provider coverage of strict tool-use across models Niki targets (only guaranteed 4.5+).

---

## Q7 — Cost tracking & observability

**Question:** How does Claude Code track and surface cost/context in real time, and what should Niki expose?

**Findings:**
- The `/usage` session block (successor to `/cost`, plus `/context`) shows API token usage and cache metrics in-session; the **dollar figure is computed locally** from token counts at standard list rates, not from the real bill. (source: https://code.claude.com/docs/en/costs)
- Observation via the **status line** (JSON on a command) includes a `cost` object: `total_cost_usd` (client-side estimate, resets on `/clear` new session), `total_duration_ms` (wall clock), `total_api_duration_ms` (time waiting on the API). (source: https://code.claude.com/docs/en/statusline)
- Context/usage fields available: `total_input_tokens`, `total_output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `context_window_size`, `used_percentage`, `remaining_percentage`, plus `exceeds_200k_tokens` and `fast_mode` flags. Note: cache-reads/writes are factored into input tokens. (source: https://code.claude.com/docs/en/statusline)
- **Enterprise norms** (docs): avg ~$13 per dev per active day; $150–250/dev/month; 90% of devs stay under $30/day. (source: https://code.claude.com/docs/en/costs)
- Status line is a **separate row above the built-in footer badges** — it adds context without replacing built-ins; `footerLinksRegexes` renders clickable badges (e.g. turn a `PROJ-1234` match into a link). (source: https://code.claude.com/docs/en/statusline)

**Confidence:**
- High — cost object + context fields + footer badges are first-party and precise (field names literal).
- Medium — enterprise $ figures are marketing-approximate, not a hard target.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Client-side computed cost estimate from token counts | Compute a per-session USD estimate locally from tokens×list-price, reset on new session | https://code.claude.com/docs/en/statusline |
| Rich usage object (cache-read/write, %, remaining) | Expose input/cache/usage-% and remaining in a status channel Niki renders per turn | https://code.claude.com/docs/en/statusline |
| Footer link badges from regex on output | Turn emitted IDs (task ids, refs) into clickable links via regex→url on turn output | https://code.claude.com/docs/en/settings#footer-link-badges |
| Separate statusline row, not replacing footer | Render live context/cost as an added status row, keep built-in footer intact | https://code.claude.com/docs/en/statusline |

**Disagreements:**
- "Estimated $, may differ from actual bill" — anyone treating the status-line number as billing truth is over-trusting a local heuristic; surface it labeled "estimate".

**Open questions:**
- How Claude computes per-request price factors (caching discounts) client-side.

---

## Q8 — Theme & customization (extends TUI research)

**Question:** How is Claude Code's theme/model color/agent color configured, beyond the verified clay-orange accent in the TUI research?

**Findings:**
- The `theme` setting default is `"dark"`; valid values: `"auto"`, `"dark"`, `"light"`, `"dark-daltonized"`, `"light-daltonized"`, `"dark-ansi"`, `"light-ansi"`, or a custom reference `"custom:<slug>"` / `"custom:<plugin-name>:<slug>"`. (source: https://code.claude.com/docs/en/settings)
- Settings precedence is **specific-scope-over-specific** with key merge across scopes: managed > user (`~/.claude/settings.json`) > project (`.claude/settings.json`) > local (`.claude/settings.local.json`). (source: https://code.claude.com/docs/en/settings)
- Custom themes are provided as theme JSON files and referenced through the `custom:` prefix; theme files live alongside user config, loadable via the referenced slug. (source: https://code.claude.com/docs/en/settings)
- NO_COLOR/FORCE_COLOR set in `settings.json` env only reach subprocesses, **not** Claude Code's own interface — to change the TUI's own colors you must set them in the shell before launch. (source: https://code.claude.com/docs/en/settings)
- Context-specific statusLine config controls spacing (`padding`, `refreshInterval`, `hideVimModeIndicator`) — customization is layered: theme (colors) + statusline (layout) + footerLinks (actionable badges). (source: https://code.claude.com/docs/en/settings, https://code.claude.com/docs/en/statusline)
- `availableModels` restricts selectable models for main session/subagents/skills/advisor; `enforceAvailableModels` (v2.1.175+) extends that restriction to the "Default" option. (source: https://code.claude.com/docs/en/settings)

**Confidence:**
- High — theme values/default, precedence chain, custom slug scheme, NO_COLOR quirk are first-party.

**"Steal for Niki" (ranked):**
| practice | what Niki should do | source |
|---|---|---|
| Theme presets incl. daltonized + ansi variants + custom slug | Ship dark/light/daltonized/ansi theme presets plus a custom-theme JSON load path | https://code.claude.com/docs/en/settings |
| Strict scope precedence w/ key merge | Implement orderly settings merge (managed→user→project→local) merging keys, matching the documented chain | https://code.claude.com/docs/en/settings |
| Theme vs. statusline separate concerns | Keep theme (color tokens) separate from statusline layout config in Niki's settings schema | https://code.claude.com/docs/en/settings |
| Custom statusline row above built-in footer | Let Niki's cusom statusline overlay/xadd a row without replacing default footer | https://code.claude.com/docs/en/statusline |
| NO_COLOR only reaches subprocesses | Don't let env NO_COLOR in config change Niki's own TUI palette — separate launch-env from app palette | https://code.claude.com/docs/en/settings |

**Disagreements:**
- Models/settings naming spells color themes differently across versions; rely on the canonical `theme` key + slug scheme rather than rotated aliases.

**Open questions:**
- Whether `model`/`agent`-specific color keys (like the `some/what` coloring the TUI research verified) are configured via the `theme` slug or via separate model color overrides — docs expose `theme`+`availableModels`, but not a model-specific color-override key in the same table.

---

## Consolidated "Steal for Niki" top list

1. **Two-tier memory** (committed op-opinion file + worktree-shared auto memory, head-loaded) — Q1
2. **Read-only free-run + session-scoped write approval + persisted-per-repo bash grants** — Q2
3. **Fresh-context subagents spawned from an explicit delegation brief, background-by-default with surfaced named permission prompts** — Q3
4. **Hook lifecycle at tool boundaries + turn events; skills loaded on demand** — Q4
5. **Continuous session save + resume-by-id + project-tied sessions** — Q5
6. **Strict tool use + client/server tool split on top of existing repair stack** — Q6
7. **Client-side cost estimate + rich usage object surfaced as a statusline row above an intact footer; footer link badges from output regex** — Q7
8. **Theme presets (incl. daltonized/ansi) + custom-slug themes + explicit settings precedence merge; NO_COLOR scoping quirk** — Q8

## Open questions (across all)
- Auto-compaction trigger threshold (undocumented).
- Background-subagent scheduling internals.
- Whether hooks can mutate tool arguments in-stream.
- Accessing true per-request billing factors for the cost estimate.
- Model-specific color-override key vs. theme-slug-only.
- Provider coverage of strict tool-use beyond Claude 4.5+.