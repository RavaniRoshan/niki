# NIKI — Design Note (Phase 0)

Status: DRAFT for approval. Nothing in this document is verified until a probe
or test prints its number (see CHECKLIST.md).

## 1. Package graph

```
cmd/niki                  binary: interactive / headless / (optional) app-server
internal/protocol         flat Event + Submission types (the single seam)
internal/config           layered config resolution + settings view model
internal/llm              provider clients, streaming, catalog, cost
internal/core             SQ/EQ runtime, Session spawn, manager bag, turn state machine
internal/tools            tool registry + built-in tools (fail-closed factory)
internal/mcp              MCP client, connection manager, startup policy, catalog cache
internal/skills           skill discovery, parsing, catalog
internal/contextwin       instruction assembly, static/dynamic boundary, compaction
internal/permissions      policy, approval, fail-closed shell AST allowlist
internal/sandbox          per-OS isolation backends + documented fallback
internal/session          persistence, resume, list, fork, atomic writes
internal/tui              Bubble Tea v2 client; render is pure, async by messages
```

Rules:
- One runtime, one event type, one dispatcher, one keymap registry, one command
  registry, one theme module, one config resolver, one tool registry, one MCP
  connection manager, one session store, one prompt assembler with one
  static/dynamic boundary.
- `internal/core` never imports `internal/tui`. The TUI is one client.
- Render is a pure function of (state, terminal size); it performs no I/O.

## 2. SQ/EQ runtime shape

- `Submission` queue: bounded, cap 512.
- `Event` queue: unbounded, flat interface with a marker method; consumers
  type-switch on concrete event structs.
- One loop goroutine: `select` over `ctx.Done()`, submission channel, and an
  input mailbox. One turn goroutine per active turn; cancellation is
  `context.Context` throughout.
- Every state change emits a typed `Event`. The UI renders events; it never
  invents them.

## 3. Boot pipeline & readiness contract

Parallel pipeline, fire I/O side-effects before heavy work:

1. Parse CLI; resolve config layers (defaults → user → project → env → flags).
2. Detect terminal capabilities (via termenv / bubbletea).
3. Prefetch goroutines: instruction files (root→cwd), skills index
   (frontmatter only), model catalog (cache-valid), git snapshot, MCP servers
   per policy, renderer warm-up (theme, glyphs).
4. Draw first frame; open composer. User can type while optional prefetch runs.
5. Publish readiness state. First submitted prompt waits only for REQUIRED.
6. Background: model client prewarm handed to first turn; MCP refresh worker.

Required vs optional (THE readiness contract):

| Capability                | Class    | Blocks first prompt? |
|---------------------------|----------|----------------------|
| Config resolved           | required | yes                  |
| Terminal capabilities     | required | yes                  |
| Instruction files loaded  | required | yes                  |
| Skills catalog indexed    | required | yes                  |
| Git snapshot              | optional | no                   |
| Model catalog resolved    | required | yes (from cache ok)  |
| Model client prewarm      | optional | no (first turn may warm) |
| MCP servers               | mixed    | required-server wait skipped when cached catalog exists |
| Renderer glyph warm       | required | yes                  |
| Tree-sitter/highlighting  | optional | no                   |
| History hydration         | optional | no                   |

## 4. Agent loop & tool model

- Turn = user input → model stream → 0..n tool calls → tool results → repeat
  until the model stops calling tools.
- Each continue builds a complete new immutable state and records
  `transition: { reason }`. Termination reasons: completed, interrupted,
  too-long, model-error, max-turns. Diminishing-returns guard on repeated tiny
  continuations.
- Recovery is cheapest-first (free drain → one summarization call → surface);
  circuit breakers on compaction and classifier denials.
- Tools behind one interface: name, description, JSON-schema args, run,
  permission class, `IsConcurrencySafe` (fail-closed: default false via
  embedded base struct). A batch runs concurrently iff every call is safe; cap
  ~10; deferred state changes applied in original call-id order.

## 5. MCP / skills / config design

- MCP: one client per server keyed by name; sanitized qualified names for the
  model, raw identity preserved for routing; required = eager start, optional =
  lazy-with-cached-catalog; persisted tool-catalog cache; read-only per-server
  status; stdio + optional streamable HTTP; MCP output treated as untrusted.
- Skills: `SKILL.md` + YAML frontmatter; ordered root list (global + project
  ancestors); catalog prompt with progressive disclosure; body loaded on
  invocation; skill injects into current context (cheap), subagent gets an
  isolated context (expensive); malformed skill skipped, never fatal; bundled
  skills install only on fingerprint change.
- Config: defaults → managed → user → project → session flags; resolved to
  effective values; every value shows its source in the settings view.

## 6. Permissions & sandbox

- Modes: read-only, ask, auto (allow-list), no-review (never default).
  Deny wins. Safest option focused by default on approval prompts; Esc denies.
- Shell analysis: fail-closed AST allowlist via `mvdan.cc/sh/v3/syntax`;
  unknown node ⇒ `too-complex` ⇒ prompt. Compound commands split and checked
  each (capped). No regex denylist.
- Untrusted text (repo, tool output, MCP) never alters policy; sanitize
  control chars on render; every decision logged; no credentials in files.
- Sandbox: one type, per-OS backend (Linux landlock+seccomp or namespaces,
  macOS seatbelt, Windows restricted token), documented fallback. Filesystem
  allow-list + network off by default + resource limits + kill-on-parent-exit.
  Never reach home, SSH keys, cloud creds, unrelated repos. Cleanup on exit and
  on panic.

## 7. TUI architecture

- Inline viewport anchored at the real cursor: finalized history scrolls into
  native scrollback; a small live region streams at the bottom. Fullscreen
  owned mode only if a proven need exists.
- One border level (the composer); hierarchy via weight/dimness/whitespace.
- Header scrolls away: NIKI mark+version, model, cwd+branch, readiness.
- Transcript is a retained semantic document with width/theme/height caches;
  user band, assistant markdown (Glamour), collapsed reasoning line, tool rows
  (`glyph Tool(args)` + indented result + expand hint), inline errors, live
  activity line above the composer.
- Composer: single rule, `>` prompt, textarea, history, `/` fuzzy menu, `@`
  file picker, queue indicator during a run.
- Footer: model/state left, cwd+branch center, hints+context meter right,
  collapses by width.
- Scheduler coalesces updates into one frame per deadline with a frame-rate
  cap; idle costs nothing. Streaming paced with hysteresis; committed markdown
  never re-rendered. Synchronized output (DEC 2026) where supported; kitty
  keyboard protocol with restore on every exit path. termenv/NO_COLOR-aware
  color in a pure, testable function. Reduced-motion honored.
- TUI never writes to stdout while live; terminal restored on clean exit, Esc,
  SIGTERM, SIGHUP, panic; Ctrl+Z suspend/resume redraws.

## 8. Performance contract (honest Go budgets)

| Metric                                        | Target        |
|-----------------------------------------------|---------------|
| Cold start → first frame                      | ≤ 60 ms       |
| Cold start → interactive composer             | ≤ 90 ms       |
| Warm start → interactive composer             | ≤ 25 ms       |
| Input echo p95 under streaming                | ≤ 30 ms       |
| Idle                                          | 0 redraws, CPU < 1% |
| Render cost at 100 vs 5,000 messages          | ratio ≤ 1.5   |
| Memory                                        | bounded idle RSS, no unbounded growth |
| Required capabilities warm before first prompt| yes           |

Every number requires a measured probe; otherwise UNVERIFIED.

## 9. Testing strategy

- Headless render harness → text/ANSI artifacts, golden files at 50x16,
  80x24, 120x38, 160x45.
- PTY e2e: real binary in a pseudo-terminal, raw bytes in, assert final
  screen, hard timeout, child cleanup.
- stdlib fuzzing: hostile bytes never wedge input; hostile markup never
  reaches the terminal.
- Fixture runtime behind a build tag (never release) replays a coding loop.
- Lint tests: no color literals outside theme; no key handling outside
  registry; no stdout writes while the TUI is live; no session-varying
  conditional in the static prompt prefix.
- `go test ./... -race`, `go vet`, `golangci-lint`, benchmarks with recorded
  baselines. No test skipped/weakened/deleted to get green.

## 10. Conformance matrix

See docs/CHECKLIST.md. All rows start UNVERIFIED.
