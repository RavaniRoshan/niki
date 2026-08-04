# Research Synthesis: Claude Code vs. Google Cloud Code / Gemini Code Assist Architecture

## 1. Agent Loop Architecture (Perceive → Tool Use → Validate → Critique → Retry)

### Claude Code (Anthropic)

**Core implementation**: `queryLoop()` async generator (query.ts, ~1,729 lines). A simple `while(true)` loop implementing the ReAct pattern (Yao et al., 2022).

**9-step pipeline per turn** (arxiv 2604.14228v1 §4.1):
1. **Settings resolution** — destructures immutable parameters (systemPrompt, canUseTool callback, toolUseContext, taskBudget, querySource)
2. **Mutable state initialization** — single State object stored across iterations with seven "continue sites" that overwrite state via whole-object assignment (`state = { ... }`) rather than field-by-field mutation (the "Continue Site" pattern, inspired by React's `setState` philosophy)
3. **Context assembly** — `getMessagesAfterCompactBoundary()` retrieves messages from the last compact boundary forward
4. **Pre-model context shapers** — five sequential shapers (See §4 below)
5. **Model call** — `for await` loop over `deps.callModel()` streams the model response
6. **Tool-use dispatch** — `StreamingToolExecutor` (primary) or synchronous `runTools()` (fallback)
7. **Permission gate** — each tool passes through the permission system (§5)
8. **Tool execution and result collection** — tool results added as `tool_result` messages; loop continues
9. **Stop condition** — if response has no tool_use blocks, the turn is complete

**AsyncGenerator design**: Yields `StreamEvent`, `RequestStartEvent`, `Message`, `TombstoneMessage`, `ToolUseSummaryMessage` — unifies streaming output, termination, and error propagation in a single function signature.

**Recovery mechanisms** (§4.4):
- **Max output tokens escalation**: Up to 3 recovery attempts (MAX_OUTPUT_TOKENS_RECOVERY_ATTEMPTS) when hitting output cap, gated by GrowthBook flag
- **Reactive compaction**: When context nears capacity, summarizes just enough to free space (gated by REACTIVE_COMPACT, fires at most once per turn via `hasAttemptedReactiveCompact` flag)
- **Prompt-too-long (413) recovery — 3-stage cascade**:
  - Stage 1: Context Collapse drain (cost: 0) — pending collapse candidates immediately committed
  - Stage 2: Reactive Compact (cost: 1 API call) — full conversation summary, strips images, retries
  - Stage 3: "strip retry" — if summary still too large, removes media and tries once more
  - *Design principle: "the first attempt is always free"* — expensive summarization is a last resort
- **Streaming fallback**: `onStreamingFallback` callback handles streaming API issues
- **Fallback model**: `fallbackModel` parameter enables model switching on failure

**Stop conditions** (§4.5) — 9 termination reasons:
1. No tool use (text-only response) — primary stop
2. Max turns reached (configurable maxTurns)
3. Context overflow (API returns prompt_too_long)
4. Hook intervention (PostToolUse sets hook_stopped_continuation)
5. Explicit abort (abortController)
6. Max budget exceeded
7. Max output tokens reached
8. Error during execution
9. Loop detected

**Diminishing returns detection**: If Claude continues 3 consecutive turns producing fewer than 500 tokens each, the system stops — prevents "infinite loop of Claude saying let me try one more fix" (Bits & Bytes blog).

### Google / Gemini CLI

**Core implementation**: `packages/core/src/core/turn.ts` — TypeScript, event-based architecture using `GeminiEventType` enum.

**Event types**: Content, ToolCallRequest, ToolCallResponse, ToolCallConfirmation, UserCancelled, Error, ChatCompressed, Thought, MaxSessionTurns, Finished, LoopDetected, Citation, Retry, ContextWindowWillOverflow, InvalidStream, ModelInfo, AgentExecutionStopped, AgentExecutionBlocked.

**Agent mode** (Google blog, Jul 17, 2025): "multi-step, collaborative, reasoning agent that expands the capabilities of simple-command response interactions." Key behaviors:
- Builds a multi-step plan from a single prompt
- Auto-recovers from failed implementation paths
- Recommends solutions
- Analyzes entire codebase, proposes plan, awaits approval before changes

**Key events for loop control**:
- `LoopDetected`: Infinite loop detection (analogous to Claude Code's diminishing returns detection)
- `ContextWindowWillOverflow`: Proactive context management with `estimatedRequestTokenCount` and `remainingTokenCount`
- `ChatCompressed`: Emitted when context is compressed
- `MaxSessionTurns`: Turn-based stopping condition
- `Retry`: Automatic retry on failures

**Tool execution pipeline** (gemini-cli source): `tool.build(args)` → `shouldConfirmExecute(params, abortSignal)` (permission check) → `tool.execute(params, signal)` (execution). Results fed back as `ToolCallResponse` events.

### Key Difference
Claude Code: async generator with 9-step explicit pipeline and rich recovery cascade.
Gemini CLI: event-based streaming with specialized event types for each loop concern (compression, overflow, loop detection).

---

## 2. Agent Harness Pattern

### Claude Code (Anthropic)

**"Agentic harness" is the explicit term**: Claude Code "serves as the **agentic harness** around Claude" (claude.com/docs). The arxiv paper notes 98.4% of Claude Code's codebase is deterministic infrastructure, with only ~1.6% being model decision logic.

**Design principle**: "Minimal scaffolding, maximal operational harness" — invest in infrastructure over decision scaffolding. The model reasons freely; the harness creates conditions (tool routing, permission enforcement, context assembly).

**"Continue Site" pattern**: React's `setState` philosophy permeates the backend loop. Seven continue points each overwrite the State object in a single whole-object assignment rather than field-by-field mutation. "React's setState philosophy has permeated all the way into the backend loop — a glimpse into the Anthropic engineers' love for React."

**Layered architecture** (arxiv §3.3):
- **Surface layer**: Interactive CLI, Headless CLI, Agent SDK, IDE/Desktop/Browser, UI/renderer (React 18 + Ink)
- **Core layer**: agent loop (query.ts), compaction pipeline
- **Safety/Action layer**: permission system, hooks, extensibility, tools, sandbox, subagents
- **State layer**: context assembly, runtime state, persistence, memory, sidechains
- **Backend layer**: shell execution (optional sandboxing), remote execution, MCP connections (stdio, SSE, HTTP, WebSocket, SDK, IDE-specific)

### Long-Running Agent Harness Pattern (Anthropic Engineering Blog, Nov 26, 2025)

**"Two-fold solution" for working across many context windows**:
1. **Initializer agent**: First session uses specialized prompt to set up environment (init.sh, claude-progress.txt, feature_list.json with 200+ features as JSON)
2. **Coding agent**: Subsequent sessions make incremental progress, leave structured artifacts for next session

**Structured artifacts**:
- `claude-progress.txt`: Log of what agents have done
- `feature_list.json`: JSON array of features with description, steps, and `passes: false` status
- JSON chosen over Markdown because "the model is less likely to inappropriately change or overwrite JSON files"
- Git commit + progress update at end of each session

**Failure modes addressed**:
- One-shotting an app → feature list with incremental approach
- Premature completion declaration → self-verification before marking features as passing
- Leaving bugs → init.sh for basic end-to-end test, git for revert

### Three-Agent Architecture (Anthropic Engineering Blog, Mar 24, 2026)

**"GAN-inspired" harness**: Planner + Generator + Evaluator
- **Planner**: Takes 1-4 sentence prompt, expands into full product spec (stays focused on product context and high-level design, not granular technical details)
- **Generator**: Works in sprints (one feature at a time), self-evaluates at end of each sprint, uses git for version control
- **Evaluator**: Uses Playwright MCP to interact with running application, grades against criteria (product depth, functionality, visual design, code quality) with hard thresholds

**Sprint contract negotiation**: Generator proposes what it will build + how success will be verified; evaluator reviews the proposal; both iterate until agreement. Communication via files.

**Context reset vs compaction**: Context resets provide a "clean slate" (vs compaction's preserved continuity). Claude Code Sonnet 4.5 exhibited "context anxiety" strongly enough that compaction alone wasn't sufficient. Opus 4.6 largely removed this behavior, making context resets unnecessary.

**Dynamic workflows**: Claude Code Agent SDK blog references "a harness for every task: dynamic workflows in Claude Code" (claude.com/blog) as the approach used to orchestrate subagents at scale.

**Evaluation table** (from blog):
| Harness | Duration | Cost |
|---------|----------|------|
| Solo agent | 20 min | $9 |
| Full harness (initial) | 6 hr | $200 |
| Full harness (Opus 4.6, simplified) | 3 hr 50 min | $124.70 |

**Evaluator's role**: "the evaluator is not a fixed yes-or-no decision. It is worth the cost when the task sits beyond what the current model does reliably solo."

### Google / Gemini CLI

**"Shared technology" architecture**: Gemini CLI and Gemini Code Assist share the same underlying agent technology.

**Agent mode** (Google blog, Jul 17, 2025): "multi-step, collaborative, reasoning agent" that "builds a multi-step plan, auto-recovers from failed implementation paths and recommends solutions."

**Key characteristics**:
- Analyzes entire codebase (not just open file)
- Proposes plan and awaits approval before changes
- Checkpoints: "revert to a checkpoint" feature added (referenced in Claude Sonnet 4.5 launch blog)
- Shared with Claude Code via Agent SDK for long-running harness design

**Open source**: Gemini CLI is Apache 2.0 on GitHub (github.com/google-gemini/gemini-cli) — "developers can inspect the code to understand how it works and verify its security implications."

**Configuration**: `GEMINI.md` (analogous to CLAUDE.md) for project-specific instructions; `settings.json` for personal/team configuration.

### Key Difference
Claude Code: "maximal operational harness" with 4,600+ files of deterministic infrastructure, 1,729-line async generator loop, 5-layer compaction pipeline.
Google: "shared technology" with simpler open-source codebase, configuration via GEMINI.md, agent mode as IDE extension of CLI capabilities.

---

## 3. Prompt and JSON Schema Versioning & Validation

### Claude Code (Anthropic)

**Multiple system prompts** (not a single string): Claude Code has separate system prompts for:
- Main system prompt
- Builtin subagents: Explore, Plan, Verification, Claude Code Guide, Statusline-setup, CLAUDE.md creation
- Specialized prompts: Conversation summarization, Bash command prefix detection, Hook condition evaluator, Prompt suggestion generator, Dream memory consolidation
- Agent thread notes, Action safety and truthful reporting, etc.

**System prompt version tracking**:
- **Piebald-AI/claude-code-system-prompts** GitHub repo tracks 515 system prompts across 247 versions as of Claude Code v2.1.221 (Aug 3, 2026)
- "Updated within minutes of each Claude Code release"
- CHANGELOG.md documents changes per version
- Prompts "extracted directly from Claude Code's compiled source code" — guaranteed exact match
- Each system prompt includes token count (e.g., "871 tks" for Explore subagent prompt)
- `tweakcc` tool allows customizing individual pieces as markdown files

**Zod schema validation**:
- **27 hook events** defined in coreTypes.ts, 5 participating in permission flow (types/hooks.ts)
- Each permission-flow hook has specific Zod-validated output schema:
  - `PreToolUse`: permissionDecision (deny/ask), permissionDecisionReason, updatedInput (modify parameters)
  - `PostToolUse`: additionalContext, updatedMCPToolOutput (modify MCP results before context entry)
  - `PostToolUseFailure`: additionalContext
  - `PermissionDenied`: retry guidance
  - `PermissionRequest`: decision of allow or deny
- `PluginManifestSchema` (utils/plugins/schemas.ts) validates 10 component types: commands, agents, skills, hooks, MCP servers, LSP servers, output styles, channels, settings, user configuration
- `parseSkillFrontmatterFields()` parses 15+ YAML frontmatter fields for skills
- Hook condition evaluator uses dedicated subagent prompt

**Schema evolution**: 
- Anthropic internal docs (cited as `anthropic2026managed`) describe "Managed Agents" API as evolving (hooks, sessions, environments)
- Plugin manifest schema at v1 with 10 component types

### Google / Gemini CLI

**Configuration via GEMINI.md**: Project/team/user-level instruction files (analogous to CLAUDE.md)
**settings.json**: Configuration at multiple levels (project, user, system)

**Schema validation**:
- Uses `@google/genai` library with `FunctionDeclaration` type for tool schemas
- Tool arguments typed as `Record<string, unknown>` in TypeScript
- `PluginManifestSchema` validates plugin structure
- `InvalidStreamError` with specific subtypes: NO_FINISH_REASON, NO_RESPONSE_TEXT, MALFORMED_FUNCTION_CALL, MAX_TOKENS_EXCEEDED, SAFETY_BLOCKED, RECITATION_BLOCKED, OTHER_BLOCKED, THINKING_ONLY_RESPONSE

**Version tracking**: 
- Open-source repository allows community inspection of changes
- No centralized system prompt tracking equivalent to Piebald-AI repo
- Release notes published on Google blog for major updates

### Key Difference
Claude Code: External community tracking (Piebald-AI repo) independently monitors all 515 system prompts across 247 versions with changelog. Zod schemas for 27 hook events with 5 in permission flow.
Google: Internal version tracking via open-source repo; schema validation via @google/genai FunctionDeclaration; less granular hook event system.

---

## 4. Session/Context Management

### Claude Code (Anthropic)

**Five-layer compaction pipeline** (arxiv §7.3, Bits & Bytes §4) — cheapest-first, executed in sequence before every model call in query.ts (lines 365-543):

| Layer | Gate | Cost | Info Loss | Cache Awareness | Description |
|-------|------|------|-----------|-----------------|-------------|
| **1. Budget reduction** | Always active | Free | Low | No | Per-tool-result size limits; replaces oversized outputs with content references |
| **2. Snip compact** | HISTORY_SNIP | Free | High | No | Removes older history segments wholesale |
| **3. Microcompact** | CACHED_MICROCOMPACT | Free | Medium | Yes | Cache-aware selective clearing; defers boundary messages until after API response |
| **4. Context collapse** | CONTEXT_COLLAPSE | Low | Minimal | Yes | Read-time virtual projection over history; originals never modified |
| **5. Auto-compact** | Default enabled | High | Low | No | Full model-generated summary; LLM summarizes conversation |

**Design choice explanation**: Context Collapse runs BEFORE Auto-Compact so that if collapse gets us under the auto-compact threshold, auto-compact is a no-op and the system keeps granular context instead of a single summary.

**Auto-compact specifics** (§4.3):
- Threshold: `getEffectiveContextWindowSize(model) - 13_000` (13K token buffer for system prompt, tool definitions, next turn response)
- Circuit breaker: `MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES = 3` — prevents infinite loop when summarization itself exceeds context limit
- `PreCompact` hooks fire first, allowing custom instructions
- Uses `getCompactPrompt()` to generate the summarization request
- `buildPostCompactMessages()` returns: [boundaryMarker, ...summaryMessages, ...messagesToKeep, ...attachments, ...hookResults]

**Full Compaction engine** (§7.3, 1,705 lines): Preprocessing before Auto-Compact includes:
1. Image stripping: Replaces images/documents with `[image]`/`[document]` placeholders
2. Thinking block removal: (internal builds) Removes chain-of-thought blocks before compaction
3. Metadata extraction: Extracts structured metadata from tool results
4. PTL retry: Drops oldest API round groups until within limits

**4-stage token warning system**:
- 20K tokens remaining: Orange warning ("Compaction will trigger soon")
- 13K tokens remaining: Auto-compact fires
- 3K tokens remaining: Red blocking limit (only manual /compact available)

**CLAUDE.md hierarchy** (§7.2) — 4 levels, managed through directory-specific loading:

| Level | Path | Scope | Loading |
|-------|------|-------|---------|
| 1. Managed memory | /etc/claude-code/CLAUDE.md (Linux) | OS-level policy for all users | Startup |
| 2. User memory | ~/.claude/CLAUDE.md | Private global instructions | Startup |
| 3. Project memory | CLAUDE.md, .claude/CLAUDE.md, .claude/rules/*.md | Checked into codebase | Base at startup, nested lazy |
| 4. Local memory | CLAUDE.local.md | Gitignored, private project instructions | Startup |

Key design decisions:
- **Lazy loading**: Base hierarchy loaded at session start; nested-directory instruction files load only when agent reads files in those directories (prevents unused instructions consuming context)
- **@include directive**: `@path`, `@./relative`, `@~/home`, `@/absolute` for modular instruction sets (processMemoryFile() at claudemd.ts)
- **User context, not system prompt**: CLAUDE.md delivered as a user message, not system instructions — has compliance implications (model compliance with these instructions is not at system level)
- **File discovery**: Traverses from CWD up to root; files closer to CWD have higher priority (loaded later, more model attention)

**Auto memory** (§7.2, §7.1 item 5):
- Contextually relevant memory entries prefetched asynchronously
- First 200 lines or 25KB of MEMORY.md load at start of each session (claude.com/docs)
- "Dream memory consolidation" subagent (Piebald repo) — multi-phase memory consolidation pass: orienting on existing memories, gathering, summarizing
- LLM-based memory scan for relevant entries

**Prompt caching strategy**:
- Cache breakpoints placed after tool definitions (server places boundary after tool list)
- Tool definitions sorted for cache stability (`assembleToolPool()` sorts partitions)
- Microcompact respects cache: clearing a tool result in the middle invalidates all cache entries after that point — so it defers clearing until cache range shifts
- System prompt, CLAUDE.md, tool schemas are prompt-cached (content same across turns)

**Subagent context isolation** (§8.2):
- Subagents return only summary text to parent, not full conversation history
- Each subagent gets fresh context — "no prior message history, though it does load its own system prompt and project-level context like CLAUDE.md" (claude.com/docs)
- Worktree isolation: Creates temporary git worktree, giving subagent its own copy of repository
- Summary-only return prevents context explosion in parent as agent team grows

**Prefetch pattern**: Slow operations (fetch skill list, load memory) kicked off in Stage 1 of pre-model context assembly, consumed in Stage 6 (after tool execution).

**Session persistence** (§9):

Three persistence channels:
1. **Session transcripts**: Project-scoped JSONL files at `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`. Stores: conversation records (user, assistant, attachment, system messages), compaction markers, file-history snapshots, attribution snapshots, content-replacement records
2. **Global prompt history**: `history.jsonl` at Claude config home (~/.claude/), stores user prompts only, supports Up-arrow and ctrl+r navigation via `makeHistoryReader()` generator with `readLinesReverse()`
3. **Subagent sidechains**: Separate `.jsonl` + `.meta.json` files per subagent (sessionStorage.ts, runAgent.ts)

**Session identity**: Pairs `sessionId` with `sessionProjectDir`, set during resume or branch. Transcript path must use the same project directory active when messages were written.

**Resume/Fork** (§9.2):
- `--resume` / `--continue`: Replays transcript via `conversationRecovery.ts`, appends new messages to existing conversation
- `--fork-session` / `/branch`: Creates new session with copy of original's history; original unchanged
- **NEITHER restores session-scoped permissions** — users must grant them again in new session
- Sessions stored at `~/.claude/projects/<encoded-cwd>/*.jsonl`

**File checkpointing**:
- Before file edits, snapshots taken at `~/.claude/file-history/<sessionId>/`
- Reversible with Esc+Esc (double-escape)
- Checkpoints are file-level snapshots for `--rewind-files`, NOT a generic checkpoint store
- Separate from git — checkpoints persist across session resumes

**Append-only durable state** (design principle from Table 1):
- Session transcripts are "mostly append-only JSONL files" with explicit cleanup rewrites as exception
- Compact boundary markers use UUID-based patching: `annotateBoundaryWithPreservedSegment()` records headUuid, anchorUuid, tailUuid in boundary event
- Read-time chain patching: `compact_boundary` marker enables session loader to patch message chain at read time
- Original messages never modified during compaction — collapsed view is a read-time projection
- Content replacements persisted for agent/REPL sources to enable reconstruction on resume

**Context efficiency features** (§3.6):
- **CLAUDE.md lazy loading**: Only loads base hierarchy at startup; nested rules load on demand
- **Deferred tool schemas**: When ToolSearch enabled, tools include only names initially; full schemas loaded on demand
- **Subagent summary-only return**: Subagents return only summary text, not full transcript
- **Per-tool-result budget**: Individual tool results capped at configurable size
- **Tool search only loads when needed**: Reduces system prompt bloat

### Google / Gemini CLI

**Context management**:
- `ContextWindowWillOverflow` event: Provides `estimatedRequestTokenCount` and `remainingTokenCount` for proactive management
- `ChatCompressed` event: Emitted when context is compressed
- 1M token context window (advertised for Gemini Code Assist Enterprise)
- **Tool search** (MCP): Defers MCP tool schemas by default, loads on demand (same as Claude Code)
- **Shared Agent SDK**: Session management via `query()` with `resume`, `continue`, `fork` options (same API as Claude Code SDK)

**Configuration**:
- `GEMINI.md`: Project-level instruction file (analogous to CLAUDE.md)
- `settings.json`: Project-level configuration (analogous to .claude/settings.json)
- `.aiexlude` file: For ignoring sensitive or legacy code
- **.gitignore enforcement**: Automatically enforced

**Session persistence**:
- `sessions.json`: Session state tracking at config directory
- Session files stored at `~/.gemini/sessions/` or configurable via `GEMINI_CONFIG_DIR`
- Session management options: `persistSession: false` (TypeScript only) for in-memory sessions

**Context sources**:
- Local codebase awareness via file reads
- Selected code snippets can be added to chat context
- Terminal output can be attached to chat
- MCP servers for external context

### Key Difference
Claude Code: 5-layer compaction pipeline with cache awareness, 4-level CLAUDE.md hierarchy with @include directives and lazy loading, append-only JSONL transcripts with UUID-based boundary patching.
Google: Simpler event-based model with `ContextWindowWillOverflow` and `ChatCompressed` events, GEMINI.md configuration, 1M token window.

---

## 5. Structured Output Enforcement Across Tool-Use and Text Responses

### Claude Code (Anthropic)

**Structured output retry mechanism** (claude.com/docs/agent-sdk/agent-loop):
- **`error_max_structured_output_retries`**: SDK result subtype — "No valid structured output was produced within the configured retry limit: every attempt failed validation, or a model fallback retracted the completed output with no successful retry"
- Retries with escalation: When structured output fails validation, the system retries up to a configured limit
- Model fallback: A different model can be used for retries via `fallbackModel` parameter
- Each result message carries `total_cost_usd`, `usage`, `num_turns`, and `session_id` so tracking works even after structured output failures

**Zod schema validation for hooks** (§5.3):
- 5 permission-flow hooks each have specific Zod-validated output schemas (types/hooks.ts):
  - `PreToolUseSchema`: { permissionDecision?: 'deny' | 'ask', permissionDecisionReason?: string, updatedInput?: object }
  - `PostToolUseSchema`: { additionalContext?: string, updatedMCPToolOutput?: object } (for MCP tools, allows modifying results before they enter context)
  - `PostToolUseFailureSchema`: { additionalContext?: string }
  - `PermissionDeniedSchema`: { permissionDecision?: 'allow' | 'ask', retryAfter?: string }
  - `PermissionRequestSchema`: { permissionDecision?: 'allow' | 'deny' }
- For non-MCP tools, the tool_result is emitted before PostToolUse hook fires; for MCP tools, result is delayed until after post hooks (enables updatedMCPToolOutput to take effect)

**Tool parameter validation**:
- Tool definitions use JSON Schema (FunctionDeclaration) for parameter validation
- `PluginManifestSchema` validates 10 component types
- Skill frontmatter parsed with 15+ YAML fields validated by `parseSkillFrontmatterFields()`

**Circuit breakers and anti-loop mechanisms**:
- `MAX_CONSECUTIVE_AUTOCOMPACT_FAILURES = 3`: When compaction itself fails 3 times consecutively, the circuit breaker stops retrying ("After 3 consecutive failures, the circuit breaker tris...")
- **Diminishing returns detection**: If Claude continues 3 consecutive turns producing fewer than 500 tokens each, the system stops — prevents infinite loops
- **4-stage token warning system**: Progressive visual feedback (20K → 13K → 3K) with auto-compact trigger and hard stop at 3K

**Prompt-too-long recovery cascade** (§4.4):
1. Context Collapse drain (cost: 0) — frees space without API calls
2. Reactive Compact (cost: 1 API call) — full conversation summary
3. Strip retry — if summary too large, removes images/media and retries
- "The first attempt is always free" — expensive summarization is last resort

**Error subtypes for result handling**:
| Result subtype | What happened | result field available? |
|----------------|---------------|------------------------|
| `success` | Task completed normally | Yes |
| `error_max_turns` | Hit maxTurns limit | No |
| `error_max_budget_usd` | Hit budget limit | No |
| `error_during_execution` | API failure/cancellation | No |
| `error_max_structured_output_retries` | Structured output validation failed | No |
| `prompt_too_long` | Prompt exceeded limit after recovery | No |
| `max_output_tokens` | Hit output token cap | No |
| `loop_detected` | Loop detected | No |
| `refusal` | Model declined request | No |

**PreCompact hooks**:
- Fire before compaction with `trigger` field ('manual' or 'auto')
- Can archive full transcript before summarization
- CLAUDE.md summarization instructions: Add a "Compact Instructions" section telling the compactor what to preserve during summarization

**PostToolUse hooks for output control**:
- Can inject `additionalContext` after tool returns
- For MCP tools: can return `updatedMCPToolOutput` to modify results before they enter context
- This is an attack surface: even trusted tools can have poisoned output
- Tool calls route through proxies that "can inspect return values before they enter the model's context" (How we contain Claude, §5)
- Input-layer probe scans tool outputs for prompt injection before context entry

**Content replacement persistence**:
- For agent and REPL query sources, content replacements persisted via `recordContentReplacement()` to enable reconstruction on resume
- Budget-reduced tool results replaced with references, but original can be reconstructed from disk

### Google / Gemini CLI

**Streaming validation** (turn.ts source):
- `InvalidStreamError` with specific subtypes:
  - `NO_FINISH_REASON`: No finish reason in response
  - `NO_RESPONSE_TEXT`: No text in response
  - `MALFORMED_FUNCTION_CALL`: Invalid function call format
  - `MAX_TOKENS_EXCEEDED`: Response exceeded max tokens
  - `SAFETY_BLOCKED`: Blocked by safety filters
  - `RECITATION_BLOCKED`: Blocked recitation filter
  - `OTHER_BLOCKED`: Other blocking
  - `THINKING_ONLY_RESPONSE`: Response contained only thinking blocks

**FinishReason detection**:
- `end_turn`: Model finished normally
- `max_tokens`: Hit output token limit
- `refusal`: Model declined request (requires explicit check: `stop_reason === "refusal"`)

**Retry mechanism**:
- `Retry` event type for automatic retries on failures
- `AgentExecutionStopped` and `AgentExecutionBlocked` events for explicit stop/blocking

**Tool validation**:
- `FunctionDeclaration` type with JSON Schema for parameter validation
- `ToolCallConfirmation`: Confirmation checked before tool execution (`shouldConfirmExecute` method)
- Tool arguments typed as `Record<string, unknown>` with validation at runtime

**Agent mode recovery** (Google blog):
- "Auto-recovers from failed implementation paths" — analogous to Claude Code's recovery cascade
- No explicit mention of structured output retries with model fallback

**Session-level result handling**:
- `GeminiFinishedEventValue`: { reason: FinishReason | undefined, usageMetadata: ... }
- Result messages carry cost and usage metadata (similar to Claude Code)

### Key Difference
Claude Code: Explicit `error_max_structured_output_retries` with model fallback escalation, Zod schemas for 5 hook types with specific output schemas, PreCompact and PostToolUse hooks for output modification, content replacement persistence for resume reconstruction.
Google: `InvalidStreamError` with 8 error subtypes, simpler retry via `Retry` event, `FunctionDeclaration` JSON Schema for tool params, no equivalent to Claude Code's hook-level output modification.

---

## Comparative Analysis Summary

### Claude Code Strengths
1. **Sophisticated context management**: 5-layer compaction pipeline with cache awareness, read-time projections, auto-compact with circuit breaker
2. **Rich harness infrastructure**: 4,600+ files, 1,729-line async generator loop, 52K-line permission system with ML classifier
3. **Multi-level memory**: 4-level CLAUDE.md hierarchy with @include directives, auto-memory scanning, lazy loading
4. **Defensive engineering**: Diminishing returns detection, 3-stage 413 recovery, circuit breakers, deny-first permissions
5. **Transparency**: Append-only JSONL transcripts, file-based memory (CLAUDE.md is markdown), Piebald-AI community version tracking

### Google/Gemini CLI Strengths
1. **Open source**: Entire codebase Apache 2.0 on GitHub, fully inspectable
2. **Shared technology**: Same agent loop across CLI, IDE, and cloud
3. **Large context window**: 1M token context window advertised
4. **Simpler architecture**: Event-based system easier to understand and modify
5. **Strong IDE integration**: Agent mode in VS Code/JetBrains with plan-before-execute, checkpoints

### Architectural Philosophy
- **Claude Code**: "Model judgment within a deterministic harness" — trusts model reasoning but surrounds it with rich deterministic infrastructure (98.4% non-decision code). Prefers graduated layering over monolithic mechanisms.
- **Google**: "Shared technology, open extensibility" — same agent loop across products, open source for transparency, MCP-based extensibility. Less infrastructure investment, more focus on integration.

### Sources
1. Arxiv 2604.14228v1 — "Dive into Claude Code: The Design Space of Today's and Future AI Agent Systems" (cs.SE, Apr 14, 2026) — https://arxiv.org/html/2604.14228v1
2. Arxiv 2603.05344v3 — "Building Effective AI Coding Agents for the Terminal" (cs.AI, Mar 13, 2026) — https://arxiv.org/html/2603.05344v3
3. Anthropic Engineering Blog — "Effective harnesses for long-running agents" (Nov 26, 2025) — https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents
4. Anthropic Engineering Blog — "Harness design for long-running application development" (Mar 24, 2026) — https://www.anthropic.com/engineering/harness-design-long-running-apps
5. Anthropic Engineering Blog — "How we contain Claude across products" (May 25, 2026) — https://www.anthropic.com/engineering/how-we-contain-claude
6. Anthropic Engineering Blog — "How we built Claude Code auto mode" (Mar 25, 2026) — https://www.anthropic.com/engineering/claude-code-auto-mode
7. Anthropic Engineering Blog — "Building Effective Agents" (Dec 19, 2024) — https://www.anthropic.com/research/building-effective-agents
8. Anthropic Engineering Blog — "Effective context engineering for AI agents" — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
9. Claude Code Docs — "How Claude Code works" — https://code.claude.com/docs/en/how-claude-code-works
10. Claude Code Agent SDK — "How the agent loop works" — https://code.claude.com/docs/en/agent-sdk/agent-loop
11. Claude Code Agent SDK — "Work with sessions" — https://code.claude.com/docs/en/agent-sdk/sessions
12. Claude Code Agent SDK — "Overview" — https://code.claude.com/docs/en/agent-sdk/overview
13. Piebald-AI — "Claude Code System Prompts" GitHub repo — https://github.com/Piebald-AI/claude-code-system-prompts
14. Bits & Bytes — "Claude Code Architecture Analysis" (Mar 31, 2026) — https://bits-bytes-nn.github.io/insights/agentic-ai/2026/03/31/claude-code-architecture-analysis.html
15. Google Blog — "Gemini CLI: your open-source AI agent" (Jun 25, 2025) — https://blog.google/innovation-and-ai/technology/developers-tools/introducing-gemini-cli-open-source-ai-agent/
16. Google Blog — "New in Gemini Code Assist: Agent Mode and IDE enhancements" (Jul 17, 2025) — https://blog.google/innovation-and-ai/technology/developers-tools/gemini-code-assist-updates-july-2025/
17. Google Cloud Docs — "Gemini Code Assist Standard and Enterprise overview" — https://docs.cloud.google.com/gemini/docs/codeassist/overview
18. Gemini CLI source code (turn.ts) — https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/core/turn.ts
19. Anthropic Blog (claude.com) — "A harness for every task: dynamic workflows in Claude Code" — https://claude.com/blog/a-harness-for-every-task-dynamic-workflows-in-claude-code
20. Anthropic Engineering Blog — "Managed Agents" — https://www.anthropic.com/engineering/managed-agents

### Sources Not Accessible
- Addy Osmani's "Agent Harness Engineering" blog (DNS resolution failure)
- MindStudio blog on Claude Code memory architecture (404)
- Some Google developer docs URLs returned 404 (URL changes)
- OpenDev arxiv paper 2603.05344v3 (limited content due to truncation, but abstract and architecture overview obtained)

### Notes on Methodology
Research was conducted primarily through public sources: arxiv papers, official engineering blogs, documentation sites, open-source repositories, and independent technical analysis. The arxiv paper 2604.14228v1 provides source-level analysis of Claude Code v2.1.88 TypeScript source code. The Bits & Bytes blog provides complementary analysis from the March 2026 npm source map leak incident. Google's Gemini CLI is open-source (Apache 2.0), allowing direct source code inspection.
