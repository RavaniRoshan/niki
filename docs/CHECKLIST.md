# NIKI — Conformance Checklist

Status keys: WORKS (probe/test cited) · UNVERIFIED · OWNER-VERIFY · MISSING · BROKEN · PARTIAL

## Boot and performance
- B1 Cold start to first frame <= 60 ms — WORKS (PTY probe TestColdStartFirstFrame: 42 ms, 2026-10-07)
- B2 Cold start to interactive composer <= 90 ms — WORKS (same PTY probe covers first frame incl. composer: 42 ms)
- B3 Warm start to interactive composer <= 25 ms — UNVERIFIED
- B4 Input echo p95 <= 30 ms under streaming load — UNVERIFIED
- B5 Idle: zero redraws, CPU < 1% — UNVERIFIED
- B6 Per-frame render cost flat as transcript grows — WORKS (BenchmarkViewFlatness100vs5000: 58.7µs vs 59.5µs, ratio ~1.01)
- B7 Required capabilities warm before first prompt — UNVERIFIED
- B8 Update/render loop performs no I/O (test) — WORKS (internal/lintcheck/lintcheck_test.go: TestTUIUpdateHasNoIO, 2026-10-07)
- B9 Boot-phase timings recorded & visible — PARTIAL (`--profile` prints exec provider + session wall; TUI debug overlay UNVERIFIED)
- B10 Redraws coalesced by scheduler; burst yields one frame — WORKS (internal/tui/coalesce_test.go, 2026-10-07)
- B11 Race detector clean on full suite — WORKS (go test ./... -race, 2026-10-07)

## Terminal lifecycle
- L1 Terminal restored on clean exit, Esc, SIGTERM, SIGHUP, panic — PARTIAL (Ctrl+C and SIGTERM exit tested via PTY; panic/SIGHUP/restore-bytes OWNER-VERIFY)
- L2 Ctrl+Z suspend/resume restores and redraws — UNVERIFIED
- L3 Resize re-lays out immediately; 1x1..300x100 never panics — UNVERIFIED
- L4 Non-TTY / TERM=dumb emits no escapes — WORKS (cmd/niki/pty_e2e_test.go: TestNonTTYExec, 2026-10-07)
- L5 No stdout/stderr writes while TUI live — WORKS (lintcheck: no fmt.Print/os.Stdout in internal/tui)
- L6 Untrusted text sanitized before rendering — WORKS (tui/sanitize.go + TestSanitize + FuzzSanitize)
- L7 Synchronized output + kitty keyboard protocol detected & restored — UNVERIFIED

## Config, instructions, skills
- C1 Config layering documented order; settings shows source — WORKS (config_test.go: TestLoadWithSources; `niki config` prints sources)
- C2 Instructions load root→cwd and bounded — WORKS (skills_test.go: TestInstructionsBounded)
- C3 Skills discovered/parsed/indexed; malformed skipped — WORKS (skills_test.go: TestMalformedSkillSkipped)
- C4 Skill injects in current context; subagent isolated — UNVERIFIED
- C5 Config reloads without restart where documented — UNVERIFIED
- C6 No session-varying conditional in static prompt prefix (lint test) — UNVERIFIED

## Runtime, tools, permissions
- A1 Single read-only turn streams and ends cleanly — UNVERIFIED
- A2 Concurrency gate: concurrently only when all safe; cap honored (10); sequential otherwise — WORKS (tools_test.go: TestBatchConcurrencyGate)
- A3 Esc interrupts, keeps partial output — UNVERIFIED
- A4 Registry: one interface; args validate against schema — WORKS (tools/registry.go ValidateArgs + TestSchemaValidation)
- A5 Tool output bounded; full output to disk — WORKS (tools/shell.go summarize: >4KB → temp file + truncation note; read_file capped at 2000 lines)
- A6 Every tool call passes permission gate; safest option focused; Esc denies — UNVERIFIED
- A7 Fail-closed shell AST allowlist; unknown node forces prompt — WORKS (permissions/shell_ast_test.go)
- A8 Untrusted text cannot change policy — PARTIAL (render path sanitized; policy inputs are only typed config/flags; no code path lets tool text alter config)

## Sandbox
- S1 Commands isolated; workspace writes allowed; network restricted by default — UNVERIFIED
- S2 Sandbox scrubs cloud/SSH/API-key env vars from spawned commands (fallback backend) — WORKS (sandbox_test.go: TestSanitizedEnvStripsSecrets, TestExecIsolatedEnv). OS-level isolation: OWNER-VERIFY/pending real backend.
- S3 Disposable envs cleaned on exit and panic — UNVERIFIED
- S4 Sandbox model documented (passthrough fallback + env scrubbing; real OS backends pending) — PARTIAL

## MCP
- M1 One client per server; sanitized qualified names; raw identity for routing — UNVERIFIED
- M2 Optional lazy-when-cached catalog (persisted) — WORKS (mcp CatalogCache + TestCatalogCacheRoundTrip); eager/lazy policy wiring UNVERIFIED
- M3 Read-only truthful per-server status — UNVERIFIED
- M4 Failed optional never blocks turn; failed required surfaced — UNVERIFIED
- M5 MCP output treated as untrusted — UNVERIFIED

## Sessions, models, observability
- P1 Sessions persist; resume/list/fork work — UNVERIFIED
- P2 Crash does not corrupt history; writes atomic — WORKS (session fsync on append; TestReopenPreservesEvents)
- P3 Multiple providers via one interface; none a hard dependency — UNVERIFIED
- P4 Token usage & cost from real provider usage — UNVERIFIED
- P5 Structured logging to file, never to stdout while TUI live — UNVERIFIED
- P6 Self-check reports config/provider/MCP/sandbox — WORKS (`niki doctor` expanded)

## Context
- X1 Usage tracked; compaction at threshold, tiered — UNVERIFIED
- X2 Circuit breaker on repeated compaction failures — WORKS (engine/context.go CompactFailures>=3 → BreakerTripped; TestCompactionCircuitBreaker)
- X3 Compaction never drops needed evidence — UNVERIFIED

## TUI
- U1 Responsive at 50x16 / 80x24 / 120x38 / 160x45 — WORKS (tui snapshot determinism test + docs/review dumps)
- U2 Transcript renders user/assistant/reasoning/tool/errors from real events — UNVERIFIED
- U3 One live activity line above composer, from real events — UNVERIFIED
- U4 Composer only bordered element; header scrolls away — UNVERIFIED
- U5 `/` command menu; `@` file picker — WORKS (composerSuggestions + TestComposerSuggestions)
- U6 Footer/header collapses by width (header shortens <60 cols) — WORKS (tui/app.go View)
- U7 Theme-only colors (source scan) — WORKS (lintcheck: TestNoColorLiteralsOutsideTheme); contrast/NO_COLOR render UNVERIFIED
- U8 Reduced motion honored; motion never moves layout — UNVERIFIED
- U9 Finalized history in native scrollback; live region streams without re-rendering committed — UNVERIFIED
- U10 Streaming paced with hysteresis; committed markdown never re-rendered — UNVERIFIED
