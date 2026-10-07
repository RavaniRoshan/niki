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

## State
Phases 1–3 partial; Phase 4 partial (A6 approval prompt, A5 full-output-to-disk,
A4 schema validation, A3 verified via PTY only). Remaining goals (P5 MCP live,
P7 taste review, P9 sessions crash-safety proof, perf benchmarks, debug view)
are UNVERIFIED. See CHECKLIST.md.

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
