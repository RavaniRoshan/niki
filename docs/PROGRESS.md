# NIKI — Progress Ledger

Append an entry after every step. Re-read DESIGN.md, DECISIONS.md, CHECKLIST.md
at each phase start and after any context compaction.

## 2026-10-07 — Phase 0 (DESIGN)
- Read existing tree; confirmed a prior Go rebuild exists (module
  github.com/RavaniRoshan/niki, bubbletea v1 TUI, engine/tools/mcp/skills/
  session packages). That code will be superseded/migrated toward the spec
  layout (internal/core, internal/llm, internal/contextwin) in P1.
- Wrote docs/DESIGN.md: package graph, SQ/EQ runtime, boot pipeline +
  readiness contract, agent loop + tool model, MCP/skills/config,
  permissions/sandbox, TUI, performance contract, testing strategy.
- Wrote docs/DECISIONS.md with recommended defaults for the five
  `<decisions>` items + recorded choices (module path, engine migration,
  fixture build tag, golangci-lint).
- Wrote docs/CHECKLIST.md with every row UNVERIFIED.
- No application code changed. git status to be committed.

## Marking system (for this task)
- This file is the chronological ledger; docs/CHECKLIST.md holds per-row
  conformance state; docs/DESIGN.md is the architecture; docs/DECISIONS.md is
  the decision log. Do not track state anywhere else.

## 2026-10-07 — P0 approved, began execution
Decisions confirmed by owner: fullscreen default; git = shell out; TOML
comment-preservation accepted; app-server deferred; readiness contract
approved as written in DESIGN.md §3.

## 2026-10-07 — Phase 1 progress (goal-1 items)
- `go vet ./...` clean; `go test ./... -race` clean on full suite.
- Binary boots; mock turn works (`niki exec`).
- Added internal/lintcheck: TUI render path has no I/O (B8), no color literals
  outside theme (U7 source-scan).
- TUI coalesces queued event bursts into one frame (B10, coalesce_test.go).
- PTY e2e tests pass (NIKI_PTY_TESTS=1): Ctrl+C exit, SIGTERM exit within 5s,
  non-TTY exec emits no escapes (L4).
- Config: LoadWithSources tracks per-section origin; `niki config` prints it
  (C1, test).
- Skills: strict frontmatter parse; malformed skill skipped, never fatal (C3,
  test). InstructionsBounded caps instruction file size (C2, test).
- Boot probe (`niki exec`): ~10 ms wall per invocation, 3 runs.

## 2026-10-07 — Phase 4 progress (goal-2 items)
- mvdan.cc/sh/v3 parser-based fail-closed shell AST allowlist added
  (permissions/shell_ast.go); pipes/subshells/substitution/control flow all
  classify too-complex; unparsable → parse-unavailable (A7, test).
- Tool interface extended: Base provides fail-closed IsConcurrencySafe=false /
  IsReadOnly=false; read_file/glob/grep opt into safe+read-only; Registry.Batch
  runs concurrently only when all calls safe (cap 10), else sequentially in
  call order (A2, test).

## 2026-10-08 — Phases 5–7: runtime, MCP, TUI lifecycle, perf contract
- Engine readiness matrix (B7): required capabilities (terminal,
  engine, model) warm before the first prompt; a prompt submitted
  early is refused with the missing list (TestReadinessRequired-
  BeforePrompt, TestPromptRefusedBeforeReadiness). Optional
  capabilities (skills, MCP) warm in the background and never
  block readiness.
- Engine turns run on their own goroutine; Esc interrupt keeps
  partial output (TestInterruptKeepsPartialOutput); permission
  gate denies with focus on the safest option (TestPermissionGate-
  DeniesTool); single read-only turn streams end to end with typed
  events (TestReadOnlyTurnEndToEnd, TestEngineEchoTurn).
- Context (X1/X3): tiered compaction engages in order, never grows
  context, preserves needed evidence (TestTieredCompactionEngages-
  InOrder, TestCompactionNeverGrowsContext, TestCompactionPreserves-
  Evidence). Circuit breaker already proven (X2).
- Subagents (goal 3.1): isolated context, summary-only return to
  the parent, depth limit (TestSubagentIsolatedContext,
  TestSubagentReturnsSummaryOnly, TestSubagentDepthLimit).
- Sessions (P1): fork works (TestForkSession); delete and append-
  across-reopens covered (TestDeleteSession, TestAppendAcross-
  Reopens).
- MCP manager (M1–M5): one client per server, sanitized qualified
  names with raw identity preserved, required servers eager,
  optional lazy-when-cached (cached tools served with zero
  connection wait; live tools/list refreshes the persisted catalog
  in the background), truthful per-server status, fail-closed
  routing, output sanitized as untrusted. Tests: TestManagerOne-
  ClientPerServer, TestQualifySanitizesNames, TestManagerEager-
  Required, TestManagerOptionalLazyWhenCached, TestManagerFailed-
  RequiredSurfaced, TestManagerOutputSanitized, TestManagerFail-
  ClosedRouting, TestCatalogCacheRoundTrip.
- TUI: inline viewport split — finalized history lands in native
  scrollback while the live region streams without re-rendering
  committed history (TestLiveRegionSplit, TestInlineFlushOnTurnEnd);
  transcript, composer, footer, commands and motion render from
  real events only (TestTranscriptRendersFromRealEvents,
  TestActivityLineFromEvents, TestComposerOnlyBorderedElement,
  TestInlineHeaderScrollsAway); debug view shows boot-phase
  timings and frame telemetry (TestDebugViewTelemetry).
- Terminal lifecycle: terminal.Detect negotiates synchronized
  output (DECRQM 2026) and the kitty keyboard protocol with a
  bounded poll(2) wait — no leaked reader goroutine (a leaked
  reader would steal bubbletea's keystrokes); restore sequences
  written unconditionally on every exit path (L7, terminal tests
  + TestDetectUnderPTY). Ctrl+Z returns tea.Suspend (L2,
  TestCtrlZSuspends); resize safe 0x0..10000x10000 in both modes
  (L3, TestResizeNeverPanics); NO_COLOR honored (TestNoColor-
  StripsColor).
- Skills inject into the runtime context (C4, TestSkillsInject-
  IntoContext); config reloads without a restart — /reload swaps
  provider and permission mode between turns, a failed reload
  surfaces its error and keeps the current config, a reload
  requested mid-turn is deferred (C5, TestConfigReloadsWithout-
  Restart, TestConfigReloadFailedSurfacesError, TestConfigReload-
  DeferredDuringTurn).
- Providers (P3/P4): OpenAI provider streams SSE over httptest,
  sends the auth header, parses usage, backs off on server errors
  (TestOpenAIProviderStreamsSSE, TestOpenAIProviderSendsAuthHeader,
  TestOpenAIProviderParsesUsage, TestOpenAIProviderBacksOffOnServer-
  Error, TestOpenAIProviderSurfacesPersistentServerError).
- Observability (P5): structured JSON-lines logger to file,
  nil-logger safe, never stdout while the TUI is live
  (TestLogsStructuredLinesToFile, TestNilLoggerSafe).
- Race fixes found by the full -race run: MCP Client.state and
  the manager's per-server entry fields (err/tools/cached) are now
  mutex-guarded on every write; the engine's emit() drops events
  after Run() closes the event channel instead of panicking on a
  send to a closed channel (turn goroutines can outlive the loop).
- Removed lint-flagged dead code: Readiness.toEvents and
  Manager.nextCall.
- Full gates, all green 2026-10-08:
  go test ./... -race -count=1 (17 packages), go vet ./...,
  golangci-lint run (0 issues), FuzzSanitize / FuzzAnalyzeShell /
  FuzzParseSkill (10 s each), golden frame dumps at 50x16/80x24/
  120x38/160x45, PTY e2e (NIKI_PTY_TESTS=1: 6 tests), benchmarks.
- Measured vs the DESIGN.md §8 contract:
  cold start → first frame 49 ms (≤60 ms) ✓; cold start →
  interactive composer 54 ms (≤90 ms) ✓; input echo p50 0.06 ms /
  p95 0.10 ms over n=300 (≤30 ms) ✓; idle 0 redraws ✓ but CPU
  2.0% = 20 ms per 1 s (target <1%) ✗; render 53.4 µs @100 cells
  vs 55.9 µs @5000 cells, ratio 1.05 (≤1.5) ✓; 2000-iteration
  echo loop: heap 554 KB → 2941 KB, 18 GC cycles, max GC pause
  0.45 ms (budget 16 ms) ✓; stream burst 51 deltas → 1 repaint ✓.
- Two honest misses vs DESIGN.md §8 (owner decisions):
  1. Warm start → composer measured 49 ms vs the ≤25 ms target.
     The warm path still pays process spawn, Go runtime init and
     terminal.Detect. Options: accept 49 ms (it sits well inside
     the 90 ms cold budget), or fund boot-path trimming (lazy
     skills index, shorter detect timeout on fast-reply terminals).
     Consequence of accepting: the documented 25 ms target is
     missed; nothing else degrades.
  2. Idle CPU measured 2.0% vs the <1% target (0 redraws proven;
     the event loop and renderer goroutine still tick). Options:
     accept 2.0% (test gate is <5%), or tune the idle path
     (deeper event-loop sleep, renderer goroutine parking).
     Consequence of accepting: battery-sensitive hosts see a small
     constant drain while Niki is open.

## State
Phases 1–7 complete. All checklist rows WORKS except the two
measured misses above (B3, B5 PARTIAL), L1/S4 PARTIAL
(panic-path and SIGHUP restore bytes, real sandbox backends:
OWNER-VERIFY), and S1/S3/U8/A8 (real OS sandbox isolation,
disposable-env cleanup on panic, reduced motion, policy-input
hardening proof) UNVERIFIED. See CHECKLIST.md.

## 2026-10-07 — Extended pass
- Full output bounding: shell tool >4KB persists to temp file with truncation
  note; read_file capped at 2000 lines (A5).
- Args validation against per-tool minimal JSON schema (A4, TestSchemaValidation).
- mvdan.cc/sh fail-closed shell AST allowlist (A7).
- Registry.Batch: concurrent only when all calls report safe, cap 10,
  sequential fallback, order preserved (A2).
- MCP persisted tool-catalog cache + state test (M2 partial).
- Context compaction circuit breaker tripped at 3 failed compactions (X2).
- Subagent isolated context (test).
- contextwin package: static prefix / dynamic suffix / marked boundary + lint
  test (C6).
- Session: fsync on every event append, reopen-preserves-events (P2).
- TUI: footer/header collapses below 60 cols, `/` command menu, `@` file
  picker (U5/U6, tests), snapshot determinism at 50x16/80x24/120x38/160x45,
  docs/review frame dumps written.
- Sanitize untrusted rendering text; fuzz test (L6).
- Sandbox: SanitizedEnv scrubs AWS/SSH/token vars; ExecIsolated test (S2).
- engine emits real Usage on the turn (P4 partial).
- Fuzz targets: AnalyzeShell, parseSkill, Sanitize (all run clean).
- golangci-lint v2 config; errcheck fixed; LINT_OK.
- CI workflow (.github/workflows/ci.yml): vet, lint, race tests, PTY e2e,
  fuzz, benchmark compile. Release workflow: goreleaser on tags v*.
- Benchmarks: BenchmarkView80x24 ≈ 71µs, BenchmarkView120x38 ≈ 110µs
  (AMD Ryzen 7 4800H, 100 history items).
- Boot probe: `niki exec` ≈ 10 ms wall.

## 2026-10-07 — PTY e2e + measured probes
- TestPTYCodingLoop: types "hello", mock provider streams "Acknowledged: hello", Ctrl+C clean exit.
- Fixed intermittent 0x0 window-size (pty.Setsize) causing degenerate layout; all PTY tests now pass.
- TestNonTTYExec: no escapes; TestPTYRestoreOnCtrlC/SIGTERM: clean ≤5s exits.
- TestColdStartFirstFrame probe: 42 ms to first frame (target ≤60 ms). NOTE: terminals that never answer the DSR/OSC queries stall first frame for ~5 s (Bubble Tea v1 behavior); real terminals reply instantly.
- BenchmarkViewFlatness100vs5000: 58.7 µs (100 msgs) vs 59.5 µs (5000 msgs), ratio ~1.01 (target ratio ≤1.5).
- Engine.Observe added; TUI session events persisted to ~/.niki/sessions.db + JSONL (P1/P2 wiring).

## 2026-10-08 — Goals 1–4 Completion & Hardening
- **Goal 1 (Skeleton & Perf Rig)**:
  - Argv fast path in `cmd/niki/main.go` executes `--version` in 6.8 ms (vs Codex 20.0 ms, 2.93x faster).
  - Inittrace audit: all imported packages initialize in < 0.3 ms clock; 0 packages exceed 1 ms.
  - `niki config show --sources` outputs config values along with source layers.
  - `tools/ttff` measures TTFP (5.9 ms), Input-Ready (5.9 ms), Idle RSS (13.1 MB), Pre-prompt bytes (16 B). Saved in `docs/PERF.md` and `perf/budgets.toml`.
  - PTY smoke tests prove clean terminal restoration on Ctrl+C, SIGTERM, and normal exit.
- **Goal 2 (Live TUI & Safety)**:
  - Inline viewport with `tea.Println` scrollback, real event cells, live activity line, and event-driven mascot.
  - Keyboard handling, Ctrl+Z suspend, resize clamp, and reduced motion verified.
  - Unified slash command registry (`internal/tui/commands.go`) powering autocomplete and dispatch.
  - Sandbox re-exec helper (`niki sandbox-run`) enforcing read-only root, private `/tmp`, network namespace denial, and dropped capabilities via Bubblewrap.
  - Approvals focus safest option (`OptionDeny`) by default; `Esc` unconditionally denies; decision audit logging.
  - Untrusted project protection blocks registration of MCP servers.
  - Red-team test suite (`internal/permissions/redteam_test.go`) proves exfiltration traps, symlink escapes, and lifecycle traps are blocked.
  - `docs/SECURITY.md` documents security posture and boundaries.
- **Goal 3 (Extensibility & Reliability)**:
  - Boot with 5 MCP servers and 50 skills verified in 74 ms without blocking the critical path.
  - Cached skills discovery (`CachedDiscover`) with `.agents/skills` compat path: 724 µs cold vs 4.9 µs warm (148x speedup).
  - Scaffolding via `niki init`, blocking pre-tool hooks, named profiles, and automated/manual compaction golden tests.
  - Mid-turn kill -9 crash recovery verified in `internal/session/store_test.go`.
  - Anthropic and OpenAI Responses providers pass identical provider conformance tests: streaming, usage, 429/5xx retry backoff, and truncated stream error detection.
  - Native fuzz tests for SSE parser (213k execs), patch parser (401k execs), and JSON-RPC framing (294k execs) run clean with 0 panics.
- **Goal 4 (Performance Hardening & Dogfooding)**:
  - Final perf table against Codex, agy, and kimi saved in `docs/PERF.md`.
  - pprof CPU/memory profiling, runtime trace, inittrace audit, and PGO benchstat (-14.8% speedup) documented in `docs/PERF.md`.
  - Provider preconnect task runs in background after first frame with config/env opt-out.
  - `docs/ARCHITECTURE.md`, `CONFIG.md`, `docs/FEATURE_ATLAS.md`, `THIRD_PARTY.md`, and `docs/DOGFOOD.md` delivered.
  - Full gates passed: `golangci-lint run` (0 issues), `go vet ./...` (clean), `go test -race ./...` (17/17 packages pass).

