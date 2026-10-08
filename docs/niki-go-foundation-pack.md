# NIKI From Scratch — Go, Performance-First (for Kimi Code)

**What this is:** the complete from-zero build pack for **NIKI, a personal coding-agent harness**, now in **Go**. One static binary, Codex's architecture and boot behavior as the main reference, Kimi Code and Charm Crush for terminal look and Go-idiomatic TUI patterns, Google's Antigravity CLI for behavior ideas. It is for you, not a product. Launch, Product Hunt, signing, and distribution channels are **shelved**.

**This is the Go edition of the Rust pack.** The architecture, boot DAG, phases, feature atlas, perf rig, and reference policy carry over unchanged. The language-specific parts (packages, TUI stack, sandbox, MCP, tooling, goals) were rewritten.

---

## Verdict on "Rust to Go"

**Yes, do it. The pack barely changes, but your iteration speed changes a lot.** Why:

- **Compile loops.** Go builds in seconds and has no borrow checker. The loops that hurt before (compile errors, lifetime fights, huge incremental builds on a low-RAM machine) mostly disappear.
- **The TUI stack you were missing exists.** Bubble Tea v2, Lip Gloss v2, and Bubbles v2 shipped in Feb 2026 and have powered Charm's own coding agent, Crush, in production from the start. **Inline mode is a first-class use case**, the renderer supports synchronized output and richer keyboard input, mouse events are typed, and the view is declarative. `tea.Println` prints finished content above the live view, which is the same scrollback technique Codex uses, built in. Bubbles ships a textarea, viewport (soft wrap, gutters), list, and more.
- **Elm architecture** (Model, Update, View) is exactly the shape that avoids the state problems of a hand-rolled UI.
- **Google chose Go** for its Antigravity CLI (May 2026): per Google, for faster execution than Gemini CLI, and it is built for SSH and multiplexer workflows with low resource overhead. Same class of tool, same choice.
- **Stdlib covers a lot:** `net/http`, `crypto/tls`, `os/exec`, `context`, `log/slog`, native fuzzing, pprof, cross-compilation with `CGO_ENABLED=0`.
- **MCP:** official and community Go SDKs exist (verify maturity in Phase 0).

**What it costs (so you choose with open eyes):**

| Cost | Consequence |
|---|---|
| Garbage collector and runtime | Idle memory is higher than Rust. The RSS budget is relaxed (see the performance contract). Startup is still fast; verify. |
| Codex's code is Rust | You cannot drop its code in. You reimplement from its behavior. Anything closely translated still needs attribution. |
| No ripgrep library | Search is a Go walker with gitignore support, with `rg` used when installed. Measure. |
| Sandbox nuance | NIKI itself must stay unrestricted while the commands it runs are confined, and kernel restrictions are per-thread while Go's runtime is multithreaded. So the pack uses a **re-exec helper** (`niki sandbox-run -- cmd`) that restricts only itself and then execs the target command. |
| Init-time traps | Go packages run `init()` at startup. Heavy registries (syntax highlighters, big regex tables) can cost tens of milliseconds. The pack makes you audit with `GODEBUG=inittrace=1`. |
| Fewer permissive Go agents to read | Crush is **FSL-1.1-MIT** (source-available; treat as ideas-only unless its terms clearly allow more). Antigravity CLI is **proprietary**. |

## Verified facts (re-verify in Phase 0)

- **Bubble Tea v2** (announced Feb 23, 2026; v2.0.6 by May 2026): the "Cursed Renderer", inline mode first-class, synchronized rendering, keyboard enhancements, typed mouse messages (`MouseClickMsg`, `MouseReleaseMsg`, `MouseWheelMsg`, `MouseMotionMsg`), a declarative `tea.View` (alt screen, mouse mode, cursor, title, progress bar), import path `charm.land/bubbletea/v2`. Bubbles v2 and Lip Gloss v2 ship alongside. Lip Gloss v2 removed `AdaptiveColor`, so light/dark selection is manual.
- **Google Antigravity CLI** (`agy`): Go, **proprietary** (its repository holds install scripts and docs, no source license), released May 19, 2026, replaces Gemini CLI. Documented features: skills as slash commands (Markdown with name/description frontmatter in `.agents/skills/`), plugins bundling skills, subagents, rules, MCP definitions, and hooks, checkpoints, resume, headless/CI, SDK. **Behavior reference only.**
- **Codex** (Apache-2.0, Rust): native zero-dependency executable; MCP startup is non-blocking with "Booting MCP server" status lines; the session's MCP pool initializes lazily and the first consumer awaits it; optional servers never hold the session hostage; failed clients recover in the background with exponential cooldown; the skills watcher starts after `SessionConfigured`.
- **Kimi Code** is MIT, TypeScript. **pi** is MIT, TypeScript. **Goose** is Apache-2.0, Rust.
- **"Microseconds" is a feeling.** Codex opens in tens of milliseconds. The pack makes you measure it, then match or beat it.

## Reference policy

| Tier | Sources | Allowed |
|---|---|---|
| **A: read, learn, adapt with attribution** | Codex, Kimi Code, kimi-cli, pi, Goose, Gemini CLI, OpenCode, Aider (verify every LICENSE file) | Read freely. Reimplement core ideas yourself. Adapt only small isolated pieces with a header comment and `THIRD_PARTY.md`. Rust-to-Go translation of code is still derivative: attribute it. |
| **A-limited** | Charm Crush (FSL-1.1-MIT) | Read for ideas. Adapt nothing unless you verify the terms permit it. |
| **B: behavior only** | Claude Code, Google Antigravity CLI | Public docs, public write-ups, and black-box observation. |
| **C: forbidden** | Any leaked, extracted, or reconstructed proprietary source, including copies of Claude Code's code circulating online | Do not read, copy, summarize, or ask the agent to read it. |

---

## Setup (once)

```bash
mkdir niki && cd niki && git init
go version                         # pin a recent stable Go; record it in go.mod
go install gotest.tools/gotestsum@latest golang.org/x/vuln/cmd/govulncheck@latest golang.org/x/perf/cmd/benchstat@latest
# install golangci-lint, hyperfine, and just with your package manager
mkdir -p ../refs docs/reference
cp <path>/niki-go-foundation-pack.md docs/PACK.md
```

`AGENTS.md`:

```
# NIKI
- Read docs/PACK.md completely at the start of every session and after any /compact.
- Memory: docs/PROGRESS.md, DECISIONS.md, CHECKLIST.md, PERF.md. Re-read before each phase.
- Never read leaked or extracted proprietary source. Never publish or push.
- Build hygiene: loop on `go build ./...` and `go test ./internal/<pkg>/...`; run golangci-lint, `go vet`, and the whole test suite at the end of a slice.
```

## Launch (Kimi Code)

```bash
kimi --plan
```

Kickoff: **"Read docs/PACK.md completely, then run Phase 0."** Then `/auto` and a goal (`KIMI_CODE_EXPERIMENTAL_GOAL_COMMAND=1 kimi`; fallback `kimi --max-ralph-iterations 20` with `<choice>STOP</choice>`). Five goals at the bottom.

---

## The prompt

````
<role>
You are the lead Go engineer building NIKI from zero. Repo root = current directory (empty at the start). Read AGENTS.md and this pack first. No code, design, or progress exists yet: this is the concept stage. Verify every external fact against the pinned source or docs; never rely on memory for an API.
</role>

<mission>
Build NIKI: a personal coding-agent harness in Go. One static binary, instant to open, with the capabilities a modern coding agent has: a streaming agent loop, tools, sandboxed command execution with approvals, MCP servers, skills, AGENTS.md instructions, settings, sessions with resume, context compaction, slash commands, hooks, a fast inline terminal UI, and a headless mode.
It is for me, not a product. The goal is a fast, understandable, well-made harness that I fully own and learn from. NIKI is allowed to be smaller than Codex. It is not allowed to be slower to start, and it must never claim anything it does not do.
Codex is the main reference for architecture, boot behavior, and functionality. Kimi Code and Charm Crush are references for terminal look and Go-idiomatic TUI patterns. pi and Goose support. Closed products (Claude Code, Google Antigravity CLI) are studied by behavior only.
RULE OF PROOF: "works" means a test or measurement ran and its real output is printed here. Reading code is not proof. Anything needing my API key, a real terminal, or my taste is OWNER-VERIFY with exact steps.
NO INVENTION: the UI shows only what the core reported. Numbers in docs come from stored measurements.
</mission>

<principles>
P1 One language, one binary. P2 Headless core first; every frontend is a client of the same Op/Event stream. P3 The boot critical path contains ONLY argv parsing, terminal setup, and the first frame; everything else is a background task with a status event. P4 Measure against Codex on this machine from Phase 0; budgets are numbers, not feelings. P5 Vertical slices: each slice ends with a runnable `niki` that does one new real thing. P6 Lean tests: tests earn their keep; no snapshot explosions; no test matrix before there is a product. P7 Small and boring dependencies; every Go module passes an admission check. P8 Read the references, then write NIKI's own code; adapt only small isolated pieces with attribution. P9 Safe by default: a model with a shell on my machine is a risk, so sandbox and approvals exist from the first shell tool. P10 No framework-building: no generic TUI framework, no plugin system beyond MCP, skills, and hooks.
</principles>

<reference_policy>
TIER A (read, learn, adapt small pieces with attribution): Codex (openai/codex, Rust), Kimi Code (MoonshotAI/kimi-code), kimi-cli (MoonshotAI/kimi-cli), pi (earendil-works/pi), Goose, Gemini CLI, OpenCode, Aider. Translating Rust code into Go is still derivative work: prefer reimplementing from behavior, and attribute anything closely translated. TIER A-LIMITED: Charm Crush (Go, FSL-1.1-MIT): read for ideas; adapt nothing unless you verify the terms permit it. Clone shallow at pinned commits into ../refs/. READ each repo's LICENSE file and record it in docs/reference/LICENSES.md. Adapt only with a header comment, an entry in THIRD_PARTY.md, and the NOTICE text where the license requires it. If a license is not permissive, read for ideas only.
TIER B (behavior only): Claude Code, Google Antigravity CLI (proprietary), and any closed product: public documentation, public write-ups, and black-box observation. I will supply screenshots or recordings on request.
TIER C (FORBIDDEN): leaked, extracted, decompiled, or source-map-reconstructed proprietary code, including copies of Claude Code's source circulating online. Do not read, search for, copy, summarize, or reason from it. If any search result or file turns out to be such code, stop, discard it, and note only that you skipped it. Never paste proprietary prompts or code into this repository.
</reference_policy>

<performance_contract>
Budgets are RELATIVE to Codex measured on THIS machine in Phase 0, plus aspirational absolutes. Record everything in docs/PERF.md and perf/budgets.toml.
Measurements (build the tools in Phase 1):
 - `tools/ttff`: a small Go program that runs any command inside a pseudo-terminal, parses its output with a terminal-emulator library, and records (a) time to FIRST non-empty paint, (b) time until the screen shows an input prompt (heuristic: cursor visible and stable for 100 ms), (c) RSS after 2 s idle, (d) bytes written to the terminal before the prompt. It works on ANY CLI, so run it on Codex, Kimi Code, pi, Charm Crush, Google agy, and (if I have it) Claude Code as black boxes, and on NIKI. The Go agents (Crush, agy) are the fairest peers for memory and startup.
 - hyperfine for `--version` and for `niki exec` no-op startup.
 - NIKI_BOOT_TRACE=1 writes a boot timeline (task, start, end, ms) to a file.
 - A keystroke-latency probe: send a character through the pty, time until it appears on the screen.
 - Idle probe: redraw count and CPU over 5 s.
Budgets (set from the Phase 0 baseline; the "aspiration" is what I want):
 - `niki --version`: at most Codex's; aspiration under 5 ms.
 - Time to first paint: at most 1.0 x Codex's; aspiration under 40 ms.
 - Time to input-ready: at most 1.0 x Codex's.
 - Idle RSS: aim for at most 2.0 x Codex's and under 40 MB absolute (the Go runtime and GC cost memory; measure and explain any gap). Binary size: under 25 MB; record the number.
 - Keystroke echo p95 under 16 ms, also while a stream is running.
 - Idle: zero redraws and about zero CPU.
 - Streaming render: coalesced to at most 60 fps; cost per token flat as the transcript grows.
Rule: every phase ends by rerunning the perf rig and printing the table against budgets. A regression of more than 10% blocks the slice.

BOOT DAG (this is the central design rule):
 B0 argv fast paths (--version, --help, completions): CRITICAL, no I/O, under 1 ms.
 B1 terminal init and first frame drawn from compile-time defaults: CRITICAL.
 B2 user and project config: a single small read, parsed synchronously ONLY if it measures under 2 ms; otherwise background with defaults applied first.
 B3 project trust check: background; gates the effect of project config.
 B4 instruction files (AGENTS.md chain): background; needed before the first turn.
 B5 skills index (frontmatter only, cached by directory mtimes): background; needed before the first turn.
 B6 MCP servers: all in parallel in the background; each has startup_timeout_sec; `required = false` servers never delay anything; `required = true` servers gate only the first turn that needs them; status events ("Booting MCP server: name", then ready or failed); background recovery with exponential cooldown (1 s doubling to 30 s).
 B7 git and repo state: background.
 B8 credentials: LAZY at the first request (OS keychain lookups can be slow).
 B9 provider client, DNS, and TLS: pre-connect in the background after the first frame while I type (opt-out; connects only to my configured provider; sends no data).
 B10 session store: LAZY at the first persist.
 B11 syntax-highlighting and Markdown assets: LAZY at first use (highlighter registries and Markdown renderers can cost tens of milliseconds to initialize).
 B12 update checks and telemetry: NONE.
The UI is drawn at B1 and shows each later task's status as it resolves. A first turn waits only for tasks it actually needs, and shows why.
Build: CGO_ENABLED=0 (static), -trimpath, -ldflags="-s -w", a pinned Go version, profile-guided optimization (a default.pgo from real runs) in P7. Audit package init cost with GODEBUG=inittrace=1 and eliminate heavy init() work: package-level regexp.MustCompile, large lexer or syntax registries, embedded assets decoded at init, cobra-style command trees built before the fast path. Choose the HTTP and TLS setup by measuring (stdlib net/http and crypto/tls first). Touch GOGC and GOMEMLIMIT only if a measurement shows a benefit. Never compress the binary (it slows startup).
</performance_contract>

<architecture>
A single Go module (path chosen in Phase 0) with packages under internal/ and commands under cmd/. Dependency direction: types <- config, provider, tools, sandbox, mcp, skills, instructions, session <- core <- tui, cli. Core NEVER imports a UI package. Define interfaces where they are consumed (Provider, Frontend, Tool, Sandbox).
 internal/types: Op, Event, items, messages, ids, errors. No third-party imports.
 internal/config: layered TOML (defaults < user < project < env < flags), source tracking per value, profiles, project-trust gate, validation, `config show --sources`, comment-preserving edits.
 internal/provider: Provider interface; OpenAI-compatible Chat Completions streaming (covers Kimi, DeepSeek, Ollama, OpenRouter, vLLM, LM Studio), OpenAI Responses, Anthropic Messages; SSE parser (own, tiny, fuzzed, on bufio); retries with backoff; usage and cost; reasoning parameters and reasoning-summary deltas; prompt-cache hints; an embedded model catalog (context window, price).
 internal/tools: registry and tools: shell (timeout, streamed output, process-group kill), read_file, list_dir, grep and glob (an in-process walker with gitignore support; use ripgrep when installed and faster; measure), apply_patch (the grammar models are trained on) and a search-replace edit (configurable per model profile), update_plan, web_fetch (P2). A small, sharp toolset beats a large one.
 internal/sandbox: process spawning, sandboxes, exec policy, approval policies. Linux: Landlock via a maintained Go library plus network denial by the strongest available mechanism (Landlock network rules where the kernel supports them, a seccomp filter on socket creation, or a network namespace when permitted); NIKI itself must stay unrestricted and kernel restrictions are per-thread while Go is multithreaded, so apply them in a RE-EXEC HELPER (`niki sandbox-run -- cmd`) that restricts only itself and then execs the target command. macOS: Seatbelt via sandbox-exec with a generated profile. `niki doctor` reports what is actually enforced. Exec policy: prefix rules (allow, prompt, forbid) and a built-in list of known read-only commands; approval policies (untrusted, on-request, never); writable roots; network off by default.
 internal/mcp: MCP client: stdio first, then streamable HTTP; initialize and version negotiation; tools/list and tools/call with timeouts and cancellation; list_changed handling; per-server config (command, args, env allowlist, cwd, url, headers, required, startup_timeout_sec, tool_timeout_sec, enabled_tools, disabled_tools); tool names namespaced mcp__server__tool; OAuth and resources/prompts later. Decide in Phase 0 between the official Go MCP SDK, the community SDK, and your own minimal client after reading what each costs in startup and binary size.
 internal/skills: discovery of SKILL.md folders (frontmatter name and description; body loaded on demand); an index cache keyed by directory mtimes; locations: ~/.niki/skills and <project>/.niki/skills plus compat paths including .agents/skills (a convention Google's agy and others use); explicit invocation and model-requested loading; skills surface as slash commands; optional hot reload (fsnotify) started after the session is configured.
 internal/instructions: AGENTS.md chain (global, then git root down to cwd), size caps, `niki init` generates one.
 internal/session: append-only JSONL rollouts under ~/.niki/sessions/, header line with metadata, resume, fork, titles, compaction records, atomic writes (write to temp, fsync, rename), crash-safe.
 internal/core: Session and Turn state machines; context builder (stable prefix: system prompt, tool schemas, instructions, skills index, environment context, then history); token accounting from provider usage; auto-compaction near the window limit; tool dispatch with approvals; interrupts; steering and message queueing while running; subagents later; hooks later.
 internal/tui: Bubble Tea v2 (Model, Update, View), Bubbles v2, Lip Gloss v2, INLINE by default (no alt screen). Commit finished cells to native scrollback with tea.Println and keep only the live region in View(): activity line, composer, popups, footer. Stream the in-progress message in the live region and commit settled lines progressively so the live region stays small. All I/O happens in tea.Cmd, never in Update. Components: header with mascot, transcript cells, activity line, composer (Bubbles textarea or your own, grapheme-correct, history, paste via the paste message), footer, popups (slash, approvals, pickers), Markdown and diff renderers, theme tokens. Read Charm Crush for structure and Codex for behavior.
 internal/hooks: lifecycle command hooks (later phase).
 internal/testkit: a scripted fake provider, a tiny fake MCP server (a test binary), temp-repo fixtures.
 cmd/niki: the binary: default TUI, `exec` (headless, --json event stream), `resume`, `mcp` (add, list, remove), `skills` (list), `doctor`, `config`, `login`, `completion`, `sandbox-run` (internal helper). The argv fast path (--version, --help, completions) runs BEFORE any command tree or heavy package is touched.
 tools/ttff: the time-to-first-paint measuring program.
INTERNAL PROTOCOL: `Op` (frontend to core): UserInput, Interrupt, ApprovalDecision, OverrideContext, Compact, ReloadSkills, ListMcpTools, Shutdown. `Event` (core to frontend): BootProgress{task,status}, SessionConfigured, TurnStarted, MessageDelta, ReasoningSummaryDelta, ToolCallBegin and End, ExecBegin and OutputDelta and End, PatchBegin and End, ApprovalRequest, McpStartupUpdate, TokenUsage, PlanUpdate, Notice, Error, TurnComplete. Study Codex's protocol for the shape, then define NIKI's own as Go types (an interface plus a type switch, JSON-serializable with a "type" discriminator) so a JSON-RPC wrapper (for ACP or an IDE) can be added later with no refactor.
RUNTIME: goroutines with context.Context cancellation everywhere; errgroup for the boot DAG; bounded channels with backpressure; one goroutine owns session state (actor style) so there are no shared-state locks; Update never blocks; no goroutine leaks (goleak in tests); tests run with the race detector where supported.
DATA DIRS: ~/.niki/{config.toml, sessions/, skills/, prompts/, rules/, cache/, log/}; project: .niki/{config.toml, skills/, prompts/}. NIKI never writes into a project unless I ask it to.
</architecture>

<feature_atlas>
In Phase 0 produce docs/FEATURE_ATLAS.md from the references: feature, how Codex does it, how Kimi Code and pi and Goose do it, NIKI plan (P0, P1, P2, skip), and why. Seed list by area (extend from the sources):
 BOOT: fast paths, boot DAG, boot trace, preconnect, lazy credentials.
 CONFIG: layers, profiles, project trust, env overrides, feature flags, `config show --sources`, safe comment-preserving edits, schema validation.
 AUTH: API keys via env and keychain (lazy), login helper, no subscription OAuth automation, per-provider settings, redaction.
 PROVIDERS and MODELS: Chat Completions, Responses, Anthropic; reasoning effort; streaming; usage and cost; model catalog; per-model profiles (edit format, tool set, prompt variant); retries and error UX; local models.
 CONTEXT: system prompt assembly; environment context; AGENTS.md chain; skills index; prompt-cache-stable ordering; token budgeting; tool-output truncation with the full output stored; compaction (auto and /compact); history rewriting rules.
 TOOLS: shell, read, list, grep, glob, apply_patch, edit, plan, web_fetch, view_image (P2), subagent (P2), tool search for large MCP catalogs (P2).
 EXEC and SAFETY: sandbox modes (read-only, workspace-write, full-access), approval policies, exec-policy rules, known-safe command list, command parsing (conservative; when unsure, ask), writable roots, network off by default, secret redaction, Rule-of-Two default posture (untrusted input present, so by default no network and no access outside the workspace), project-config trust (an untrusted project can NOT start MCP servers, hooks, or commands).
 MCP: stdio, HTTP, status events, timeouts, required and optional, recovery, namespacing, per-tool approvals, env isolation, `niki mcp` management, expose NIKI as an MCP server (P2).
 SKILLS: SKILL.md discovery, progressive disclosure, index cache, invocation (`$skill` or /skills), hot reload, compat paths, safety (scripts run through normal exec policy).
 INSTRUCTIONS and MEMORY: AGENTS.md chain, `niki init`, custom prompts and slash commands from Markdown files with arguments, user memory file (P2).
 HOOKS: lifecycle commands (session start, user prompt submit, pre and post tool use, stop) with JSON on stdin and exit-code semantics that can block a tool call; trust-gated.
 SESSIONS: JSONL rollouts, resume picker, `resume --last`, fork, titles, export, compaction records.
 TUI: inline viewport, scrollback insertion, composer (multi-line, history, Ctrl+R search, paste handling, external editor), live activity line, tool and exec and patch cells, streaming Markdown, diffs, approvals, slash popup, pickers (/model, /resume), transcript pager (Ctrl+T), footer with context meter, interrupt and queue, themes, reduced motion, NO_COLOR, mascot.
 HEADLESS: `niki exec`, --json events, exit codes, stdin prompts, max-time and max-cost, non-interactive approval policy (deny by default).
 EDITOR and IDE (P2): ACP server over stdio.
 OBSERVABILITY: tracing to a log file, boot trace, `niki doctor`, local crash reports, no telemetry.
 TESTING and EVAL: fake provider, fake MCP server, golden event streams, perf rig, optional Harbor adapter later.
</feature_atlas>

<phase_0_study_and_decide>
Read-only plus measurements. Deliver docs/DESIGN.md (at most 120 lines) and wait for my approval:
1. CLONE and STUDY the Tier A references at pinned commits. For Codex write docs/reference/CODEX.md answering from SOURCE: the boot sequence in order and what blocks the first frame; how MCP startup, skills loading, config layering, project trust, sessions, and compaction work; the Op and Event protocol; exec policy and sandboxing on Linux and macOS; the apply_patch grammar and implementation; how the TUI renders (custom terminal, history insertion, frame scheduling, keyboard-protocol handling); token accounting; what it does at idle. Shorter notes for the other references. ALSO study Charm Crush (Go, Bubble Tea v2) for how a Go agent is structured and where its TUI limits show, and read Google's public Antigravity CLI documentation for behavior (skills as slash commands, plugins, hooks, checkpoints, headless); do not look for its source.
2. MEASURE: build tools/ttff (a quick prototype is fine) and record time to first paint, time to input-ready, idle RSS, and `--version` time for every reference CLI installed on this machine (Codex first; include Go agents such as Crush or agy if present). Write the baseline into docs/PERF.md. If a reference is not installed, ask me to install it or skip it.
3. LICENSES: docs/reference/LICENSES.md from the actual LICENSE files. A proposal for any code you would adapt.
4. DECISIONS with rationale and rejected alternatives in docs/DECISIONS.md: inline versus alt-screen (recommended: inline); own MCP client versus the official or community Go MCP SDK; Bubbles textarea versus an own text area; Glamour versus an own Markdown renderer; syntax highlighting (chroma init cost) versus a lazy or lighter option; net/http client settings; the Landlock and seccomp library choices; apply_patch versus search-replace as the default edit tool; JSONL versus SQLite; actor and goroutine shape.
5. FEATURE_ATLAS.md.
6. The package list, the slice plan, and the build and test commands (a justfile with build, test, lint, perf, ui-gallery).
7. ASK ME: which provider(s) and models I will use and whether I have keys; my OS(es) (default Linux and macOS; Windows later); my machine's RAM; whether Codex is installed; anything else you need.
</phase_0_study_and_decide>

<phases>
P1 SKELETON AND PERF RIG. Module, types, config (layers and sources), CLI with the B0 fast path, the boot DAG executor with tracing, tools/ttff, hyperfine scripts, perf/budgets.toml, `niki --version`, `niki doctor` stub, `niki config show --sources`. A minimal inline TUI that draws the header, an empty composer, and the footer at B1, with BootProgress rows. Gate: the perf table versus Codex printed; first paint measured.
P2 HEADLESS CORE. Provider (Chat Completions streaming first), SSE parser, tool loop, tools (shell, read, list, grep, glob, apply_patch, edit, plan), instruction and environment context builder, JSONL sessions, `niki exec` with --json, interrupt, the fake provider and golden event tests. Gate: `niki exec "..."` completes a real multi-tool task against my provider (OWNER-VERIFY key) and a deterministic fixture task in CI-less tests.
P3 LIVE TUI. Op and Event wiring; streaming Markdown; transcript cells and scrollback insertion; live activity line; tool, exec, and patch cells; composer with history, multi-line, paste, and Ctrl+C, Esc, and Ctrl+D semantics; queueing; footer with context use; mascot; core slash commands (/help /model /clear /new /compact /resume /status /diff /quit). Gate: a real session in the TUI; perf rig rerun.
P4 SAFETY. Sandboxes, exec policy, approval UI (safest option focused by default; Esc denies), writable roots, network-off default, project trust, secret redaction, command safety parsing, a small red-team set (a README that orders exfiltration, a symlink escape, a lifecycle-script trap) with expected refusals. Gate: the safety tests pass; default posture documented in docs/SECURITY.md.
P5 EXTENSIBILITY. MCP client (stdio, then HTTP) with the boot-DAG semantics, status events, recovery, per-tool approvals; skills with the cached index; AGENTS.md chain and `niki init`; custom prompts and slash commands; hooks; profiles. Gate: the fake MCP server and a real one (OWNER-VERIFY) work; boot stays inside budget with 5 MCP servers and 50 skills.
P6 CONTEXT AND RELIABILITY. Compaction, resume and fork, retries and error UX, Responses and Anthropic providers, the model catalog, cost and usage, crash recovery, Ctrl+R history search, transcript pager. Gate: kill -9 mid-turn then resume works; long-session probe.
P7 PERFORMANCE HARDENING. Profile with pprof and runtime/trace, PGO (default.pgo from real runs), GC settings only by measurement, binary size, an init audit with GODEBUG=inittrace=1, dependency pruning, preconnect, lazy audit against the boot DAG, frame-time work. Gate: the final perf table; every budget met or the gap explained.
P8 DOGFOOD. Use NIKI to work on NIKI. Fix what hurts. Optional: a Harbor adapter and a pilot comparison against Codex at the same model, with the integrity rules (no task-specific content, no test peeking) and a stated budget. Docs for myself: ARCHITECTURE.md, CONFIG.md, a map of where every feature lives.
</phases>

<visual_spec>
Original to NIKI. Study Codex's and Kimi Code's terminal behavior for structure; do NOT copy any product's mascot, wording, spinner verbs, glyph set, or colors.
ANATOMY at 80x24 (every line comes from a real event):
     ▄███▄    Niki 0.x
    ███◐███   <model> · <approval policy>
     ▀███▀    ~/proj (<branch> ↑1)                       <- header: mascot + 3 lines, scrolls away
    > user message                                       <- raised row
    ● assistant text
    ● Read(cmd/niki/main.go)                             <- tool cell: glyph, bold name, dim args
      └ read 46 lines · ctrl+o expand                    <- dim result line
    ✗ Bash(go test ./...)
      └ exit 1 · 3 failing                             <- failures auto-show the useful excerpt
    ◐ Running tests… 12s · esc to interrupt              <- live activity line (only while working)
    ─────────────────────────────────────────────────    <- rule
    > _                                                  <- composer, never moves, always live
    ─────────────────────────────────────────────────    <- rule
    manual · main ↑1 · ~/proj        esc interrupt · ? help · ctx 12%    <- footer
INLINE MODE: finished cells are inserted above the viewport into native scrollback; the live viewport holds only the activity line, composer, popups, and footer. The terminal handles scrolling, selection, copy, and search. In Bubble Tea v2 this is the default (no alt screen): print finished cells with tea.Println and keep only the live region in View().
TOKENS (one theme module; no color literals elsewhere). Dark: background #14161A, surface #1B1E25, panel #232831, foreground #E4E7EC, muted #9AA1AD, primary #6B8EF2, secondary #8B93A3, accent #43AFA0, success #5FAE7A, warning #D99A3E, error #D2605C, tool #5FA8C4. Light: background #FAFAFB, surface #F1F2F4, foreground #1F2328, muted #5C636E, primary #3355CC, accent #1F7A70, success #2E7D4F, warning #8A5E12, error #A83232, tool #1F6C86. One accent for focus and activity; status colors only for status; text contrast at least 4.5:1, glyphs at least 3:1, tested in a unit test; color is never the only state signal; NO_COLOR, 16-color, and ASCII fallbacks.
GLYPHS (each with ASCII fallback; no emoji): activity sweep ◐ ◓ ◑ ◒ (- | / \), done ✓ (+), failed ✗ (x), queued ○ (o), connector └ (+-), assistant bullet ●. Cadence 120 ms, motion only while working, a static frame in reduced motion, no layout-moving animation.
MASCOT (the orb): one eye only (no legs, no two eyes, no rounded-square face). 7x3 half-block art above; compact one-row form at 50-79 columns; none under 50. States driven only by real events: idle ◐ (accent), working ◑ (static; the sweep belongs to the activity line), done ● (green), error ○ (red), interrupted ◐ (muted); ASCII eyes o, *, x. Art lives in one module; width never changes between states; it never schedules a redraw.
KEYS (one registry feeds the footer, help, popup, and palette): Esc interrupts; Ctrl+C clears non-empty input, else interrupts, else arms exit; Ctrl+D exits only on an empty input; Ctrl+L redraws; Ctrl+R history search; Shift+Enter or Alt+Enter newline; Ctrl+G external editor; Ctrl+O toggle tool output; Ctrl+T transcript pager; Tab queues a message while a run is active; arrows and PgUp and PgDn work wherever something scrolls or selects. Approvals: the safest option focused, 1/2/3, y, n, arrows, Enter, Esc denies.
MICROCOPY: sentence case, no exclamation marks, errors say what happened and what to do, every number from a real counter.
RESPONSIVE: under 50 columns compact with a resize hint; 50-79 one column and a minimal footer; 80 and up standard. Never panic at any size.
</visual_spec>

<process_rules>
- Slice discipline: before coding a slice, write its acceptance test or demo command in docs/PROGRESS.md. A slice is done when the demo runs and the perf rig is green.
- Build hygiene: loop on `go build ./...` and `go test ./internal/<pkg>/...`; run golangci-lint (errcheck, staticcheck, govet), `go vet ./...`, govulncheck, and the whole suite with the race detector (where supported) only at the end of a slice. If the same compile or test failure survives 3 different fixes, stop, write the diagnosis, and simplify the design.
- Lean tests: unit tests per package; golden event-stream tests for the core (golden files); the fake provider and fake MCP server for integration; TUI: teatest-style model tests and a few text snapshots at 3 sizes, plus a handful of PTY smoke tests (terminal restore on exit and Ctrl+C, paste, resize). No color-depth or ASCII snapshot matrix until the product is stable.
- Fuzz the SSE parser, patch parser, and JSON-RPC framing with Go's native fuzzing in P6.
- Keep docs/DECISIONS.md current: one entry per decision with the rejected alternatives.
- Dogfood from the end of P3: use NIKI for docs and test tasks and write down what hurts.
</process_rules>

<recommended_modules>
Pre-approved subject to an admission check (license; maintenance; no cgo unless justified; binary-size impact; init-time impact via GODEBUG=inittrace=1; startup impact via the perf rig). VERIFY each module exists, is maintained, and fits before adopting it.
 Stdlib first: net/http, crypto/tls, encoding/json, bufio (SSE), os/exec, context, log/slog, embed, flag (for the fast path).
 TUI: charm.land/bubbletea/v2, charm.land/bubbles/v2, charm.land/lipgloss/v2; Markdown via Glamour (verify its v2 status) or goldmark with an own terminal renderer; syntax highlighting via chroma (LAZY only; mind init cost); display width via go-runewidth and uniseg; diff via an own Myers implementation or go-diff.
 CLI and config: a stdlib fast path, then cobra or kong for subcommands; pelletier/go-toml/v2; a YAML library used only for skill frontmatter.
 Files and search: fsnotify, doublestar, a gitignore library, ripgrep when installed.
 Sandbox: go-landlock, a pure-Go seccomp library (verify), sandbox-exec on macOS.
 PTY and test emulators: creack/pty, a Go terminal emulator library for tests and tools/ttff (charmbracelet/x/vt or vt10x; verify), teatest.
 MCP: the official Go SDK, mark3labs/mcp-go, or your own.
 Misc: zalando/go-keyring (LAZY), google/uuid, golang.org/x/sync/errgroup, go.uber.org/goleak, google/go-cmp.
 Tools: gotestsum, golangci-lint, govulncheck, benchstat, pprof, hyperfine, just.
 Anything else: ask me.
</recommended_modules>

<limits>
- Work only inside this repository and ../refs. Never publish, push, or contact anything. Spend nothing beyond my provider usage for explicit OWNER-VERIFY runs; ask before any run over USD 5.
- Never read, search for, or use leaked or extracted proprietary source (tier C).
- Memory across compaction: re-read docs/PACK.md, PROGRESS.md, DECISIONS.md, CHECKLIST.md, PERF.md at the start of each phase and after any /compact. Append to PROGRESS.md after every step and print what was completed.
- Ask me ONLY for: the Phase 0 questions, API keys and spend, real-terminal checks, dependencies outside the pre-approved list, and any metric you cannot infer or measure. Everything else: choose conservatively and record it.
- If the same gate fails 3 times in a row with different fixes, stop, write a diagnosis, and ask.
</limits>

<evidence_discipline>
Printed command output or measurements for every claim. Never skip, ignore, or weaken a test; updating a golden snapshot after an intentional reviewed change is allowed. No placeholders, no dead commands or keys, no ignored errors (errcheck-enforced) and no panics in production paths. Fix root causes. Check a module's source or docs before using its API. Label anything unverified UNVERIFIED with the reason.
</evidence_discipline>

<definition_of_done>
NIKI runs as one static binary on my machine; the perf table shows time to first paint, input-ready, idle RSS, binary size, and `--version` at or better than Codex's (or each gap explained); a real multi-tool session works with my provider; sandbox and approvals are on by default; MCP servers and skills load in the background without delaying the first frame; sessions resume after a kill; I use NIKI to develop NIKI; docs/ARCHITECTURE.md maps every feature to its package. Nothing proprietary was read or copied; THIRD_PARTY.md lists every adapted piece with its license.
</definition_of_done>

<final_answer>
Provide: the perf table versus the baseline; the feature atlas with what shipped, what was skipped, and why; the decisions log highlights; the list of adapted code with licenses; the OWNER-VERIFY steps; known limitations; and what to build next.
</final_answer>

Begin with Phase 0 now. Do not write any code until I approve the design note.
````

---

---

## Goals (after plan approval and `/auto`)

**Goal 1 — Skeleton and perf rig (P1)**

```
/goal NIKI skeleton and perf rig are done, proven in this conversation: (1) the Go module builds with go build ./... and the package list from docs/DESIGN.md exists; (2) niki --version uses the argv fast path before any command tree or heavy package is touched, shown by a test and a hyperfine result; (3) GODEBUG=inittrace=1 output was printed and every package init over 1 ms is listed and justified or removed; (4) niki config show --sources prints every value with its source, shown; (5) the boot DAG executor (errgroup) runs with NIKI_BOOT_TRACE=1 writing a timeline, shown; (6) tools/ttff measures time to first paint, time to input-ready, idle RSS, and bytes written for NIKI and for every reference CLI installed, and the table versus budgets is printed and saved in docs/PERF.md and perf/budgets.toml; (7) a minimal inline Bubble Tea v2 TUI draws header, composer, and footer at B1 with BootProgress rows, and the terminal is restored on exit and on Ctrl+C, shown by a PTY smoke test; (8) golangci-lint, go vet, and go test (with -race where supported) ran clean with output printed; (9) git diff was reviewed and it is stated that no test was weakened and no proprietary code was read or used; (10) docs/PROGRESS.md is current and git status is clean. OR stop if only owner-required items remain, and print them.
```

**Goal 2 — Headless core (P2)**

```
/goal NIKI headless core works, proven in this conversation: (1) the provider package streams Chat Completions through a tested SSE parser and a scripted fake provider; (2) the tool loop runs shell, read_file, list_dir, grep, glob, apply_patch, edit, and update_plan with timeouts, output caps, and process-group kill, each covered by a test; (3) the context builder assembles system prompt, tool schemas, AGENTS.md chain, and environment context in a stable-prefix order, shown by a golden test; (4) JSONL sessions are written atomically and a golden event-stream test replays a multi-tool fixture task end to end; (5) niki exec "<task>" and niki exec --json run the fixture task deterministically, output printed, and Interrupt stops a running turn cleanly, shown; (6) a goroutine-leak test passes for a full turn; (7) a real multi-tool task against my provider is documented as an OWNER-VERIFY script with exact steps; (8) the perf rig was rerun and printed with no budget regression over 10 percent; (9) golangci-lint, go vet, and tests with the race detector ran clean; (10) git diff was reviewed and no test was weakened; (11) docs/PROGRESS.md is current and git status is clean. OR stop if only owner-required items (provider key) remain, and print them.
```

**Goal 3 — Live TUI and safety (P3-P4)**

```
/goal NIKI live TUI and safety are done, proven in this conversation: (1) a session in the inline TUI streams Markdown, commits settled cells to scrollback with tea.Println, shows tool, exec, and patch cells, a live activity line, queued messages, the footer context meter, and the mascot states driven only by real events, shown by a recorded fixture run and PTY smoke tests; (2) Esc, Ctrl+C, and Ctrl+D semantics, paste handling, multi-line input, history, and resize are covered by tests with the terminal restored on every exit path; (3) the core slash commands work from one registry that also feeds the footer and help; (4) the sandbox helper (niki sandbox-run) enforces writable roots and network denial where the kernel supports them, shown by tests that try to write outside the roots and reach the network, and niki doctor reports what is enforced; (5) approvals focus the safest option, Esc denies, and every decision is logged, shown by test; (6) an untrusted project cannot start MCP servers, hooks, or commands from its own config, shown by test; (7) the red-team set (exfiltration README, symlink escape, lifecycle-script trap) ends in refusal or approval requests with nothing leaving the sandbox, shown; (8) docs/SECURITY.md states the default posture and its limits; (9) the perf rig was rerun and printed with no regression over 10 percent; (10) golangci-lint and tests ran clean; (11) git diff was reviewed and no test was weakened; (12) git status is clean. OR stop if only owner-required items (real-terminal checks) remain, and print them.
```

**Goal 4 — Extensibility and reliability (P5-P6)**

```
/goal NIKI extensibility and reliability are done, proven in this conversation: (1) the MCP client starts every configured server in parallel in the background, shows Booting and ready or failed status events, honors required, startup_timeout_sec, and tool_timeout_sec, recovers a crashed server with exponential cooldown, and namespaces tools, shown by tests against the fake MCP server; (2) with 5 MCP servers and 50 skills configured, time to first paint stays within budget and the boot trace proves nothing on the critical path waits for them, printed; (3) skills load from a cached frontmatter index with bodies on demand, including the .agents/skills compat path, shown by test and a cold versus warm timing; (4) the AGENTS.md chain, niki init, custom prompts and slash commands from Markdown, hooks that can block a tool call, and profiles work, each covered by a test; (5) compaction triggers near the context limit and via /compact, shown by a golden test; (6) kill -9 mid-turn then niki resume restores the session, shown; (7) Responses and Anthropic providers pass the same provider conformance tests against the fake server; (8) retries with backoff and clear errors are shown against injected 429, 5xx, and truncated streams; (9) the SSE parser, patch parser, and JSON-RPC framing ran under go test -fuzz for the recorded duration with no panic; (10) the perf rig was rerun with no regression over 10 percent; (11) golangci-lint and tests ran clean; (12) git diff was reviewed and no test was weakened; (13) git status is clean. OR stop if only owner-required items (a real MCP server, provider keys) remain, and print them.
```

**Goal 5 — Performance hardening and dogfood (P7-P8)**

```
/goal NIKI performance hardening and dogfooding are done, proven in this conversation: (1) the final perf table was printed for NIKI and every reference CLI installed: time to first paint, time to input-ready, idle RSS, binary size, niki --version, keystroke echo p95, and idle redraws, with every budget met or each gap explained with its cause; (2) pprof and runtime/trace output, the GODEBUG=inittrace=1 audit, and a PGO before and after comparison with benchstat are saved in docs/PERF.md with the changes they drove; (3) the preconnect task is shown in the boot trace running after first paint and opt-out works; (4) docs/ARCHITECTURE.md, CONFIG.md, and a feature map from docs/FEATURE_ATLAS.md to packages exist and every atlas row is marked shipped, skipped, or planned with a reason; (5) THIRD_PARTY.md lists every adapted piece with its license, and a statement confirms nothing proprietary was read or copied; (6) docs/DOGFOOD.md records tasks done with NIKI on NIKI and what hurt; (7) the full gates (golangci-lint, go vet, govulncheck, tests with the race detector) ran clean on the final commit with output printed; (8) git diff was reviewed and no test was weakened; (9) git status is clean. OR stop if only owner-required items remain, and print them.
```
